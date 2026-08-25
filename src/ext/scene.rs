use bevy::scene::{Scene, WorldSceneExt};

use crate::{access::AsyncEntity, executor::with_world_mut, AccessError, AccessResult, AsyncWorld};

impl AsyncWorld {
    /// Spawn a [`Scene`] using `bsn`.
    pub fn spawn_scene(&self, scene: impl Scene) -> AccessResult<AsyncEntity> {
        with_world_mut(|world| {
            world
                .spawn_scene(scene)
                .map(|e| AsyncEntity(e.id()))
                .map_err(|_| AccessError::Custom("`spawn_scene` failed."))
        })
    }
}
