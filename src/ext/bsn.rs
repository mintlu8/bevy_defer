use crate::access::AsyncEntity;
use crate::executor::with_world_mut;
use crate::{AccessError, AccessResult, AsyncWorld};
use bevy::prelude::World;
use bevy::scene::{Scene, WorldSceneExt};

impl AsyncWorld {
    /// Spawn a new [`Entity`] from a given BSN Scene
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_bsn_spawn!(
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

/// For doctests only.
#[doc(hidden)]
#[allow(unused)]
#[macro_export]
macro_rules! test_bsn_spawn {
    ($expr: expr) => {{
        use ::bevy::prelude::*;
        use ::bevy_defer::access::*;
        use ::bevy_defer::*;
        use bevy::state::app::StatesPlugin;
        use ::bevy::scene::ScenePlugin;
        #[derive(Debug, Default, Clone, Copy, Component, TypePath)]
        pub struct Int(i32);

        #[derive(Debug, Default, Clone, Copy, Component, TypePath)]
        pub struct Str(&'static str);

        let mut app = ::bevy::app::App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(AssetPlugin::default());
        app.add_plugins(StatesPlugin);
        app.add_plugins(ScenePlugin::default());
        app.add_plugins(bevy_defer::AsyncPlugin::default_settings());
        app.spawn_task(async move {
            $expr;
            AsyncWorld.quit();
            Ok(())
        });
        app.run();
    }};
}
