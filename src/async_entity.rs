use crate::{channels::channel, signals::SignalInner};
use bevy_ecs::{bundle::Bundle, entity::Entity, system::Command, world::World};
use bevy_hierarchy::{BuildWorldChildren, DespawnChildrenRecursive, DespawnRecursive};
use futures::FutureExt;
use std::{future::Future, sync::Arc};
use crate::{async_world::AsyncEntityMut, signals::{SignalId, Signals}, AsyncFailure, AsyncResult, CHANNEL_CLOSED};

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
    pub fn insert(&self, bundle: impl Bundle) -> impl Future<Output = Result<(), AsyncFailure>> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
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
    pub fn remove<T: Bundle>(&self) -> impl Future<Output = Result<(), AsyncFailure>> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
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
    pub fn retain<T: Bundle>(&self) -> impl Future<Output = Result<(), AsyncFailure>> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
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
    pub fn take<T: Bundle>(&self) -> impl Future<Output = Result<Option<T>, AsyncFailure>> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
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
    pub fn spawn_child(&self, bundle: impl Bundle) -> impl Future<Output = AsyncResult<Entity>> {
        let (sender, receiver) = channel::<Option<Entity>>();
        let entity = self.entity;
        self.queue.once(
            move |world: &mut World| {
                world.get_entity_mut(entity).map(|mut entity| {
                    let mut id = Entity::PLACEHOLDER;
                    entity.with_children(|spawn| {id = spawn.spawn(bundle).id()});
                    id
                })
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED)
            .ok_or(AsyncFailure::EntityNotFound))
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
    pub fn add_child(&self, child: Entity) -> impl Future<Output = AsyncResult<()>> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
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
    pub fn despawn(&self) -> impl Future<Output = ()> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
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
    pub fn despawn_descendants(&self) -> impl Future<Output = ()> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Send data through a signal on this entity.
    /// 
    /// Returns `true` if the signal exists.
    pub fn send<S: SignalId>(&self, data: S::Data) -> impl Future<Output = AsyncResult<bool>> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Receive data from a signal on this entity.
    pub fn recv<S: SignalId>(&self) -> impl Future<Output = AsyncResult<S::Data>> {
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
                let Some(receiver) = signals.borrow_receiver::<S>() else {
                    return Err(AsyncFailure::SignalNotFound)
                };
                Ok(receiver)
            },
            sender
        );
        async {
            match receiver.await.expect(CHANNEL_CLOSED) {
                Ok(fut) => Ok(fut.poll().await),
                Err(e) => Err(e),
            }
        }
    }
    
    /// Borrow a sender from an entity with shared read tick.
    pub fn sender<S: SignalId>(&self) -> impl Future<Output = AsyncResult<Arc<SignalInner<S::Data>>>> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
    
    /// Borrow a receiver from an entity with shared read tick.
    pub fn receiver<S: SignalId>(&self) -> impl Future<Output = AsyncResult<Arc<SignalInner<S::Data>>>> {
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}
