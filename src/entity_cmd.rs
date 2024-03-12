use std::time::Duration;

use async_oneshot::oneshot;
use bevy_animation::{AnimationClip, AnimationPlayer};
use bevy_ecs::{bundle::Bundle, entity::Entity, system::Command, world::World};
use bevy_hierarchy::{BuildWorldChildren, DespawnChildrenRecursive, DespawnRecursive};
use bevy_asset::Handle;
use futures_lite::Future;
use crate::{async_world::AsyncEntityMut, AsyncFailure, AsyncResult, BoxedQueryCallback, CHANNEL_CLOSED};

impl AsyncEntityMut<'_> {

    pub fn insert(&self, bundle: impl Bundle) -> impl Future<Output = Result<(), AsyncFailure>> {
        let (sender, receiver) = oneshot();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| {e.insert(bundle);})
                    .ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    pub fn remove<T: Bundle>(&self) -> impl Future<Output = Result<(), AsyncFailure>> {
        let (sender, receiver) = oneshot();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut e| {e.remove::<T>();})
                    .ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }


    pub fn spawn_child(&self, bundle: impl Bundle) -> impl Future<Output = AsyncResult<Entity>> {
        let (sender, receiver) = oneshot::<Option<Entity>>();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_entity_mut(entity).and_then(|mut entity| {
                    let mut id = Entity::PLACEHOLDER;
                    entity.with_children(|spawn| {id = spawn.spawn(bundle).id()});
                    Some(id)
                })
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
                .ok_or(AsyncFailure::EntityNotFound)
        }
    }

    pub fn add_child(&self, child: Entity) -> impl Future<Output = AsyncResult<()>> {
        let (sender, receiver) = oneshot();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_entity_mut(entity)
                    .map(|mut entity| {entity.add_child(child);})
                    .ok_or(AsyncFailure::EntityNotFound)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    // Calls despawn_recursive
    pub fn despawn(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = oneshot::<()>();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                DespawnRecursive {
                    entity
                }.apply(world);
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    // Calls despawn_children_recursive
    pub fn despawn_descendants(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = oneshot::<()>();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                DespawnChildrenRecursive {
                    entity
                }.apply(world)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    pub fn animate(&self, clip: Handle<AnimationClip>) -> impl Future<Output = AsyncResult<()>> {
        let (sender, receiver) = oneshot();
        let entity = self.entity;
        let mut once = true;
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Some(Err(AsyncFailure::EntityNotFound))
                };
                let Some(mut player) = entity.get_mut::<AnimationPlayer>() else {
                    return Some(Err(AsyncFailure::ComponentNotFound))
                };
                if once {
                    once = false;
                    player.play(clip.clone());
                }
                (player.animation_clip() != &clip || player.is_finished())
                    .then_some(Ok(()))
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {receiver.await.expect(CHANNEL_CLOSED)}
    }

    pub fn animate_with_transition(&self, clip: Handle<AnimationClip>, time: Duration) -> impl Future<Output = AsyncResult<()>> {
        let (sender, receiver) = oneshot();
        let entity = self.entity;
        let mut once = true;
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Some(Err(AsyncFailure::EntityNotFound))
                };
                let Some(mut player) = entity.get_mut::<AnimationPlayer>() else {
                    return Some(Err(AsyncFailure::ComponentNotFound))
                };
                if once {
                    once = false;
                    player.play_with_transition(clip.clone(), time);
                }
                (player.animation_clip() != &clip || player.is_finished())
                    .then_some(Ok(()))
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }
}
