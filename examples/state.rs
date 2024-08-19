use std::time::Duration;

use bevy::{
    color::palettes::css::{BLUE, GREEN, RED},
    prelude::*,
};
use bevy_defer::{
    access::AsyncWorld, cancellation::Cancellation, tween::Playback, AccessResult, AsyncAccess,
    AsyncExecutor, AsyncExtension, AsyncPlugin,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum MainState {
    #[default]
    Red,
    Green,
    Blue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Spin;

impl ComputedStates for Spin {
    type SourceStates = MainState;

    fn compute(sources: Self::SourceStates) -> Option<Self> {
        match sources {
            MainState::Blue => Some(Spin),
            _ => None,
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AsyncPlugin::default_settings())
        .init_state::<MainState>()
        .add_computed_state::<Spin>()
        .add_systems(Startup, setup)
        .add_systems(OnEnter(MainState::Red), |world: &mut World| {
            let _ = world.spawn_state_scoped(MainState::Red, async {
                AsyncWorld.run_cached_system_with_input(set_color, RED.into())?;
                AccessResult::Ok(())
            });
        })
        .add_systems(OnEnter(MainState::Green), |world: &mut World| {
            let _ = world.spawn_state_scoped(MainState::Green, async {
                AsyncWorld.run_cached_system_with_input(set_color, GREEN.into())?;
                AccessResult::Ok(())
            });
        })
        .add_systems(OnEnter(MainState::Blue), |world: &mut World| {
            let _ = world.spawn_state_scoped(MainState::Blue, async {
                AsyncWorld.run_cached_system_with_input(set_color, BLUE.into())?;
                AccessResult::Ok(())
            });
        })
        .spawn_task(async {
            AsyncWorld.sleep(2.).await;
            AsyncWorld.set_state(MainState::Green)?;
            AsyncWorld.sleep(2.).await;
            AsyncWorld.set_state(MainState::Blue)?;
            AsyncWorld.sleep(2.).await;
            AsyncWorld.set_state(MainState::Red)?;
            AccessResult::Ok(())
        })
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
    commands.spawn(SpriteBundle {
        sprite: Sprite {
            color: RED.into(),
            custom_size: Some(Vec2::new(100., 100.)),
            ..Default::default()
        },
        ..Default::default()
    });
}

fn set_color(color: In<Color>, mut query: Query<&mut Sprite>) {
    query.single_mut().color = color.0;
}
