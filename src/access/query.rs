use std::{
    any::{type_name, Any, TypeId},
    cell::RefCell,
};

use bevy::ecs::{
    entity::{Entity, EntityEquivalent},
    query::{
        QueryData, QueryFilter, QueryIter, QueryManyIter, QuerySingleError, QueryState,
        ReadOnlyQueryData,
    },
    world::{CommandQueue, World},
};
use rustc_hash::FxHashMap;

use crate::AccessError;

/// A resource that caches a [`QueryState`].
///
/// Since [`QueryState`] cannot be constructed from `&World`, readonly access is not supported.
#[derive(Debug, Default)]
pub(crate) struct QueryCache(RefCell<FxHashMap<(TypeId, TypeId), Box<dyn Any>>>);

impl QueryCache {
    pub fn take_query_state<D: QueryData + 'static, F: QueryFilter + 'static>(
        &self,
    ) -> Option<Box<QueryState<D, F>>> {
        let mut guard = self.0.borrow_mut();
        guard
            .remove(&(TypeId::of::<D>(), TypeId::of::<F>()))
            .and_then(|x| x.downcast().ok())
    }

    pub fn push_query_state<D: QueryData + 'static, F: QueryFilter + 'static>(
        &self,
        state: Box<QueryState<D, F>>,
    ) {
        let mut guard = self.0.borrow_mut();
        guard.insert((TypeId::of::<D>(), TypeId::of::<F>()), state);
    }
}

/// A [`World`] reference with a cached [`QueryState`].
///
/// Tries to obtain the [`QueryState`] from the [`World`] on create
/// and caches the [`QueryState`] in the [`World`] on drop.
pub struct OwnedQueryState<'t, D: QueryData + 'static, F: QueryFilter + 'static> {
    pub(crate) world: &'t mut World,
    pub(crate) state: Option<Box<QueryState<D, F>>>,
}

impl<D: QueryData + 'static, F: QueryFilter + 'static> OwnedQueryState<'_, D, F> {
    /// Apply a command queue to world.
    pub fn apply_commands(&mut self, commands: &mut CommandQueue) {
        commands.apply(self.world)
    }
}

impl<D: QueryData + 'static, F: QueryFilter + 'static> OwnedQueryState<'_, D, F> {
    pub fn new(world: &mut World) -> OwnedQueryState<'_, D, F> {
        OwnedQueryState {
            state: match world
                .non_send_resource::<QueryCache>()
                .take_query_state::<D, F>()
            {
                Some(item) => Some(item),
                None => Some(Box::new(QueryState::new(world))),
            },
            world,
        }
    }

    pub fn single(&mut self) -> Result<<D::ReadOnly as QueryData>::Item<'_, '_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .single(self.world)
            .map_err(|e| match e {
                QuerySingleError::NoEntities(_) => AccessError::NoEntityFound {
                    query: type_name::<D>(),
                },
                QuerySingleError::MultipleEntities(_) => AccessError::TooManyEntities {
                    query: type_name::<D>(),
                },
            })
    }

    pub fn single_mut(&mut self) -> Result<D::Item<'_, '_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .single_mut(self.world)
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
    ) -> Result<<D::ReadOnly as QueryData>::Item<'_, '_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .get(self.world, entity)
            .map_err(|_| AccessError::QueryConditionNotMet {
                entity,
                query: type_name::<(D, F)>(),
            })
    }

    pub fn get_mut(&mut self, entity: Entity) -> Result<D::Item<'_, '_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .get_mut(self.world, entity)
            .map_err(|_| AccessError::QueryConditionNotMet {
                entity,
                query: type_name::<(D, F)>(),
            })
    }

    pub fn iter_many<E: IntoIterator<Item: EntityEquivalent>>(
        &mut self,
        entities: E,
    ) -> QueryManyIter<'_, '_, D::ReadOnly, F, E::IntoIter> {
        self.state.as_mut().unwrap().iter_many(self.world, entities)
    }

    pub fn iter_many_mut<E: IntoIterator<Item: EntityEquivalent>>(
        &mut self,
        entities: E,
    ) -> QueryManyIter<'_, '_, D, F, E::IntoIter> {
        self.state
            .as_mut()
            .unwrap()
            .iter_many_mut(self.world, entities)
    }

    pub fn iter(&mut self) -> QueryIter<'_, '_, D::ReadOnly, F> {
        self.state.as_mut().unwrap().iter(self.world)
    }

    pub fn iter_mut(&mut self) -> QueryIter<'_, '_, D, F> {
        self.state.as_mut().unwrap().iter_mut(self.world)
    }
}

