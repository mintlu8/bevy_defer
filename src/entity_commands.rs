use crate::access::AsyncWorld;
use crate::executor::{with_world_mut, with_world_ref};
use crate::OwnedQueryState;
use crate::{
    access::AsyncEntityMut,
    signals::{SignalId, Signals},
    AccessError, AccessResult,
};
use async_shared::Value;
use bevy_core::Name;
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::world::Command;
use bevy_ecs::{bundle::Bundle, entity::Entity, world::World};
use bevy_hierarchy::{
    BuildWorldChildren, Children, DespawnChildrenRecursive, DespawnRecursive, Parent,
};
use bevy_transform::components::{GlobalTransform, Transform};
use rustc_hash::FxHashMap;
use std::borrow::Borrow;
use std::sync::Arc;

impl AsyncEntityMut {
    /// Adds a [`Bundle`] of components to the entity.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// entity.insert(Str("bevy"));
    /// # });
    /// ```
    pub fn insert(&self, bundle: impl Bundle) -> Result<(), AccessError> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.insert(bundle);
                })
                .ok_or(AccessError::EntityNotFound)
        })
    }

    /// Removes any components in the [`Bundle`] from the entity.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// entity.remove::<Int>();
    /// # });
    /// ```
    pub fn remove<T: Bundle>(&self) -> Result<(), AccessError> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.remove::<T>();
                })
                .ok_or(AccessError::EntityNotFound)
        })
    }

    /// Removes any components except those in the [`Bundle`] from the entity.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// entity.retain::<Int>();
    /// # });
    /// ```
    pub fn retain<T: Bundle>(&self) -> Result<(), AccessError> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.retain::<T>();
                })
                .ok_or(AccessError::EntityNotFound)
        })
    }

    /// Removes all components in the [`Bundle`] from the entity and returns their previous values.
    ///
    /// Note: If the entity does not have every component in the bundle, this method will not remove any of them.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// entity.take::<Int>();
    /// # });
    /// ```
    pub fn take<T: Bundle>(&self) -> Result<T, AccessError> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .ok_or(AccessError::EntityNotFound)
                .and_then(|mut e| e.take::<T>().ok_or(AccessError::ComponentNotFound))
        })
    }

    /// Spawns an entity with the given bundle and inserts it into the parent entity's Children.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// let child = entity.spawn_child(Str("bevy"));
    /// # });
    /// ```
    pub fn spawn_child(&self, bundle: impl Bundle) -> AccessResult<AsyncEntityMut> {
        let entity = self.0;
        let entity = with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    let mut id = Entity::PLACEHOLDER;
                    entity.with_children(|spawn| id = spawn.spawn(bundle).id());
                    id
                })
                .ok_or(AccessError::EntityNotFound)
        })?;
        Ok(AsyncWorld.entity(entity))
    }

    /// Adds a single child.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = AsyncWorld.spawn_bundle(Int(1)).id();
    /// entity.add_child(child);
    /// # });
    /// ```
    pub fn add_child(&self, child: Entity) -> AccessResult<()> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    entity.add_child(child);
                })
                .ok_or(AccessError::EntityNotFound)
        })
    }

    /// Despawns the given entity and all its children recursively.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// entity.despawn();
    /// # });
    /// ```
    pub fn despawn(&self) {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            DespawnRecursive { entity }.apply(world);
        })
    }

    /// Despawns the given entity's children recursively.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// entity.despawn_descendants();
    /// # });
    /// ```
    pub fn despawn_descendants(&self) {
        let entity = self.0;
        with_world_mut(move |world: &mut World| DespawnChildrenRecursive { entity }.apply(world))
    }

    /// Send data through a signal on this entity.
    ///
    /// Returns `true` if the signal exists.
    pub fn send<S: SignalId>(&self, data: S::Data) -> AccessResult<bool> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Some(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound);
            };
            let Some(signals) = entity.get_mut::<Signals>() else {
                return Err(AccessError::ComponentNotFound);
            };
            Ok(signals.send::<S>(data))
        })
    }

    /// Borrow a sender from an entity with shared read tick.
    pub fn sender<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Some(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound);
            };
            let Some(signals) = entity.get_mut::<Signals>() else {
                return Err(AccessError::ComponentNotFound);
            };
            signals
                .borrow_sender::<S>()
                .ok_or(AccessError::SignalNotFound)
        })
    }

    /// Borrow a receiver from an entity with shared read tick.
    pub fn receiver<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Some(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound);
            };
            let Some(signals) = entity.get_mut::<Signals>() else {
                return Err(AccessError::ComponentNotFound);
            };
            signals
                .borrow_receiver::<S>()
                .ok_or(AccessError::SignalNotFound)
        })
    }

    /// Init or borrow a sender from an entity with shared read tick.
    pub fn init_sender<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Some(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound);
            };
            let mut signals = match entity.get_mut::<Signals>() {
                Some(sender) => sender,
                None => entity.insert(Signals::new()).get_mut::<Signals>().unwrap(),
            };
            Ok(signals.init_sender::<S>())
        })
    }

    /// Init or borrow a receiver from an entity with shared read tick.
    pub fn init_receiver<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Some(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound);
            };
            let mut signals = match entity.get_mut::<Signals>() {
                Some(sender) => sender,
                None => entity.insert(Signals::new()).get_mut::<Signals>().unwrap(),
            };
            Ok(signals.init_receiver::<S>())
        })
    }

    /// Obtain all descendent entities in the hierarchy.
    ///
    /// # Guarantee
    ///
    /// The first item is always this entity,
    /// use `[1..]` to exclude it.
    pub fn descendants(&self) -> Vec<Entity> {
        fn get_children(world: &World, parent: Entity, result: &mut Vec<Entity>) {
            let Some(entity) = world.get_entity(parent) else {
                return;
            };
            if let Some(children) = entity.get::<Children>() {
                result.extend(children.iter().cloned());
                for child in children {
                    get_children(world, *child, result);
                }
            }
        }
        let entity = self.0;

        let mut result = vec![entity];

        with_world_ref(|world| get_children(world, entity, &mut result));
        result
    }

    /// Obtain a child entity by [`Name`].
    pub fn child_by_name(
        &self,
        name: impl Into<String> + Borrow<str>,
    ) -> AccessResult<AsyncEntityMut> {
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
        let entity = self.0;

        match with_world_ref(|world| find_name(world, entity, name.borrow())) {
            Some(entity) => Ok(AsyncEntityMut(entity)),
            None => Err(AccessError::EntityNotFound),
        }
    }

    /// Obtain a child entity by [`Name`].
    pub fn children_by_names<I: IntoIterator>(&self, names: I) -> NameEntityMap
    where
        I::Item: Into<String>,
    {
        let descendants = self.descendants();
        let mut result = NameEntityMap(names.into_iter().map(|n| (n.into(), None)).collect());
        AsyncWorld.run(|world| {
            let mut query_state = OwnedQueryState::<(Entity, &Name), ()>::new(world);
            for (entity, name) in query_state.iter_many(descendants) {
                if let Some(item) = result.0.get_mut(name.as_str()) {
                    *item = Some(entity);
                }
            }
        });
        result
    }

    /// Obtain an entity's [`GlobalTransform`] while respecting change detection.
    ///
    /// If `is_added` is true, calculate it from ancestors.
    /// Otherwise returns the entity's [`GlobalTransform`] directly.
    ///
    /// This function is greedy with change detection and does not
    /// promise a perfectly accurate global transform.
    ///
    /// # Errors
    ///
    /// If [`Entity`], [`Transform`] or [`GlobalTransform`] is missing in one of the
    /// target's ancestors.
    pub fn global_transform(&self) -> AccessResult<GlobalTransform> {
        AsyncWorld
            .run(|world| {
                let mut entity = world.get_entity(self.0)?;
                let t = entity.get_ref::<GlobalTransform>()?;
                if !t.is_added() {
                    return Some(*t);
                }
                let mut transform = *entity.get::<Transform>()?;
                while let Some(parent) = entity.get::<Parent>().map(|x| x.get()) {
                    entity = world.get_entity(parent)?;
                    let t = entity.get_ref::<GlobalTransform>()?;
                    if !t.is_added() {
                        return Some(t.mul_transform(transform));
                    }
                    transform = entity.get::<Transform>()?.mul_transform(transform)
                }
                Some(transform.into())
            })
            .ok_or(AccessError::ComponentNotFound)
    }
}

/// A map of names to entities.
#[derive(Debug, Default, Clone)]
pub struct NameEntityMap(FxHashMap<String, Option<Entity>>);

impl NameEntityMap {
    pub fn get(&self, name: impl Borrow<str>) -> AccessResult<Entity> {
        self.0
            .get(name.borrow())
            .copied()
            .flatten()
            .ok_or(AccessError::EntityNotFound)
    }

    pub fn into_map(self) -> impl IntoIterator<Item = (String, Entity)> {
        self.0.into_iter().filter_map(|(s, e)| Some((s, e?)))
    }
}
