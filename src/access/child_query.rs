use std::marker::PhantomData;

use bevy::ecs::{
    entity::Entity,
    hierarchy::Children,
    query::{QueryData, QueryFilter, QueryManyIter, QueryState},
    relationship::{RelationshipSourceCollection, RelationshipTarget},
    world::unsafe_world_cell::UnsafeWorldCell,
};

use super::AsyncEntityMut;

#[derive(Debug)]
pub struct AsyncRelatedQuery<R: RelationshipTarget, D: QueryData, F: QueryFilter> {
    entity: Entity,
    p: PhantomData<(R, D, F)>,
}

impl<R: RelationshipTarget, D: QueryData, F: QueryFilter> Clone for AsyncRelatedQuery<R, D, F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: RelationshipTarget, D: QueryData, F: QueryFilter> Copy for AsyncRelatedQuery<R, D, F> {}

impl<R: RelationshipTarget, D: QueryData, F: QueryFilter> AsyncRelatedQuery<R, D, F> {
    pub fn id(&self) -> Entity {
        self.entity
    }

    pub fn entity(&self) -> AsyncEntityMut {
        AsyncEntityMut(self.entity)
    }
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
    pub(super) world: UnsafeWorldCell<'t>,
    pub(super) query: &'t mut QueryState<D, F>,
    pub(super) parent: Entity,
    pub(super) p: PhantomData<R>,
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
    pub fn iter(&mut self) -> QueryManyIter<'_, '_, D::ReadOnly, F, FilterEntity<Iter<'_, R>>> {
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
    pub fn iter_mut(&mut self) -> QueryManyIter<'_, '_, D, F, FilterEntity<Iter<'_, R>>> {
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

    pub fn for_each(&mut self, mut f: impl FnMut(D::Item<'_, '_>)) {
        let mut iter = self.iter_mut();
        while let Some(item) = iter.fetch_next() {
            f(item)
        }
    }
}
