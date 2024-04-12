use crate::{async_systems::AsyncWorldParam, async_world::AsyncWorldMut, signals::Signals, AsyncAccess};
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
use super::{AsyncQueryQueue, AsyncFailure, async_systems::AsyncEntityParam};

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

/// Async version of [`Query`]
#[derive(Debug, Clone)]
pub struct AsyncQuerySingle<T: QueryData, F: QueryFilter = ()> {
    pub(crate) queue: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<(T, F)>,
}


impl<T: QueryData, F: QueryFilter> AsyncQuery<T, F> {
    /// Obtain an [`AsyncEntityQuery`] on a specific entity.
    pub fn entity(&self, entity: impl Borrow<Entity>) -> AsyncEntityQuery<T, F> {
        AsyncEntityQuery {
            entity: *entity.borrow(),
            queue: self.queue.clone(),
            p: PhantomData,
        }
    }

    /// Obtain an [`AsyncQuerySingle`] on a single entity.
    pub fn single(&self) -> AsyncQuerySingle<T, F> {
        AsyncQuerySingle {
            queue: self.queue.clone(),
            p: PhantomData,
        }
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncQuery<T, F> {
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

impl<T: QueryData, F: QueryFilter> AsyncWorldParam for AsyncQuerySingle<T, F> {
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

/// Add method to [`AsyncQuerySingle`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncQuerySingleDeref: QueryData + Sized {
    type Target<F: QueryFilter>;
    fn async_deref<F: QueryFilter>(this: &AsyncQuerySingle<Self, F>) -> &Self::Target<F>;
}

impl<C, F> Deref for AsyncQuerySingle<C, F> where C: AsyncQuerySingleDeref, F: QueryFilter{
    type Target = <C as AsyncQuerySingleDeref>::Target<F>;

    fn deref(&self) -> &Self::Target {
        AsyncQuerySingleDeref::async_deref(self)
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

/// A resource that caches a [`QueryState`].
/// 
/// Since [`QueryState`] cannot be constructed from `&World`, readonly access is not supported.
#[derive(Debug, Resource)]
pub(crate) struct ResQueryCache<T: QueryData, F: QueryFilter>(pub QueryState<T, F>);

/// A [`World`] reference with a cached [`QueryState`].
/// 
/// Tries to obtain the [`QueryState`] from the [`World`] on create 
/// and stores the [`QueryState`] in the [`World`] on drop.
/// 
/// Since [`QueryState`] cannot be constructed from `&World`, readonly access is not supported.
pub struct OwnedQueryState<'t, D: QueryData + 'static, F: QueryFilter + 'static> {
    world: &'t mut World,
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
            .get_single(self.world).map_err(|e| match e {
                bevy_ecs::query::QuerySingleError::NoEntities(_) => AsyncFailure::EntityNotFound,
                bevy_ecs::query::QuerySingleError::MultipleEntities(_) => AsyncFailure::TooManyEntities,
            })
    }

    pub fn single_mut(&mut self) -> Result<D::Item<'_>, AsyncFailure> {
        self.state.as_mut().unwrap()
            .get_single_mut(self.world).map_err(|e| match e {
                bevy_ecs::query::QuerySingleError::NoEntities(_) => AsyncFailure::EntityNotFound,
                bevy_ecs::query::QuerySingleError::MultipleEntities(_) => AsyncFailure::TooManyEntities,
            })
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