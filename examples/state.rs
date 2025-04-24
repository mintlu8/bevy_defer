use bevy::{
    color::palettes::css::{BLUE, GREEN, RED},
    prelude::*,
};
use bevy_defer::{
    access::AsyncWorld, AccessResult, AppReactorExtension, AsyncExtension, AsyncPlugin,
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
        .react_to_state::<MainState>()
        .react_to_state::<Spin>()
        .add_systems(Startup, setup)
        .add_systems(OnEnter(MainState::Red), |world: &mut World| {
            let _ = world.spawn_state_scoped(MainState::Red, async {
                AsyncWorld.run_system_cached_with(set_color, RED.into())?;
                AccessResult::Ok(())
            });
        })
        .add_systems(OnEnter(MainState::Green), |world: &mut World| {
            let _ = world.spawn_state_scoped(MainState::Green, async {
                AsyncWorld.run_system_cached_with(set_color, GREEN.into())?;
                AccessResult::Ok(())
            });
        })
        .add_systems(OnEnter(MainState::Blue), |world: &mut World| {
            let _ = world.spawn_state_scoped(MainState::Blue, async {
                AsyncWorld.run_system_cached_with(set_color, BLUE.into())?;
                AccessResult::Ok(())
            });
        })
        .add_systems(OnEnter(Spin), |world: &mut World| {
            // Coroutine :)
            let _ = world.spawn_state_scoped(Spin, async {
                loop {
                    AsyncWorld.run_system_cached(spin)?;
                    AsyncWorld.yield_now().await;
                }
            });
        })
        .spawn_task(async {
            loop {
                AsyncWorld.sleep(2.).await;
                AsyncWorld.set_state(MainState::Green)?;
                AsyncWorld.sleep(2.).await;
                AsyncWorld.set_state(MainState::Blue)?;
                AsyncWorld.sleep(2.).await;
                AsyncWorld.set_state(MainState::Red)?;
            }
        })
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn(Sprite {
        color: RED.into(),
        custom_size: Some(Vec2::new(100., 100.)),
        ..Default::default()
    });
}

fn set_color(color: In<Color>, mut query: Query<&mut Sprite>) {
    query.single_mut().unwrap().color = color.0;
}

fn spin(time: ResMut<Time>, mut query: Query<&mut Transform, With<Sprite>>) {
    query.single_mut().unwrap().rotate_z(time.delta_secs());
}