impl<'s, D: QueryData + 'static, F: QueryFilter + 'static> IntoIterator
    for &'s mut OwnedQueryState<'_, D, F>
{
    type Item = D::Item<'s, 's>;
    type IntoIter = QueryIter<'s, 's, D, F>;

    fn into_iter(self) -> Self::IntoIter {
        self.state.as_mut().unwrap().iter_mut(self.world)
    }
}

impl<D: QueryData + 'static, F: QueryFilter + 'static> Drop for OwnedQueryState<'_, D, F> {
    fn drop(&mut self) {
        self.world
            .non_send_resource::<QueryCache>()
            .push_query_state(self.state.take().unwrap());
    }
}

/// A readonly [`World`] reference with a cached [`QueryState`].
///
/// # Limitation
///
/// Currently creates a new `QueryState` every time.
pub struct OwnedReadonlyQueryState<'t, D: ReadOnlyQueryData + 'static, F: QueryFilter + 'static> {
    pub(crate) world: &'t World,
    pub(crate) state: Option<Box<QueryState<D, F>>>,
}

impl<D: ReadOnlyQueryData + 'static, F: QueryFilter + 'static> OwnedReadonlyQueryState<'_, D, F> {
    pub fn new(world: &World) -> OwnedReadonlyQueryState<'_, D, F> {
        OwnedReadonlyQueryState {
            state: match world
                .non_send_resource::<QueryCache>()
                .take_query_state::<D, F>()
            {
                Some(item) => Some(item),
                None => QueryState::try_new(world).map(Box::new),
            },
            world,
        }
    }

    pub fn single(&mut self) -> Result<D::Item<'_, '_>, AccessError> {
        self.state
            .as_mut()
            .ok_or(AccessError::NoEntityFound {
                query: type_name::<D>(),
            })?
            .single(self.world)
            .map_err(|e| match e {
                QuerySingleError::NoEntities(_) => AccessError::NoEntityFound {
                    query: type_name::<D>(),
                },
                QuerySingleError::MultipleEntities(_) => AccessError::TooManyEntities {
                    query: type_name::<D>(),
                },
            })
    }

    pub fn get(&mut self, entity: Entity) -> Result<D::Item<'_, '_>, AccessError> {
        self.state
            .as_mut()
            .unwrap()
            .get(self.world, entity)
            .map_err(|_| AccessError::QueryConditionNotMet {
                entity,
                query: type_name::<(D, F)>(),
            })
    }

    pub fn iter_many<E: IntoIterator<Item: EntityEquivalent>>(
        &mut self,
        entities: E,
    ) -> QueryManyIter<'_, '_, D, F, E::IntoIter> {
        self.state.as_mut().unwrap().iter_many(self.world, entities)
    }

    pub fn iter(&mut self) -> QueryIter<'_, '_, D::ReadOnly, F> {
        self.state.as_mut().unwrap().iter(self.world)
    }
}

impl<'s, D: ReadOnlyQueryData + 'static, F: QueryFilter + 'static> IntoIterator
    for &'s mut OwnedReadonlyQueryState<'_, D, F>
{
    type Item = D::Item<'s, 's>;
    type IntoIter = QueryIter<'s, 's, D, F>;

    fn into_iter(self) -> Self::IntoIter {
        self.state.as_mut().unwrap().iter(self.world)
    }
}

impl<D: ReadOnlyQueryData + 'static, F: QueryFilter + 'static> Drop
    for OwnedReadonlyQueryState<'_, D, F>
{
    fn drop(&mut self) {
        self.world
            .non_send_resource::<QueryCache>()
            .push_query_state(self.state.take().unwrap());
    }
}
