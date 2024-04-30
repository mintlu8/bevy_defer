use crate::{executor::{with_world_mut, with_world_ref}, signals::SignalInner};
use bevy_core::Name;
use bevy_ecs::{bundle::Bundle, entity::Entity, system::Command, world::World};
use bevy_hierarchy::{BuildWorldChildren, Children, DespawnChildrenRecursive, DespawnRecursive};
use std::{borrow::Borrow, sync::Arc};
use crate::{access::AsyncEntityMut, signals::{SignalId, Signals}, AsyncFailure, AsyncResult};

impl AsyncEntityMut {

    /// Adds a [`Bundle`] of components to the entity.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1));
    /// entity.insert(Str("bevy"));
    /// # });
    /// ```
    pub fn insert(&self, bundle: impl Bundle) -> Result<(), AsyncFailure> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| {e.insert(bundle);})
                    .ok_or(AsyncFailure::EntityNotFound)
            }        
        )
    }

    /// Removes any components in the [`Bundle`] from the entity.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1));
    /// entity.remove::<Int>();
    /// # });
    /// ```
    pub fn remove<T: Bundle>(&self) -> Result<(), AsyncFailure> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| {e.remove::<T>();})
                    .ok_or(AsyncFailure::EntityNotFound)
            }
        )
    }

    /// Removes any components except those in the [`Bundle`] from the entity.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1));
    /// entity.retain::<Int>();
    /// # });
    /// ```
    pub fn retain<T: Bundle>(&self) -> Result<(), AsyncFailure> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| {e.retain::<T>();})
                    .ok_or(AsyncFailure::EntityNotFound)
            }
        )
    }

    /// Removes all components in the [`Bundle`] from the entity and returns their previous values.
    /// 
    /// Note: If the entity does not have every component in the bundle, this method will not remove any of them.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1));
    /// entity.take::<Int>();
    /// # });
    /// ```
    pub fn take<T: Bundle>(&self) -> Result<Option<T>, AsyncFailure> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| e.take::<T>())
                    .ok_or(AsyncFailure::EntityNotFound)
            }
        )
    }

    /// Spawns an entity with the given bundle and inserts it into the parent entity's Children.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1));
    /// let child = entity.spawn_child(Str("bevy"));
    /// # });
    /// ```
    pub fn spawn_child(&self, bundle: impl Bundle) -> AsyncResult<Entity> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                world.get_entity_mut(entity).map(|mut entity| {
                    let mut id = Entity::PLACEHOLDER;
                    entity.with_children(|spawn| {id = spawn.spawn(bundle).id()});
                    id
                }).ok_or(AsyncFailure::EntityNotFound)
            }
        )
    }

    /// Adds a single child.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1));
    /// # let child = world().spawn_bundle(Int(1)).id();
    /// entity.add_child(child);
    /// # });
    /// ```
    pub fn add_child(&self, child: Entity) -> AsyncResult<()> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut entity| {entity.add_child(child);})
                    .ok_or(AsyncFailure::EntityNotFound)
            }
        )
    }

    /// Despawns the given entity and all its children recursively.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1));
    /// entity.despawn();
    /// # });
    /// ```
    pub fn despawn(&self) {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                DespawnRecursive {
                    entity
                }.apply(world);
            }
        )
    }

    /// Despawns the given entity's children recursively.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1));
    /// entity.despawn_descendants();
    /// # });
    /// ```
    pub fn despawn_descendants(&self) {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                DespawnChildrenRecursive {
                    entity
                }.apply(world)
            }
        )
    }

    /// Send data through a signal on this entity.
    /// 
    /// Returns `true` if the signal exists.
    pub fn send<S: SignalId>(&self, data: S::Data) -> AsyncResult<bool> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Err(AsyncFailure::EntityNotFound)
                };
                let Some(signals) = entity.get_mut::<Signals>() else {
                    return Err(AsyncFailure::ComponentNotFound)
                };
                Ok(signals.send::<S>(data))
            }
        )
    }
    
    /// Borrow a sender from an entity with shared read tick.
    pub fn sender<S: SignalId>(&self) -> AsyncResult<Arc<SignalInner<S::Data>>> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Err(AsyncFailure::EntityNotFound)
                };
                let Some(signals) = entity.get_mut::<Signals>() else {
                    return Err(AsyncFailure::ComponentNotFound)
                };
                signals.borrow_sender::<S>().ok_or(AsyncFailure::SignalNotFound)
            }
        )
    }
    
    /// Borrow a receiver from an entity with shared read tick.
    pub fn receiver<S: SignalId>(&self) -> AsyncResult<Arc<SignalInner<S::Data>>> {
        let entity = self.entity;
        with_world_mut(
            move |world: &mut World| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Err(AsyncFailure::EntityNotFound)
                };
                let Some(signals) = entity.get_mut::<Signals>() else {
                    return Err(AsyncFailure::ComponentNotFound)
                };
                signals.borrow_receiver::<S>().ok_or(AsyncFailure::SignalNotFound)
            }
        )
    } 

    /// Obtain a child entity by [`Name`].
    pub fn child_by_name(&self, name: impl Into<String> + Borrow<str>) -> AsyncResult<AsyncEntityMut> {
        fn find_name(world: &World, parent: Entity, name: &str) -> Option<Entity> {
            let entity = world.get_entity(parent)?;
            if entity.get::<Name>().map(|x| x.as_str() == name) == Some(true) {
                return Some(parent);
            }
            if let Some(children) = entity.get::<Children>() {
                let children: Vec<_> = children.iter().cloned().collect();
                children.into_iter().find_map(|e| find_name(world, e, name))
            } else {
                None
            }
        }
        let entity = self.entity;

        match with_world_ref(|world|find_name(world, entity, name.borrow())) {
            Some(entity) => Ok(AsyncEntityMut {
                entity,
                queue: self.queue.clone(),
            }),
            None => Err(AsyncFailure::EntityNotFound),
        }
    }

    /// Obtain all descendent entities in the hierarchy.
    /// 
    /// # Guarantee
    /// 
    /// The first item is always this entity, 
    /// use `[1..]` to exclude it.
    pub fn descendants(&self) -> Vec<Entity> {
        fn get_children(world: &World, parent: Entity, result: &mut Vec<Entity>) {
            let Some(entity) = world.get_entity(parent) else {return};
            if let Some(children) = entity.get::<Children>() {
                result.extend(children.iter().cloned());
                for child in children {
                    get_children(world, *child, result);
                }
            }
        }
        let entity = self.entity;

        let mut result = vec![entity];

        with_world_ref(|world| get_children(world, entity, &mut result));
        result
    }
}


