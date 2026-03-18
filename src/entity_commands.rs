use crate::access::async_world::AsyncEntity;
use crate::access::get_entity::VirtualEntity;
use crate::access::AsyncWorld;
use crate::executor::{with_world_mut, with_world_ref};
use crate::InspectEntity;
use crate::OwnedReadonlyQueryState;
use crate::{AccessError, AccessResult};
use bevy::ecs::bundle::BundleFromComponents;
use bevy::ecs::component::Component;
use bevy::ecs::event::EntityEvent;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::name::Name;
use bevy::ecs::observer::On;
use bevy::ecs::relationship::{Relationship, RelationshipTarget};
use bevy::ecs::system::{EntityCommand, IntoObserverSystem};
use bevy::ecs::world::{EntityRef, EntityWorldMut};
use bevy::ecs::{bundle::Bundle, entity::Entity, world::World};
use bevy::transform::components::{GlobalTransform, Transform};
use event_listener::Event as AsyncEvent;
use futures::channel::mpsc;
use futures::future::Either;
use futures::Stream;
use rustc_hash::FxHashMap;
use std::any::type_name;
use std::borrow::Borrow;
use std::future::{ready, Future};

impl<E: VirtualEntity> AsyncEntity<E> {
    /// Run a function on the [`EntityRef`].
    ///
    /// Can be used inside a readonly world access scope and
    /// converts the scope into a readonly world access scope.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity1 = AsyncWorld.spawn_bundle(Int(1));
    /// # let entity2 = AsyncWorld.spawn_bundle(Int(2));
    /// // creates a readonly world access scope
    /// entity1.get(|a| {
    ///     dbg!(a.id());
    ///     // can be used in a readonly world access scope
    ///     entity2.get(|b| {
    ///         dbg!(b.id());
    ///     })
    /// })
    /// # });
    /// ```
    pub fn get<T>(&self, f: impl FnOnce(EntityRef) -> T) -> AccessResult<T> {
        with_world_ref(|w| {
            let entity = self.0.try_get_entity(w)?;
            if let Ok(e) = w.get_entity(entity) {
                Ok(f(e))
            } else {
                Err(AccessError::EntityNotFound(entity))
            }
        })
    }

    /// Run a function on the [`EntityWorldMut`].
    pub fn get_mut<T>(&self, f: impl FnOnce(EntityWorldMut) -> T) -> AccessResult<T> {
        with_world_mut(|w| {
            let entity = self.0.try_get_entity(w)?;
            if let Ok(e) = w.get_entity_mut(entity) {
                Ok(f(e))
            } else {
                Err(AccessError::EntityNotFound(entity))
            }
        })
    }

    /// Apply an [`EntityCommand`].
    pub fn apply_command(&self, command: impl EntityCommand) -> AccessResult<AsyncEntity> {
        with_world_mut(|w| {
            let entity = self.0.try_get_entity(w)?;
            if let Ok(e) = w.get_entity_mut(entity) {
                command.apply(e);
                Ok(AsyncEntity(entity))
            } else {
                Err(AccessError::EntityNotFound(entity))
            }
        })
    }

