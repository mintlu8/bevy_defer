use crate::reactors::Reactors;
use crate::{async_systems::AsyncEntityParam, AccessError};
use crate::{async_systems::AsyncWorldParam, executor::with_world_mut, signals::Signals};
use bevy::ecs::query::QuerySingleError;
#[allow(unused)]
use bevy::ecs::system::Query;
use bevy::ecs::world::CommandQueue;
use bevy::ecs::{
    entity::Entity,
    query::{QueryData, QueryFilter, QueryIter, QueryManyIter, QueryState, WorldQuery},
    system::Resource,
    world::World,
};
use std::any::type_name;
use std::{borrow::Borrow, marker::PhantomData, ops::Deref};

use super::AsyncEntityMut;

/// Async version of [`Query`]
#[derive(Debug)]
pub struct AsyncQuery<T: QueryData, F: QueryFilter = ()>(pub(crate) PhantomData<(T, F)>);

impl<T: QueryData, F: QueryFilter> Copy for AsyncQuery<T, F> {}

impl<T: QueryData, F: QueryFilter> Clone for AsyncQuery<T, F> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Async version of [`Query`] on a specific entity.
#[derive(Debug)]
pub struct AsyncEntityQuery<T: QueryData, F: QueryFilter = ()> {
    pub(crate) entity: Entity,
    pub(crate) p: PhantomData<(T, F)>,
}

impl<T: QueryData, F: QueryFilter> AsyncEntityQuery<T, F> {
    pub fn entity(&self) -> AsyncEntityMut {
        AsyncEntityMut(self.entity)
    }
}

impl<T: QueryData, F: QueryFilter> Copy for AsyncEntityQuery<T, F> {}

impl<T: QueryData, F: QueryFilter> Clone for AsyncEntityQuery<T, F> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Async version of [`Query`] on a unique entity.
#[derive(Debug)]
pub struct AsyncQuerySingle<T: QueryData, F: QueryFilter = ()>(pub(crate) PhantomData<(T, F)>);

impl<T: QueryData, F: QueryFilter> Copy for AsyncQuerySingle<T, F> {}

impl<T: QueryData, F: QueryFilter> Clone for AsyncQuerySingle<T, F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: QueryData, F: QueryFilter> AsyncQuery<T, F> {
    /// Obtain an [`AsyncEntityQuery`] on a specific entity.
    pub fn entity(&self, entity: impl Borrow<Entity>) -> AsyncEntityQuery<T, F> {
        AsyncEntityQuery {
            entity: *entity.borrow(),
            p: PhantomData,
        }
    }

    /// Obtain an [`AsyncQuerySingle`] on a single entity.
    pub fn single(&self) -> AsyncQuerySingle<T, F> {
        AsyncQuerySingle(PhantomData)
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncQuery<T, F> {
    /// Run a function on the iterator.
    pub fn for_each(&self, mut f: impl FnMut(T::Item<'_>) + 'static) {
        with_world_mut(move |w| {
            let mut state = OwnedQueryState::<T, F>::new(w);
            for item in state.iter_mut() {
                f(item);
            }
        })
    }
}

impl<T: QueryData, F: QueryFilter> AsyncWorldParam for AsyncQuery<T, F> {
    fn from_async_context(_: &Reactors) -> Option<Self> {
        Some(Self(PhantomData))
    }
}

impl<T: QueryData, F: QueryFilter> AsyncWorldParam for AsyncQuerySingle<T, F> {
    fn from_async_context(_: &Reactors) -> Option<Self> {
        Some(Self(PhantomData))
    }
}

impl<T: QueryData, F: QueryFilter> AsyncEntityParam for AsyncEntityQuery<T, F> {
    type Signal = ();

    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(entity: Entity, _: &Reactors, _: (), _: &[Entity]) -> Option<Self> {
        Some(Self {
            entity,
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

impl<C, F> Deref for AsyncQuery<C, F>
where
    C: AsyncQueryDeref,
    F: QueryFilter,
{
    type Target = <C as AsyncQueryDeref>::Target<F>;

    fn deref(&self) -> &Self::Target {
        AsyncQueryDeref::async_deref(self)
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

impl<D: QueryData + 'static, F: QueryFilter + 'static> OwnedQueryState<'_, D, F> {
    /// Apply a command queue to world.
    pub fn apply_commands(&mut self, commands: &mut CommandQueue) {
        commands.apply(self.world)
    }
}

impl<D: QueryData + 'static, F: QueryFilter + 'static> OwnedQueryState<'_, D, F> {
    pub fn new(world: &mut World) -> OwnedQueryState<D, F> {
        OwnedQueryState {
            state: match world.remove_resource::<ResQueryCache<D, F>>() {
                Some(item) => Some(item.0),
                None => Some(QueryState::new(world)),
            },
            world,
        }
    }

    pub fn single(&mut self) -> Result<<D::ReadOnly as WorldQuery>::Item<'_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .get_single(self.world)
            .map_err(|e| match e {
                QuerySingleError::NoEntities(_) => AccessError::NoEntityFound {
                    query: type_name::<D>(),
                },
                QuerySingleError::MultipleEntities(_) => AccessError::TooManyEntities {
                    query: type_name::<D>(),
                },
            })
    }

    pub fn single_mut(&mut self) -> Result<D::Item<'_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .get_single_mut(self.world)
            .map_err(|e| match e {
                QuerySingleError::NoEntities(_) => AccessError::NoEntityFound {
                    query: type_name::<D>(),
                },
                QuerySingleError::MultipleEntities(_) => AccessError::TooManyEntities {
                    query: type_name::<D>(),
                },
            })
    }

    pub fn get(
        &mut self,
        entity: Entity,
    ) -> Result<<D::ReadOnly as WorldQuery>::Item<'_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .get(self.world, entity)
            .map_err(|_| AccessError::EntityNotFound(entity))
    }

    pub fn get_mut(&mut self, entity: Entity) -> Result<D::Item<'_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .get_mut(self.world, entity)
            .map_err(|_| AccessError::EntityNotFound(entity))
    }

    pub fn iter_many<E: IntoIterator>(
        &mut self,
        entities: E,
    ) -> QueryManyIter<'_, '_, D::ReadOnly, F, E::IntoIter>
    where
        E::Item: Borrow<Entity>,
    {
        self.state.as_mut().unwrap().iter_many(self.world, entities)
    }

    pub fn iter_many_mut<E: IntoIterator>(
        &mut self,
        entities: E,
    ) -> QueryManyIter<'_, '_, D, F, E::IntoIter>
    where
        E::Item: Borrow<Entity>,
    {
        self.state
            .as_mut()
            .unwrap()
            .iter_many_mut(self.world, entities)
    }

    pub fn iter(&mut self) -> QueryIter<D::ReadOnly, F> {
        self.state.as_mut().unwrap().iter(self.world)
    }

    pub fn iter_mut(&mut self) -> QueryIter<D, F> {
        self.state.as_mut().unwrap().iter_mut(self.world)
    }
}

impl<'s, D: QueryData + 'static, F: QueryFilter + 'static> IntoIterator
    for &'s mut OwnedQueryState<'_, D, F>
{
    type Item = D::Item<'s>;
    type IntoIter = QueryIter<'s, 's, D, F>;

    fn into_iter(self) -> Self::IntoIter {
        self.state.as_mut().unwrap().iter_mut(self.world)
    }
}

impl<D: QueryData + 'static, F: QueryFilter + 'static> Drop for OwnedQueryState<'_, D, F> {
    fn drop(&mut self) {
        self.world
            .insert_resource(ResQueryCache(self.state.take().unwrap()))
    }
}
