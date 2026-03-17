use crate::{access::AsyncEntity, AccessError, AccessResult, OwnedReadonlyQueryState};
use bevy::ecs::{
    entity::Entity,
    hierarchy::{ChildOf, Children},
    name::Name,
    query::QueryFilter,
    relationship::{Relationship, RelationshipTarget},
    world::World,
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
        Self {
            inner: entity,
            p: PhantomData,
        }
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

#[derive(Debug)]
pub struct GetParent<E: VirtualEntity, R: Relationship = ChildOf> {
    inner: E,
    p: PhantomData<R>,
}

impl<E: VirtualEntity + Clone, R: Relationship> Clone for GetParent<E, R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            p: PhantomData,
        }
    }
}

impl<E: VirtualEntity + Copy, R: Relationship> Copy for GetParent<E, R> {}

impl<E: VirtualEntity, R: Relationship> GetParent<E, R> {
    pub fn new(entity: E) -> Self {
        Self {
            inner: entity,
            p: PhantomData,
        }
    }
}

impl<E: VirtualEntity, R: Relationship> VirtualEntity for GetParent<E, R> {
    fn try_get_entity(&self, world: &World) -> AccessResult<Entity> {
        let parent = self.inner.try_get_entity(world)?;
        let Some(parent) = world.get::<R>(parent) else {
            return Err(AccessError::TypedParentNotFound {
                query: type_name::<R>(),
            });
        };
        Ok(parent.get())
    }
}

impl<E: VirtualEntity> AsyncEntity<E> {
    /// Obtain a child entity by index.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Int(1)).unwrap();
    /// # assert_eq!(
    /// entity.child(0)
    /// # .realize_entity().unwrap().id(), child.id());
    /// # });
    /// ```
    pub fn child(self, index: usize) -> AsyncEntity<IndexedChild<E>> {
        AsyncEntity(IndexedChild::new(self.0, index))
    }

    /// Obtain a child entity by [`Name`].
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Name::new("bevy")).unwrap();
    /// # assert_eq!(
    /// entity.child_by_name("bevy")
    /// # .realize_entity().unwrap().id(), child.id());
    /// # });
    /// ```
    pub fn child_by_name<'t>(self, name: &'t str) -> AsyncEntity<NamedChild<'t, E>> {
        AsyncEntity(NamedChild::new(self.0, name))
    }

    /// Obtain the first child that satisfies a filter.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Int(1)).unwrap();
    /// # assert_eq!(
    /// entity.child_by_filter::<With<Int>>()
    /// # .realize_entity().unwrap().id(), child.id());
    /// # });
    /// ```
    pub fn child_by_filter<F: QueryFilter + 'static>(self) -> AsyncEntity<FilterChild<E, F>> {
        AsyncEntity(FilterChild::new(self.0))
    }

    /// Obtain a related entity by index.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Int(1)).unwrap();
    /// # assert_eq!(
    /// entity.related::<Children>(0)
    /// # .realize_entity().unwrap().id(), child.id());
    /// # });
    /// ```
    pub fn related<R: RelationshipTarget>(self, index: usize) -> AsyncEntity<IndexedChild<E, R>> {
        AsyncEntity(IndexedChild::new(self.0, index))
    }

    /// Obtain a related entity by [`Name`].
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Name::new("bevy")).unwrap();
    /// # assert_eq!(
    /// entity.related_by_name::<Children>("bevy")
    /// # .realize_entity().unwrap().id(), child.id());
    /// # });
    /// ```
    pub fn related_by_name<'t, R: RelationshipTarget>(
        self,
        name: &'t str,
    ) -> AsyncEntity<NamedChild<'t, E, R>> {
        AsyncEntity(NamedChild::new(self.0, name))
    }

    /// Obtain the first related entity that satisfies a filter.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Int(1)).unwrap();
    /// # assert_eq!(
    /// entity.related_by_filter::<With<Int>, Children>()
    /// # .realize_entity().unwrap().id(), child.id());
    /// # });
    /// ```
    pub fn related_by_filter<F: QueryFilter + 'static, R: RelationshipTarget>(
        self,
    ) -> AsyncEntity<FilterChild<E, F, R>> {
        AsyncEntity(FilterChild::new(self.0))
    }

    /// Obtain parent of an entity.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Int(1)).unwrap();
    /// #  assert_eq!(
    /// child.parent()
    /// # .realize_entity().unwrap().id(), entity.id());
    /// # });
    /// ```
    pub fn parent(self) -> AsyncEntity<GetParent<E>> {
        AsyncEntity(GetParent::new(self.0))
    }

    /// Obtain a related parent of an entity.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Int(1)).unwrap();
    /// # assert_eq!(
    /// child.related_parent::<ChildOf>()
    /// # .realize_entity().unwrap().id(), entity.id());
    /// # });
    /// ```
    pub fn related_parent<R: Relationship>(self) -> AsyncEntity<GetParent<E, R>> {
        AsyncEntity(GetParent::new(self.0))
    }
}
