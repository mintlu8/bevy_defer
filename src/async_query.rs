use crate::{async_world::AsyncWorldMut, signals::Signals, QueryCallback, CHANNEL_CLOSED};
use bevy_ecs::{
    entity::Entity,
    query::{QueryData, QueryFilter, QueryIter, QueryState, WorldQuery},
    system::Resource,
    world::World,
};
#[allow(unused)]
use bevy_ecs::system::Query;
use std::{
    borrow::Borrow, future::Future, marker::PhantomData, ops::Deref, rc::Rc
};
use crate::channels::channel;
use super::{AsyncQueryQueue, AsyncFailure, AsyncResult, async_systems::AsyncEntityParam};
use futures::FutureExt;
#[derive(Debug, Resource)]
pub(crate) struct ResQueryCache<T: QueryData, F: QueryFilter>(pub QueryState<T, F>);

/// Async version of [`Query`]
#[derive(Debug, Clone)]
pub struct AsyncQuery<T: QueryData, F: QueryFilter = ()> {
    pub(crate) executor: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<(T, F)>,
}

/// Async version of [`Query`] on a single entity.
#[derive(Debug, Clone)]
pub struct AsyncEntityQuery<T: QueryData, F: QueryFilter = ()> {
    pub(crate) entity: Entity,
    pub(crate) executor: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<(T, F)>,
}

impl<T: QueryData, F: QueryFilter> AsyncQuery<T, F> {
    pub fn entity(&self, entity: impl Borrow<Entity>) -> AsyncEntityQuery<T, F> {
        AsyncEntityQuery {
            entity: *entity.borrow(),
            executor: self.executor.clone(),
            p: PhantomData,
        }
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncQuery<T, F> {
    
    /// Run a function on the iterator.
    pub fn for_each (
        &self,
        f: impl FnMut(T::Item<'_>) + 'static,
    ) -> impl Future<Output = ()> + 'static {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
            move |world: &mut World| match world.remove_resource::<ResQueryCache<T, F>>() {
                Some(mut state) => {
                    state.0.iter_mut(world).for_each(f);
                    world.insert_resource(state);
                }
                None => {
                    let mut state = ResQueryCache(world.query_filtered::<T, F>());
                    state.0.iter_mut(world).for_each(f);
                    world.insert_resource(state);
                }
            },
            sender,
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a function on the iterator returned by [`Query`] and obtain the result.
    pub fn iter<U: 'static> (
        &self,
        f: impl FnOnce(QueryIter<'_, '_, T, F>) -> U + 'static,
    ) -> impl Future<Output = U> + 'static {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
            move |world: &mut World| match world.remove_resource::<ResQueryCache<T, F>>() {
                Some(mut state) => {
                    let value = f(state.0.iter_mut(world));
                    world.insert_resource(state);
                    value
                }
                None => {
                    let mut state = ResQueryCache(world.query_filtered::<T, F>());
                    let value = f(state.0.iter_mut(world));
                    world.insert_resource(state);
                    value
                }
            },
            sender,
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}

impl<T: QueryData, F: QueryFilter> AsyncEntityParam for AsyncEntityQuery<T, F> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(entity: Entity, executor: &AsyncWorldMut, _: (), _: &[Entity]) -> Option<Self> {
        Some(Self {
            entity,
            executor: executor.queue.clone(),
            p: PhantomData,
        })
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncEntityQuery<T, F> {
    /// Run a function on the [`Query`] and obtain the result.
    pub fn run<Out: 'static>(
        &self,
        f: impl FnOnce(T::Item<'_>) -> Out + 'static,
    ) -> impl Future<Output = AsyncResult<Out>> + 'static {
        let (sender, receiver) = channel();
        let entity = self.entity;
        let query = QueryCallback::once(
            move |world: &mut World| match world.remove_resource::<ResQueryCache<T, F>>() {
                Some(mut state) => {
                    let result = f(state.0.get_mut(world, entity).ok()?);
                    world.insert_resource(state);
                    Some(result)
                }
                None => {
                    let mut state = ResQueryCache(world.query_filtered::<T, F>());
                    let result = f(state.0.get_mut(world, entity).ok()?);
                    world.insert_resource(state);
                    Some(result)
                }
            },
            sender,
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|r| {
            match r {
                Ok(Some(out)) => Ok(out),
                Ok(None) => Err(AsyncFailure::ComponentNotFound),
                Err(_) => Err(AsyncFailure::ChannelClosed),
            }
        })
    }

    /// Run a repeatable function on the [`Query`] and obtain the result once [`Some`] is returned.
    pub fn watch<U: 'static>(
        &self,
        mut f: impl FnMut(<T as WorldQuery>::Item<'_>) -> Option<U> + 'static,
    ) -> impl Future<Output = U> + 'static {
        let (sender, receiver) = channel();
        let entity = self.entity;
        let query = QueryCallback::repeat(
            move |world: &mut World| match world.remove_resource::<ResQueryCache<T, F>>() {
                Some(mut state) => {
                    let result = f(state.0.get_mut(world, entity).ok()?);
                    world.insert_resource(state);
                    result
                }
                None => {
                    let mut state = ResQueryCache(world.query_filtered::<T, F>());
                    let result = f(state.0.get_mut(world, entity).ok()?);
                    world.insert_resource(state);
                    result
                }
            },
            sender,
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}

/// Add method to [`AsyncQuery`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncQueryDeref: QueryData + Sized {
    type Target<F: QueryFilter>;
    fn async_deref<F: QueryFilter>(this: &AsyncQuery<Self, F>) -> &Self::Target<F>;
}

impl<C, F> Deref for AsyncQuery<C, F> where C: AsyncQueryDeref, F: QueryFilter{
    type Target = <C as AsyncQueryDeref>::Target<F>;

    fn deref(&self) -> &Self::Target {
        AsyncQueryDeref::async_deref(self)
    }
}

/// Add method to [`AsyncEntityQuery`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncEntityQueryDeref: QueryData + Sized {
    type Target<F: QueryFilter>;
    fn async_deref<F: QueryFilter>(this: &AsyncEntityQuery<Self, F>) -> &Self::Target<F>;
}

impl<C, F> Deref for AsyncEntityQuery<C, F> where C: AsyncEntityQueryDeref, F: QueryFilter{
    type Target = <C as AsyncEntityQueryDeref>::Target<F>;

    fn deref(&self) -> &Self::Target {
        AsyncEntityQueryDeref::async_deref(self)
    }
}
