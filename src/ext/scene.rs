use std::borrow::Borrow;
use bevy_ecs::component::Component;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query};
use bevy_scene::SceneInstance;
use bevy_ecs::{bundle::Bundle, entity::Entity};
use crate::channels::MaybeChannelOut;
use crate::AsyncResult;
use crate::access::{AsyncEntityMut, AsyncWorldMut};

#[derive(Debug, Component)]
pub struct SceneSignal(async_oneshot::Sender<()>);

/// Send [`SceneSignal`] once scene is loaded.
pub fn react_to_scene_load(
    mut commands: Commands,
    mut query: Query<(Entity, &mut SceneSignal), With<SceneInstance>>
) {
    for (entity, mut signal) in query.iter_mut() {
        let _ = signal.0.send(());
        commands.entity(entity).remove::<SceneSignal>();
    }
}

impl AsyncWorldMut {
    /// Spawn a scene and wait for spawning to complete.
    ///
    /// Requires [`react_to_scene_load`] to function.
    pub async fn spawn_scene(&self, bun: impl Bundle) -> AsyncEntityMut{
        let (send, recv) = async_oneshot::oneshot();
        let entity = self.spawn_bundle((bun, SceneSignal(send))).await.id();
        let _ = recv.await;
        AsyncEntityMut { entity, queue: self.queue.clone() }
    }
}

impl AsyncEntityMut {
    /// Obtain a child by name, alias for `child_by_name`.
    pub fn spawned(&self, name: impl Into<String> + Borrow<str>) -> MaybeChannelOut<AsyncResult<AsyncEntityMut>> {
        self.child_by_name(name)
    }
}