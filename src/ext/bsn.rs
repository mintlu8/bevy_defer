use bevy::prelude::World;
use bevy::scene::{Scene, WorldSceneExt};
use crate::access::AsyncEntity;
use crate::{AccessError, AccessResult, AsyncWorld};
use crate::executor::with_world_mut;

impl AsyncWorld {
    /// Spawn a new [`Entity`] from a given BSN Scene
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// AsyncWorld.spawn_bsn(bsn! {
    ///     Str("Ferris")
    ///     Int(4)
    /// })
    /// # );
    /// ```
    pub fn spawn_bsn(&self, scene: impl Scene) -> AccessResult<AsyncEntity> {
        Ok(self.entity(with_world_mut(move |world: &mut World| {
            world.spawn_scene(scene).map(|e| e.id()).map_err(|_| { AccessError::SpawnSceneFailed })
        })?))
    }
}