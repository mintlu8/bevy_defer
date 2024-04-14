use crate::{channels::{channel, ChannelOut, MaybeChannelOut}, signals::SignalInner};
use bevy_core::Name;
use bevy_ecs::{bundle::Bundle, entity::Entity, system::Command, world::World};
use bevy_hierarchy::{BuildWorldChildren, Children, DespawnChildrenRecursive, DespawnRecursive};
use futures::future::{ready, Either};
use std::{borrow::Borrow, sync::Arc};
use crate::{access::AsyncEntityMut, signals::{SignalId, Signals}, AsyncFailure, AsyncResult};

impl AsyncEntityMut {

    /// Adds a [`Bundle`] of components to the entity.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1)).await;
    /// entity.insert(Str("bevy")).await;
    /// # });
    /// ```
    pub fn insert(&self, bundle: impl Bundle) -> ChannelOut<Result<(), AsyncFailure>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| {e.insert(bundle);})
                    .ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        receiver.into_out()
    }

    /// Removes any components in the [`Bundle`] from the entity.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1)).await;
    /// entity.remove::<Int>().await;
    /// # });
    /// ```
    pub fn remove<T: Bundle>(&self) -> ChannelOut<Result<(), AsyncFailure>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| {e.remove::<T>();})
                    .ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        receiver.into_out()
    }

    /// Removes any components except those in the [`Bundle`] from the entity.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1)).await;
    /// entity.retain::<Int>().await;
    /// # });
    /// ```
    pub fn retain<T: Bundle>(&self) -> ChannelOut<Result<(), AsyncFailure>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| {e.retain::<T>();})
                    .ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        receiver.into_out()
    }

    /// Removes all components in the [`Bundle`] from the entity and returns their previous values.
    /// 
    /// Note: If the entity does not have every component in the bundle, this method will not remove any of them.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1)).await;
    /// entity.take::<Int>().await;
    /// # });
    /// ```
    pub fn take<T: Bundle>(&self) -> ChannelOut<Result<Option<T>, AsyncFailure>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| e.take::<T>())
                    .ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        receiver.into_out()
    }

    /// Spawns an entity with the given bundle and inserts it into the parent entity's Children.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1)).await;
    /// let child = entity.spawn_child(Str("bevy")).await;
    /// # });
    /// ```
    pub fn spawn_child(&self, bundle: impl Bundle) -> ChannelOut<AsyncResult<Entity>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                world.get_entity_mut(entity).map(|mut entity| {
                    let mut id = Entity::PLACEHOLDER;
                    entity.with_children(|spawn| {id = spawn.spawn(bundle).id()});
                    id
                }).ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        receiver.into_out()
    }

    /// Adds a single child.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1)).await;
    /// # let child = world().spawn_bundle(Int(1)).await.id();
    /// entity.add_child(child).await;
    /// # });
    /// ```
    pub fn add_child(&self, child: Entity) -> ChannelOut<AsyncResult<()>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut entity| {entity.add_child(child);})
                    .ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        receiver.into_out()
    }

    /// Despawns the given entity and all its children recursively.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1)).await;
    /// entity.despawn().await;
    /// # });
    /// ```
    pub fn despawn(&self) -> ChannelOut<()> {
        let (sender, receiver) = channel::<()>();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                DespawnRecursive {
                    entity
                }.apply(world);
            },
            sender
        );
        receiver.into_out()
    }

    /// Despawns the given entity's children recursively.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(1)).await;
    /// entity.despawn_descendants().await;
    /// # });
    /// ```
    pub fn despawn_descendants(&self) -> ChannelOut<()> {
        let (sender, receiver) = channel::<()>();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                DespawnChildrenRecursive {
                    entity
                }.apply(world)
            },
            sender
        );
        receiver.into_out()
    }

    /// Send data through a signal on this entity.
    /// 
    /// Returns `true` if the signal exists.
    pub fn send<S: SignalId>(&self, data: S::Data) -> ChannelOut<AsyncResult<bool>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Err(AsyncFailure::EntityNotFound)
                };
                let Some(signals) = entity.get_mut::<Signals>() else {
                    return Err(AsyncFailure::ComponentNotFound)
                };
                Ok(signals.send::<S>(data))
            },
            sender
        );
        receiver.into_out()
    }
    
    /// Borrow a sender from an entity with shared read tick.
    pub fn sender<S: SignalId>(&self) -> ChannelOut<AsyncResult<Arc<SignalInner<S::Data>>>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Err(AsyncFailure::EntityNotFound)
                };
                let Some(signals) = entity.get_mut::<Signals>() else {
                    return Err(AsyncFailure::ComponentNotFound)
                };
                signals.borrow_sender::<S>().ok_or(AsyncFailure::SignalNotFound)
            },
            sender
        );
        receiver.into_out()
    }
    
    /// Borrow a receiver from an entity with shared read tick.
    pub fn receiver<S: SignalId>(&self) -> ChannelOut<AsyncResult<Arc<SignalInner<S::Data>>>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Err(AsyncFailure::EntityNotFound)
                };
                let Some(signals) = entity.get_mut::<Signals>() else {
                    return Err(AsyncFailure::ComponentNotFound)
                };
                signals.borrow_receiver::<S>().ok_or(AsyncFailure::SignalNotFound)
            },
            sender
        );
        receiver.into_out()
    } 

    /// Obtain a child entity by [`Name`].
    pub fn child_by_name(&self, name: impl Into<String> + Borrow<str>) -> MaybeChannelOut<AsyncResult<AsyncEntityMut>> {
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

        match self.with_world_ref(|world|find_name(world, entity, name.borrow())) {
            Ok(Some(entity)) => return Either::Right(ready(Ok(AsyncEntityMut {
                entity,
                queue: self.queue.clone(),
            }))),
            Ok(None) => return Either::Right(ready(Err(AsyncFailure::EntityNotFound))),
            Err(_) => (),
        }

        let (sender, receiver) = channel();
        let name = name.into();
        let queue = self.queue.clone();
        self.queue.once(
            move |world: &mut World| {
                find_name(world, entity, &name).map(|entity| AsyncEntityMut {
                    entity, 
                    queue,
                }).ok_or(AsyncFailure::EntityNotFound)
            },
            sender,
        );
        Either::Left(receiver.into_out())
    }

    /// Obtain all descendent entities in the hierarchy.
    /// 
    /// # Guarantee
    /// 
    /// The first item is always this entity, 
    /// use `[1..]` to exclude it.
    pub fn descendants(&self) -> MaybeChannelOut<Vec<Entity>> {
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

        if self.with_world_ref(|world|get_children(world, entity, &mut result)).is_ok() {
            return Either::Right(ready(result))
        }

        let (sender, receiver) = channel();
        self.queue.once(
            move |world: &mut World| {
                get_children(world, entity, &mut result);
                result
            },
            sender,
        );
        Either::Left(receiver.into_out())
    }
}


