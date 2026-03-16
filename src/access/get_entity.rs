use crate::{AccessError, AccessResult, OwnedReadonlyQueryState};
use bevy::ecs::{
    entity::Entity, hierarchy::Children, name::Name, query::QueryFilter,
    relationship::RelationshipTarget, world::World,
};
use std::{any::type_name, marker::PhantomData};

/// An [`Entity`] or a descriptor of an `Entity` that may or may not exist in the `World`.
pub trait VirtualEntity {
    fn try_get_entity(&self, world: &World) -> AccessResult<Entity>;
}

impl VirtualEntity for Entity {
    fn try_get_entity(&self, _: &World) -> AccessResult<Entity> {
        Ok(*self)
    }
}

#[derive(Debug)]
pub struct FilterChild<E: VirtualEntity, F: QueryFilter + 'static, R: RelationshipTarget = Children>
{
    inner: E,
    p: PhantomData<(F, R)>,
}

impl<E: VirtualEntity + Clone, F: QueryFilter + 'static, R: RelationshipTarget> Clone
    for FilterChild<E, F, R>
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            p: PhantomData,
        }
    }
}

impl<E: VirtualEntity + Copy, F: QueryFilter + 'static, R: RelationshipTarget> Copy
    for FilterChild<E, F, R>
{
}

impl<E: VirtualEntity, F: QueryFilter + 'static, R: RelationshipTarget> FilterChild<E, F, R> {
    pub fn new(entity: E) -> Self {
        Self { inner: entity, p: PhantomData }
    }
}

impl<E: VirtualEntity, F: QueryFilter + 'static, R: RelationshipTarget> VirtualEntity
    for FilterChild<E, F, R>
{
    fn try_get_entity(&self, world: &World) -> AccessResult<Entity> {
        let parent = self.inner.try_get_entity(world)?;
        let Some(children) = world.get::<R>(parent) else {
            return Err(AccessError::TypedChildNotFound {
                query: type_name::<F>(),
            });
        };
        let mut query = OwnedReadonlyQueryState::<Entity, F>::new(world);
        let mut q = query.iter_many(children.iter());
        match q.next() {
            Some(entity) => Ok(entity),
            None => Err(AccessError::TypedChildNotFound {
                query: type_name::<F>(),
            }),
        }
    }
}

#[derive(Debug)]
pub struct IndexedChild<E: VirtualEntity, R: RelationshipTarget = Children> {
    inner: E,
    index: usize,
    p: PhantomData<R>,
}

impl<E: VirtualEntity, R: RelationshipTarget> IndexedChild<E, R> {
    pub fn new(entity: E, index: usize) -> Self {
        Self {
            inner: entity,
            index,
            p: PhantomData,
        }
    }
}

impl<E: VirtualEntity + Clone, R: RelationshipTarget> Clone for IndexedChild<E, R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            index: self.index,
            p: PhantomData,
        }
    }
}

impl<E: VirtualEntity + Copy, R: RelationshipTarget> Copy for IndexedChild<E, R> {}

impl<E: VirtualEntity, R: RelationshipTarget> VirtualEntity for IndexedChild<E, R> {
    fn try_get_entity(&self, world: &World) -> AccessResult<Entity> {
        let parent = self.inner.try_get_entity(world)?;
        if let Some(children) = world.get::<R>(parent) {
            children
                .iter()
                .nth(self.index)
                .ok_or(AccessError::ChildNotFound { index: self.index })
        } else {
            Err(AccessError::ChildNotFound { index: self.index })
        }
    }
}

#[derive(Debug)]
pub struct NamedChild<'t, E: VirtualEntity, R: RelationshipTarget = Children> {
    inner: E,
    name: &'t str,
    p: PhantomData<R>,
}

impl<'t, E: VirtualEntity, R: RelationshipTarget> NamedChild<'t, E, R> {
    pub fn new(entity: E, name: &'t str) -> Self {
        Self {
            inner: entity,
            name,
            p: PhantomData,
        }
    }
}

impl<E: VirtualEntity + Clone, R: RelationshipTarget> Clone for NamedChild<'_, E, R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            name: self.name,
            p: PhantomData,
        }
    }
}

impl<E: VirtualEntity + Copy, R: RelationshipTarget> Copy for NamedChild<'_, E, R> {}

impl<E: VirtualEntity, R: RelationshipTarget> VirtualEntity for NamedChild<'_, E, R> {
    fn try_get_entity(&self, world: &World) -> AccessResult<Entity> {
        let parent = self.inner.try_get_entity(world)?;
        if let Some(children) = world.get::<R>(parent) {
            for child in children.iter() {
                if world
                    .get::<Name>(child)
                    .is_some_and(|x| x.as_str() == self.name)
                {
                    return Ok(child);
                }
            }
        }
        Err(AccessError::NamedChildNotFound)
    }
}
