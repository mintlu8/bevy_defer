use crate::access::{AsyncEntity, AsyncWorld};
use bevy::ecs::component::Component;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, Query};
use bevy::ecs::{bundle::Bundle, entity::Entity};
use bevy::world_serialization::WorldInstance;

/// A component that sends a signal and removes itself
/// if a paired `Scene` is loaded.
#[derive(Debug, Component)]
pub struct SceneSignal(async_oneshot::Sender<()>);

/// Send [`SceneSignal`] once scene is loaded.
pub fn react_to_scene_load(
    mut commands: Commands,
    mut query: Query<(Entity, &mut SceneSignal), With<WorldInstance>>,
) {
    for (entity, mut signal) in query.iter_mut() {
        let _ = signal.0.send(());
        commands.entity(entity).remove::<SceneSignal>();
    }
}

impl AsyncWorld {
    /// Spawn a scene and wait for spawning to complete.
    ///
    /// Requires [`react_to_scene_load`] to function.
    pub async fn spawn_scene(&self, bun: impl Bundle) -> AsyncEntity {
        let (send, recv) = async_oneshot::oneshot();
        let entity = self.spawn_bundle((bun, SceneSignal(send))).id();
        let _ = recv.await;
        AsyncEntity(entity)
    }
}
