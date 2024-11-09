use crate::access::AsyncWorld;
use crate::executor::{with_world_mut, with_world_ref};
use crate::{
    access::AsyncEntityMut,
    signals::{SignalId, Signals},
    AccessError, AccessResult,
};
use crate::{AsyncAccess, OwnedQueryState};
use async_shared::Value;
use bevy::core::Name;
use bevy::ecs::world::Command;
use bevy::ecs::{bundle::Bundle, entity::Entity, world::World};
use bevy::hierarchy::{Children, DespawnChildrenRecursive, DespawnRecursive, Parent};
use bevy::prelude::{BuildChildren, ChildBuild};
use bevy::transform::components::{GlobalTransform, Transform};
use rustc_hash::FxHashMap;
use std::any::type_name;
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
    pub fn insert(&self, bundle: impl Bundle) -> Result<Self, AccessError> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.insert(bundle);
                })
                .map_err(|_| AccessError::EntityNotFound(entity))
        })?;
        Ok(AsyncEntityMut(entity))
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
    pub fn remove<T: Bundle>(&self) -> Result<Self, AccessError> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.remove::<T>();
                })
                .map_err(|_| AccessError::EntityNotFound(entity))
        })?;
        Ok(AsyncEntityMut(entity))
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
    pub fn retain<T: Bundle>(&self) -> Result<Self, AccessError> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.retain::<T>();
                })
                .map_err(|_| AccessError::EntityNotFound(entity))
        })?;
        Ok(AsyncEntityMut(entity))
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
                .map_err(|_| AccessError::EntityNotFound(entity))
                .and_then(|mut e| {
                    e.take::<T>().ok_or(AccessError::ComponentNotFound {
                        name: type_name::<T>(),
                    })
                })
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
                .map_err(|_| AccessError::EntityNotFound(entity))
        })?;
        Ok(AsyncWorld.entity(entity))
    }

    /// Adds a single child, returns the parent [`AsyncEntityMut`].
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
    pub fn add_child(&self, child: Entity) -> AccessResult<AsyncEntityMut> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    entity.add_child(child);
                })
                .map_err(|_| AccessError::EntityNotFound(entity))
        })?;
        Ok(AsyncEntityMut(entity))
    }

    /// Obtain parent of an entity.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = entity.spawn_child(Int(1)).unwrap();
    /// child.parent()
    /// # ;
    /// # assert_eq!(child.parent().unwrap().id(), entity.id());
    /// # });
    /// ```
    pub fn parent(&self) -> AccessResult<AsyncEntityMut> {
        let entity = self.0;
        let child = with_world_mut(move |world: &mut World| {
            world
                .get_entity(entity)
                .ok()
                .and_then(|entity| entity.get::<Parent>().map(|x| x.get()))
                .ok_or(AccessError::EntityNotFound(entity))
        })?;
        Ok(AsyncEntityMut(child))
    }

    /// Set parent to an entity.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// # let child = AsyncWorld.spawn_bundle(Int(1)).id();
    /// entity.set_parent(child);
    /// # });
    /// ```
    pub fn set_parent(&self, parent: Entity) -> AccessResult<AsyncEntityMut> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    entity.set_parent(parent);
                })
                .map_err(|_| AccessError::EntityNotFound(entity))
        })?;
        Ok(AsyncEntityMut(entity))
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
            DespawnRecursive {
                entity,
                warn: false,
            }
            .apply(world);
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
        with_world_mut(move |world: &mut World| {
            DespawnChildrenRecursive {
                entity,
                warn: false,
            }
            .apply(world)
        })
    }

    /// Send data through a signal on this entity.
    ///
    /// Returns `true` if the signal exists.
    pub fn send<S: SignalId>(&self, data: S::Data) -> AccessResult<bool> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound(entity));
            };
            let Some(signals) = entity.get_mut::<Signals>() else {
                return Err(AccessError::ComponentNotFound {
                    name: type_name::<S>(),
                });
            };
            Ok(signals.send::<S>(data))
        })
    }

    /// Get [`Name`] of the entity.
    pub fn name(&self) -> AccessResult<String> {
        self.component::<Name>().get(|x| x.to_string())
    }

    /// Get [`Name`] and index of the entity.
    pub fn debug_string(&self) -> String {
        self.to_string()
    }

    /// Borrow a sender from an entity with shared read tick.
    pub fn sender<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound(entity));
            };
            let Some(signals) = entity.get_mut::<Signals>() else {
                return Err(AccessError::ComponentNotFound {
                    name: type_name::<Signals>(),
                });
            };
            signals
                .borrow_sender::<S>()
                .ok_or(AccessError::SignalNotFound {
                    name: type_name::<S>(),
                })
        })
    }

    /// Borrow a receiver from an entity with shared read tick.
    pub fn receiver<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound(entity));
            };
            let Some(signals) = entity.get_mut::<Signals>() else {
                return Err(AccessError::ComponentNotFound {
                    name: type_name::<Signals>(),
                });
            };
            signals
                .borrow_receiver::<S>()
                .ok_or(AccessError::SignalNotFound {
                    name: type_name::<S>(),
                })
        })
    }

    /// Init or borrow a sender from an entity with shared read tick.
    pub fn init_sender<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound(entity));
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
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound(entity));
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
            let Ok(entity) = world.get_entity(parent) else {
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

    /// Returns a string containing the names of component types on this entity.
    ///
    /// If the entity is missing, returns an error message.
    pub fn debug_print(&self) -> String {
        let e = self.id();
        AsyncWorld.run(|w| {
            if w.get_entity(e).is_err() {
                return format!("Entity {e} missing!");
            }
            let v: Vec<_> = w.inspect_entity(e).map(|x| x.name()).collect();
            v.join(", ")
        })
    }

    /// Obtain a child entity by index.
    pub fn child(&self, index: usize) -> AccessResult<AsyncEntityMut> {
        match self.component::<Children>().get(|x| x.get(index).copied()) {
            Ok(Some(entity)) => Ok(self.world().entity(entity)),
            _ => Err(AccessError::ChildNotFound { index }),
        }
    }

    /// Obtain a child entity by [`Name`].
    pub fn child_by_name(
        &self,
        name: impl Into<String> + Borrow<str>,
    ) -> AccessResult<AsyncEntityMut> {
        fn find_name(world: &World, parent: Entity, name: &str) -> Option<Entity> {
            let entity = world.get_entity(parent).ok()?;
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
            None => Err(AccessError::EntityNotFound(entity)),
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

    /// Obtain an entity's real [`GlobalTransform`] by traversing its ancestors,
    /// this is a relatively slow operation compared to reading [`GlobalTransform`] directly.
    ///
    /// # Errors
    ///
    /// If [`Entity`] or [`Transform`] is missing in one of the target's ancestors.
    pub fn global_transform(&self) -> AccessResult<GlobalTransform> {
        AsyncWorld
            .run(|world| {
                let mut entity = world.get_entity(self.0).ok()?;
                let mut transform = *entity.get::<Transform>()?;
                while let Some(parent) = entity.get::<Parent>().map(|x| x.get()) {
                    entity = world.get_entity(parent).ok()?;
                    transform = entity.get::<Transform>()?.mul_transform(transform)
                }
                Some(transform.into())
            })
            .ok_or(AccessError::ComponentNotFound {
                name: type_name::<GlobalTransform>(),
            })
    }

    /// Obtain an entity's real visibility by traversing its ancestors.
    ///
    /// # Errors
    ///
    /// If `Visibility` is missing in one of the target's ancestors.
    #[cfg(feature = "bevy_render")]
    pub fn visibility(&self) -> AccessResult<bool> {
        use bevy::render::prelude::Visibility;
        AsyncWorld
            .run(|world| {
                let entity = world.get_entity(self.0).ok()?;
                match entity.get::<Visibility>()? {
                    Visibility::Inherited => (),
                    Visibility::Hidden => return Some(false),
                    Visibility::Visible => return Some(true),
                }
                while let Some(parent) = entity.get::<Parent>().map(|x| x.get()) {
                    match world.get_entity(parent).ok()?.get::<Visibility>()? {
                        Visibility::Inherited => (),
                        Visibility::Hidden => return Some(false),
                        Visibility::Visible => return Some(true),
                    }
                }
                Some(true)
            })
            .ok_or(AccessError::ComponentNotFound {
                name: type_name::<Visibility>(),
            })
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
            .ok_or(AccessError::Custom("named entity missing"))
    }

    pub fn into_map(self) -> impl IntoIterator<Item = (String, Entity)> {
        self.0.into_iter().filter_map(|(s, e)| Some((s, e?)))
    }
}
