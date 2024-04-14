use crate::{channels::{channel, ChannelOut}, signals::SignalInner};
use bevy_ecs::{bundle::Bundle, entity::Entity, system::Command, world::World};
use bevy_hierarchy::{BuildWorldChildren, DespawnChildrenRecursive, DespawnRecursive};
use std::sync::Arc;
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
}
