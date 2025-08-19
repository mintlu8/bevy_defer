use std::marker::PhantomData;

use bevy::ecs::{
    entity::Entity,
    hierarchy::Children,
    query::{QueryData, QueryFilter, QueryManyIter, QueryState},
    relationship::{RelationshipSourceCollection, RelationshipTarget},
    world::{unsafe_world_cell::UnsafeWorldCell, World},
};

use crate::{AccessError, AccessResult, AsyncAccess, OwnedQueryState};

use super::{traits::AsyncMutAccess, AsyncEntityMut};

#[derive(Debug, Clone, Copy)]
pub struct AsyncRelatedQuery<R: RelationshipTarget, D: QueryData, F: QueryFilter> {
    entity: Entity,
    p: PhantomData<(R, D, F)>,
}

impl AsyncEntityMut {
    pub fn query_children<D: QueryData, F: QueryFilter>(
        &self,
    ) -> AsyncRelatedQuery<Children, D, F> {
        AsyncRelatedQuery {
            entity: self.id(),
            p: PhantomData,
        }
    }

    pub fn query_related<R: RelationshipTarget, D: QueryData, F: QueryFilter>(
        &self,
    ) -> AsyncRelatedQuery<R, D, F> {
        AsyncRelatedQuery {
            entity: self.id(),
            p: PhantomData,
        }
    }
}

pub struct RelatedQueryState<'t, R: RelationshipTarget, D: QueryData, F: QueryFilter> {
    world: UnsafeWorldCell<'t>,
    query: &'t mut QueryState<D, F>,
    parent: Entity,
    p: PhantomData<R>,
}

impl<R: RelationshipTarget, D: QueryData + 'static, F: QueryFilter + 'static> AsyncAccess
    for AsyncRelatedQuery<R, D, F>
{
    type Cx = Entity;

    type RefMutCx<'t> = OwnedQueryState<'t, D, F>;

    type Ref<'t> = RelatedQueryState<'t, R, D::ReadOnly, F>;

    type RefMut<'t> = RelatedQueryState<'t, R, D, F>;

    fn as_cx(&self) -> Self::Cx {
        self.entity
    }

    fn from_mut_cx<'t>(
        mut_cx: &'t mut Self::RefMutCx<'_>,
        cx: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>> {
        if mut_cx.world.get_entity(*cx).is_err() {
            return Err(AccessError::EntityNotFound(*cx));
        }
        Ok(RelatedQueryState {
            world: mut_cx.world.as_unsafe_world_cell(),
            query: mut_cx.state.as_mut().unwrap(),
            parent: *cx,
            p: PhantomData,
        })
    }
}

impl<R: RelationshipTarget, D: QueryData + 'static, F: QueryFilter + 'static> AsyncMutAccess
    for AsyncRelatedQuery<R, D, F>
{
    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Cx) -> AccessResult<Self::RefMutCx<'t>> {
        Ok(OwnedQueryState::new(world))
    }
}

pub struct FilterEntity<I: Iterator<Item = Entity>> {
    iter: I,
    not: Entity,
}

impl<I: Iterator<Item = Entity>> Iterator for FilterEntity<I> {
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iter.next() {
                Some(entity) if entity == self.not => (),
                Some(entity) => return Some(entity),
                None => return None,
            }
        }
    }
}

type Iter<'t, T> =
    <<T as RelationshipTarget>::Collection as RelationshipSourceCollection>::SourceIter<'t>;

impl<R: RelationshipTarget, D: QueryData + 'static, F: QueryFilter + 'static>
    RelatedQueryState<'_, R, D, F>
where
    for<'t> Iter<'t, R>: Default,
{
    /// Iterate through children.
    pub fn iter(&mut self) -> QueryManyIter<'_, '_, D::ReadOnly, F, FilterEntity<Iter<R>>> {
        let parent = self.parent;
        // Safety: Safe since nothing has been borrowed yet.
        let world = unsafe { self.world.world() };
        let children = match world.entity(self.parent).get::<R>() {
            Some(children) => children.iter(),
            None => Default::default(),
        };
        self.query.iter_many(
            world,
            FilterEntity {
                iter: children,
                not: parent,
            },
        )
    }

    /// Iterate through children.
    ///
    /// Equivalent to `iter_many_mut`, result is not an iterator and must call `fetch_next` instead.
    pub fn iter_mut(&mut self) -> QueryManyIter<'_, '_, D, F, FilterEntity<Iter<R>>> {
        let parent = self.parent;
        // Safety: Safe since nothing has been borrowed yet.
        let children = match self
            .world
            .get_entity(self.parent)
            .map(|x| unsafe { x.get::<R>() })
            .unwrap()
        {
            Some(children) => children.iter(),
            None => Default::default(),
        };
        // Safety: safe as long as parent is not queried
        let world = unsafe { self.world.world_mut() };
        self.query.iter_many_mut(
            world,
            FilterEntity {
                iter: children,
                not: parent,
            },
        )
    }

    pub fn for_each(&mut self, mut f: impl FnMut(D::Item<'_>)) {
        let mut iter = self.iter_mut();
        while let Some(item) = iter.fetch_next() {
            f(item)
        }
    }
}