    /// Check if the entity exists.
    pub fn exists(&self) -> bool {
        with_world_mut(
            move |world: &mut World| match self.0.try_get_entity(world) {
                Ok(entity) => world.get_entity(entity).is_ok(),
                Err(_) => false,
            },
        )
    }

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
    pub fn insert(&self, bundle: impl Bundle) -> Result<AsyncEntity, AccessError> {
        let entity = with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.insert(bundle);
                })
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(entity)
        })?;
        Ok(AsyncEntity(entity))
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
    pub fn remove<T: Bundle>(&self) -> Result<AsyncEntity, AccessError> {
        let entity = with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.remove::<T>();
                })
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(entity)
        })?;
        Ok(AsyncEntity(entity))
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
    pub fn retain<T: Bundle>(&self) -> Result<AsyncEntity, AccessError> {
        let entity = with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map(|mut e| {
                    e.retain::<T>();
                })
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(entity)
        })?;
        Ok(AsyncEntity(entity))
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
    pub fn take<T: Bundle + BundleFromComponents>(&self) -> Result<T, AccessError> {
        with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))
                .and_then(|mut e| {
                    e.take::<T>().ok_or(AccessError::ComponentNotFound {
                        entity,
                        name: type_name::<T>(),
                    })
                })
        })
    }

    /// Creates an `Observer` listening for events of type `E` targeting this entity.
    ///
    /// In order to trigger the callback the entity must also match the query when the event is fired.
    pub fn observe<T: EntityEvent, B: Bundle, M>(
        &self,
        observer: impl IntoObserverSystem<T, B, M>,
    ) -> AccessResult {
        with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?
                .observe(observer);
            Ok(())
        })
    }

    /// Triggers the given event for this entity, which will run any observers watching for it.
    pub fn trigger<T: EntityEvent>(&self, event: impl FnOnce(Entity) -> T) -> AccessResult
    where
        for<'t> T::Trigger<'t>: Default,
    {
        with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?
                .trigger(event);
            Ok(())
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
    pub fn spawn_child(&self, bundle: impl Bundle) -> AccessResult<AsyncEntity> {
        let entity = with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
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

    /// Spawns an entity with the given bundle and inserts it into the parent entity's Children.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// let child = entity.spawn_related::<ChildOf>(Str("bevy"));
    /// # });
    /// ```
    pub fn spawn_related<R: Relationship>(&self, bundle: impl Bundle) -> AccessResult<AsyncEntity> {
        let entity = with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    let mut id = Entity::PLACEHOLDER;
                    entity.with_related_entities::<R>(|spawn| id = spawn.spawn(bundle).id());
                    id
                })
                .map_err(|_| AccessError::EntityNotFound(entity))
        })?;
        Ok(AsyncWorld.entity(entity))
    }

    /// Adds a single child, returns the parent [`AsyncEntity`].
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
    pub fn add_child(&self, child: Entity) -> AccessResult<AsyncEntity> {
        let entity = with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    entity.add_child(child);
                })
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(entity)
        })?;
        Ok(AsyncEntity(entity))
    }

    /// Adds a single child, returns the parent [`AsyncEntity`].
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
    pub fn add_related<R: Relationship>(&self, child: Entity) -> AccessResult<AsyncEntity> {
        let entity = with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    entity.add_related::<R>(&[child]);
                })
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(entity)
        })?;
        Ok(AsyncEntity(entity))
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
    pub fn set_parent(&self, parent: impl Borrow<Entity>) -> AccessResult<AsyncEntity> {
        let entity = with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    entity.insert(ChildOf(*parent.borrow()));
                })
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(entity)
        })?;
        Ok(AsyncEntity(entity))
    }

    /// Create a [`Stream`] of a specific triggered event.
    ///
    /// `T` corresponds to `Trigger<T>` in observers.
    ///
    /// # Note
    ///
    /// This function spawns an observer.
    pub fn on<T: EntityEvent + Clone>(&self) -> AccessResult<impl Stream<Item = T> + 'static> {
        let (sender, receiver) = mpsc::unbounded();
        with_world_mut(|world| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get_entity_mut(entity)
                .map(|mut entity| {
                    entity.observe(move |trigger: On<T>| {
                        let _ = sender.unbounded_send(trigger.event().clone());
                    });
                })
                .map_err(|_| AccessError::EntityNotFound(entity))
        })?;
        Ok(receiver)
    }

    /// Returns a future that yields when the entity is despawned.
    pub fn on_despawn(self) -> impl Future + 'static {
        #[derive(Component)]
        struct OnDespawn(AsyncEvent);

        impl Drop for OnDespawn {
            fn drop(&mut self) {
                self.0.notify(usize::MAX);
            }
        }

        with_world_mut(|world| {
            let Ok(entity) = self.0.try_get_entity(world) else {
                return Either::Left(ready(()));
            };
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Either::Left(ready(()));
            };
            if let Some(event) = entity.get::<OnDespawn>() {
                Either::Right(event.0.listen())
            } else {
                let on_despawn = OnDespawn(AsyncEvent::new());
                let event = on_despawn.0.listen();
                entity.insert(on_despawn);
                Either::Right(event)
            }
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
        with_world_mut(move |world: &mut World| {
            if let Ok(entity) = self.0.try_get_entity(world) {
                world.despawn(entity);
            }
        })
    }

    /// Despawns the given entity's children recursively.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = AsyncWorld.spawn_bundle(Int(1));
    /// entity.despawn_related::<Children>();
    /// # });
    /// ```
    pub fn despawn_related<R: RelationshipTarget>(&self) {
        with_world_mut(move |world: &mut World| {
            if let Ok(entity) = self.0.try_get_entity(world) {
                if let Ok(mut entity) = world.get_entity_mut(entity) {
                    entity.despawn_related::<R>();
                }
            }
        })
    }

    /// Get [`Name`] of the entity.
    pub fn name(&self) -> AccessResult<String> {
        with_world_mut(move |world: &mut World| {
            let entity = self.0.try_get_entity(world)?;
            world
                .get::<Name>(entity)
                .map(|x| x.to_string())
                .ok_or(AccessError::ComponentNotFound {
                    entity,
                    name: type_name::<Name>(),
                })
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
                result.extend(children.iter());
                for child in children {
                    get_children(world, *child, result);
                }
            }
        }

        let mut result = vec![];

        with_world_ref(|world| {
            if let Ok(entity) = self.0.try_get_entity(world) {
                result.push(entity);
                get_children(world, entity, &mut result)
            }
        });
        result
    }

    /// Obtain the [`InspectEntity`] string.
    pub fn inspect(&self) -> String {
        if let Ok(entity) = self.try_get_id() {
            InspectEntity(entity).to_string()
        } else {
            "INVALID_ENTITY".to_owned()
        }
    }

    /// Returns a string containing the names of all component types on this entity.
    ///
    /// If the entity is missing, returns an error message.
    pub fn inspect_components(&self) -> String {
        with_world_ref(|world| {
            let Ok(entity) = self.0.try_get_entity(world) else {
                return "Invalid entity!".to_string();
            };
            if let Ok(i) = world.inspect_entity(entity) {
                let v: Vec<_> = i.map(|x| x.name().shortname().to_string()).collect();
                v.join(", ")
            } else {
                format!("Entity {entity} missing!")
            }
        })
    }

    /// Collect [`Children`] into a [`Vec`].
    pub fn children_vec(&self) -> Vec<Entity> {
        with_world_ref(|world| {
            if let Ok(entity) = self.0.try_get_entity(world) {
                world
                    .entity(entity)
                    .get::<Children>()
                    .map(|x| x.iter().collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        })
    }

    /// Collect [`RelationshipTarget`] into a [`Vec`].
    pub fn relation_vec<R: RelationshipTarget>(&self) -> Vec<Entity> {
        with_world_ref(|world| {
            if let Ok(entity) = self.0.try_get_entity(world) {
                world
                    .entity(entity)
                    .get::<R>()
                    .map(|x| x.iter().collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        })
    }

    /// Obtain child entities by [`Name`].
    pub fn descendants_by_names<I: IntoIterator>(&self, names: I) -> NameEntityMap
    where
        I::Item: Into<String>,
    {
        let descendants = self.descendants();
        let mut result = NameEntityMap(names.into_iter().map(|n| (n.into(), None)).collect());
        with_world_ref(|world| {
            let mut query_state = OwnedReadonlyQueryState::<(Entity, &Name), ()>::new(world);
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
        let mut entity_out = Entity::PLACEHOLDER;
        with_world_ref(|world| {
            let entity = self.0.try_get_entity(world).ok()?;
            entity_out = entity;
            let mut entity = world.get_entity(entity).ok()?;
            let mut transform = *entity.get::<Transform>()?;
            while let Some(parent) = entity.get::<ChildOf>().map(|x| x.parent()) {
                entity = world.get_entity(parent).ok()?;
                transform = entity.get::<Transform>()?.mul_transform(transform)
            }
            Some(transform.into())
        })
        .ok_or(AccessError::ComponentNotFound {
            entity: entity_out,
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
        use bevy::prelude::Visibility;
        let mut entity_out = Entity::PLACEHOLDER;
        with_world_ref(|world| {
            let entity = self.0.try_get_entity(world).ok()?;
            entity_out = entity;
            let mut entity = world.get_entity(entity).ok()?;
            match entity.get::<Visibility>()? {
                Visibility::Inherited => (),
                Visibility::Hidden => return Some(false),
                Visibility::Visible => return Some(true),
            }
            while let Some(parent) = entity.get::<ChildOf>().map(|x| x.parent()) {
                entity = world.get_entity(parent).ok()?;
                match entity.get::<Visibility>()? {
                    Visibility::Inherited => (),
                    Visibility::Hidden => return Some(false),
                    Visibility::Visible => return Some(true),
                }
            }
            Some(true)
        })
        .ok_or(AccessError::ComponentNotFound {
            entity: entity_out,
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
