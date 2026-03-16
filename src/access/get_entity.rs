use crate::{AccessError, AccessResult, OwnedReadonlyQueryState};
use bevy::ecs::{
    entity::Entity, hierarchy::Children, name::Name, query::QueryFilter,
    relationship::RelationshipTarget, world::World,
};
use std::{any::type_name, marker::PhantomData};

pub trait TryGetEntity {
    fn try_get_entity(&self, world: &World) -> AccessResult<Entity>;
}

impl TryGetEntity for Entity {
    fn try_get_entity(&self, _: &World) -> AccessResult<Entity> {
        Ok(*self)
    }
}

#[derive(Debug)]
pub struct FirstChild<E: TryGetEntity, F: QueryFilter + 'static, R: RelationshipTarget = Children> {
    pub(crate) inner: E,
    pub(crate) p: PhantomData<(F, R)>,
}

impl<E: TryGetEntity + Clone, F: QueryFilter + 'static, R: RelationshipTarget> Clone
    for FirstChild<E, F, R>
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            p: PhantomData,
        }
    }
}

impl<E: TryGetEntity + Copy, F: QueryFilter + 'static, R: RelationshipTarget> Copy
    for FirstChild<E, F, R>
{
}

impl<E: TryGetEntity, F: QueryFilter + 'static, R: RelationshipTarget> TryGetEntity
    for FirstChild<E, F, R>
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
pub struct IndexedChild<E: TryGetEntity, R: RelationshipTarget = Children> {
    pub(crate) inner: E,
    pub(crate) index: usize,
    pub(crate) p: PhantomData<R>,
}

impl<E: TryGetEntity + Clone, R: RelationshipTarget> Clone for IndexedChild<E, R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            index: self.index,
            p: PhantomData,
        }
    }
}

impl<E: TryGetEntity + Copy, R: RelationshipTarget> Copy for IndexedChild<E, R> {}

impl<E: TryGetEntity, R: RelationshipTarget> TryGetEntity for IndexedChild<E, R> {
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
pub struct NamedChild<'t, E: TryGetEntity, R: RelationshipTarget = Children> {
    pub(crate) inner: E,
    pub(crate) name: &'t str,
    pub(crate) p: PhantomData<R>,
}

impl<E: TryGetEntity + Clone, R: RelationshipTarget> Clone for NamedChild<'_, E, R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            name: self.name,
            p: PhantomData,
        }
    }
}

impl<E: TryGetEntity + Copy, R: RelationshipTarget> Copy for NamedChild<'_, E, R> {}

impl<E: TryGetEntity, R: RelationshipTarget> TryGetEntity for NamedChild<'_, E, R> {
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
