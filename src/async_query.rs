use crate::{signals::Signals, BoxedQueryCallback, CHANNEL_CLOSED};
use bevy_ecs::{
    entity::Entity,
    query::{QueryData, QueryFilter, QueryIter, QueryState, WorldQuery},
    system::Resource,
    world::World,
};
#[allow(unused)]
use bevy_ecs::system::Query;
use std::{
    borrow::{Borrow, Cow}, future::Future, marker::PhantomData, ops::Deref
};
use triomphe::Arc;
use futures::channel::oneshot::channel;
use super::{AsyncQueryQueue, AsyncFailure, AsyncResult, AsyncEntityParam};

#[derive(Debug, Resource)]
struct ResQueryCache<T: QueryData, F: QueryFilter>(QueryState<T, F>);

/// Async version of [`Query`]
pub struct AsyncQuery<'t, T: QueryData + 't, F: QueryFilter + 't = ()> {
    pub(crate) executor: Cow<'t, Arc<AsyncQueryQueue>>,
    pub(crate) p: PhantomData<(T, F)>,
}

/// Async version of [`Query`] on a single entity.
pub struct AsyncEntityQuery<'t, T: QueryData + 't, F: QueryFilter + 't = ()> {
    pub(crate) entity: Entity,
    pub(crate) executor: Cow<'t, Arc<AsyncQueryQueue>>,
    pub(crate) p: PhantomData<(T, F)>,
}

/// Safety: Safe since `T` and `F` are markers.
unsafe impl<T: QueryData, F: QueryFilter> Send for AsyncQuery<'_, T, F> {}
/// Safety: Safe since `T` and `F` are markers.
unsafe impl<T: QueryData, F: QueryFilter> Sync for AsyncQuery<'_, T, F> {}

/// Safety: Safe since `T` and `F` are markers.
unsafe impl<T: QueryData, F: QueryFilter> Send for AsyncEntityQuery<'_, T, F> {}
/// Safety: Safe since `T` and `F` are markers.
unsafe impl<T: QueryData, F: QueryFilter> Sync for AsyncEntityQuery<'_, T, F> {}

impl<T: QueryData, F: QueryFilter> AsyncQuery<'_, T, F> {
    pub fn entity(&self, entity: impl Borrow<Entity>) -> AsyncEntityQuery<T, F> {
        AsyncEntityQuery {
            entity: *entity.borrow(),
            executor: self.executor.clone(),
            p: PhantomData,
        }
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncQuery<'_, T, F> {
    /// Run a function on the iterator.
    pub fn for_each (
        &self,
        f: impl FnMut(T::Item<'_>) + Send + Sync + 'static,
    ) -> impl Future<Output = ()> + 'static {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::once(
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
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    /// Run a function on the iterator returned by [`Query`] and obtain the result.
    pub fn iter<U: Send + Sync + 'static> (
        &self,
        f: impl FnOnce(QueryIter<'_, '_, T, F>) -> U + Send + Sync + 'static,
    ) -> impl Future<Output = U> + 'static {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::once(
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
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }
}

impl<'t, T: QueryData, F: QueryFilter> AsyncEntityParam<'t> for AsyncEntityQuery<'_, T, F> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(entity: Entity, executor: &Arc<AsyncQueryQueue>, _: ()) -> Self {
        Self {
            entity,
            executor: Cow::Owned(executor.clone()),
            p: PhantomData,
        }
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncEntityQuery<'_, T, F> {
    /// Run a function on the [`Query`] and obtain the result.
    pub fn run<Out: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(T::Item<'_>) -> Out + Send + Sync + 'static,
    ) -> impl Future<Output = AsyncResult<Out>> + 'static {
        let (sender, receiver) = channel();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
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
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            match receiver.await {
                Ok(Some(out)) => Ok(out),
                Ok(None) => Err(AsyncFailure::ComponentNotFound),
                Err(_) => Err(AsyncFailure::ChannelClosed),
            }
        }
    }

    /// Run a repeatable function on the [`Query`] and obtain the result once [`Some`] is returned.
    pub fn watch<U: Send + Sync + 'static>(
        &self,
        mut f: impl FnMut(<T as WorldQuery>::Item<'_>) -> Option<U> + Send + Sync + 'static,
    ) -> impl Future<Output = U> + 'static {
        let (sender, receiver) = channel();
        let entity = self.entity;
        let query = BoxedQueryCallback::repeat(
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
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }
}

/// Add method to [`AsyncQuery`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncQueryDeref: QueryData + Sized {
    type Target<'t, F: QueryFilter> where Self: 't, F: 't;
    fn async_deref<'a, 'b, F: QueryFilter>(this: &'b AsyncQuery<'a, Self, F>) -> &'b Self::Target<'a, F>;
}

impl<'t, C, F> Deref for AsyncQuery<'t, C, F> where C: AsyncQueryDeref, F: QueryFilter{
    type Target = <C as AsyncQueryDeref>::Target<'t, F>;

    fn deref(&self) -> &Self::Target {
        AsyncQueryDeref::async_deref(self)
    }
}

/// Add method to [`AsyncEntityQuery`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncEntityQueryDeref: QueryData + Sized {
    type Target<'t, F: QueryFilter> where Self: 't, F: 't;
    fn async_deref<'a, 'b, F: QueryFilter>(this: &'b AsyncEntityQuery<'a, Self, F>) -> &'b Self::Target<'a, F>;
}

impl<'t, C, F> Deref for AsyncEntityQuery<'t, C, F> where C: AsyncEntityQueryDeref, F: QueryFilter{
    type Target = <C as AsyncEntityQueryDeref>::Target<'t, F>;

    fn deref(&self) -> &Self::Target {
        AsyncEntityQueryDeref::async_deref(self)
    }
}
