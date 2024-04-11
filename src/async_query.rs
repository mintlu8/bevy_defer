use crate::{async_systems::AsyncWorldParam, async_world::AsyncWorldMut, signals::Signals, AsyncAccess, CHANNEL_CLOSED};
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
    pub(crate) queue: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<(T, F)>,
}

/// Async version of [`Query`] on a single entity.
#[derive(Debug, Clone)]
pub struct AsyncEntityQuery<T: QueryData, F: QueryFilter = ()> {
    pub(crate) entity: Entity,
    pub(crate) queue: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<(T, F)>,
}

impl<T: QueryData, F: QueryFilter> AsyncQuery<T, F> {
    pub fn entity(&self, entity: impl Borrow<Entity>) -> AsyncEntityQuery<T, F> {
        AsyncEntityQuery {
            entity: *entity.borrow(),
            queue: self.queue.clone(),
            p: PhantomData,
        }
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncQuery<T, F> {
    /// Try run a function on a singleton query.
    pub fn single<U: 'static> (
        &self,
        f: impl FnOnce(T::Item<'_>) -> U + 'static,
    ) -> impl Future<Output = AsyncResult<U>> + 'static {
        self.world().run(|w| Ok(f(OwnedQueryState::<T, F>::new(w).single_mut()?)))
    }
    
    /// Run a function on the iterator.
    pub fn for_each (
        &self,
        mut f: impl FnMut(T::Item<'_>) + 'static,
    ) -> impl Future<Output = ()> + 'static {
        self.world().run(move |w| {
            let mut state = OwnedQueryState::<T, F>::new(w);
            for item in state.iter_mut(){
                f(item);
            }
        })
    }
}

impl<T: QueryData, F: QueryFilter> AsyncWorldParam for AsyncQuery<T, F> {
    fn from_async_context(executor: &AsyncWorldMut) -> Option<Self> {
        Some(Self {
            queue: executor.queue.clone(),
            p: PhantomData,
        })
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
            queue: executor.queue.clone(),
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
        self.queue.once(
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
        self.queue.repeat(
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

/// A [`World`] reference and a cached [`QueryState`].
/// 
/// Stores the [`QueryState`] in the [`World`] on drop.
pub struct OwnedQueryState<'t, D: QueryData + 'static, F: QueryFilter + 'static> {
    world: &'t mut World,
    state: Option<QueryState<D, F>>,
}

/// A [`World`] reference and a cached [`QueryState`].
/// 
/// Stores the [`QueryState`] in the [`World`] on drop.
pub struct OwnedQuerySingle<'t, D: QueryData + 'static, F: QueryFilter + 'static> {
    world: D::Item<'t>,
    state: Option<QueryState<D, F>>,
}

impl<'t, D: QueryData + 'static, F: QueryFilter + 'static> OwnedQueryState<'t, D, F> {
    pub fn new(world: &mut World) -> OwnedQueryState<D, F> {
        OwnedQueryState {
            state: match world.remove_resource::<ResQueryCache<D, F>>() {
                Some(item) => Some(item.0),
                None => Some(QueryState::new(world)),
            },
            world,
        }
    }

    pub fn single(&mut self) -> Result<<D::ReadOnly as WorldQuery>::Item<'_>, AsyncFailure> {
        self.state.as_mut().unwrap()
            .get_single(self.world).map_err(|_|AsyncFailure::EntityNotFound)
    }

    pub fn single_mut(&mut self) -> Result<D::Item<'_>, AsyncFailure> {
        self.state.as_mut().unwrap()
            .get_single_mut(self.world).map_err(|_|AsyncFailure::EntityNotFound)
    }

    pub fn get(&mut self, entity: Entity) -> Result<<D::ReadOnly as WorldQuery>::Item<'_>, AsyncFailure> {
        self.state.as_mut().unwrap()
            .get(self.world, entity).map_err(|_|AsyncFailure::EntityNotFound)
    }

    pub fn get_mut(&mut self, entity: Entity) -> Result<D::Item<'_>, AsyncFailure> {
        self.state.as_mut().unwrap()
            .get_mut(self.world, entity).map_err(|_|AsyncFailure::EntityNotFound)
    }

    pub fn iter<'s>(&'s mut self) -> QueryIter<'s, 's, D::ReadOnly, F> {
        self.state.as_mut().unwrap().iter(self.world)
    }

    pub fn iter_mut<'s>(&'s mut self) -> QueryIter<'s, 's, D, F>  {
        self.state.as_mut().unwrap().iter_mut(self.world)
    }
}

impl<'w, 's, D: QueryData + 'static, F: QueryFilter + 'static> IntoIterator for &'s mut OwnedQueryState<'w, D, F> {
    type Item = D::Item<'s>;
    type IntoIter = QueryIter<'s, 's, D, F>;

    fn into_iter(self) -> Self::IntoIter {
        self.state.as_mut().unwrap().iter_mut(self.world)
    }
}

impl<'t, D: QueryData + 'static, F: QueryFilter + 'static> Drop for OwnedQueryState<'t, D, F> {
    fn drop(&mut self) {
        self.world.insert_resource(ResQueryCache(self.state.take().unwrap()))
    }
}