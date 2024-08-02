use bevy::{prelude::*, sprite::SpriteBundle};
use bevy_defer::{
    async_system,
    async_systems::AsyncSystems,
    signals::{Signals, Value},
    AsyncPlugin,
};
use bevy_defer_picking::{react_to_picking, PickingInteractionChange, PickingSelected};
use bevy_mod_picking::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DefaultPickingPlugins)
        .add_plugins(AsyncPlugin::default_settings())
        .add_systems(Update, react_to_picking)
        .insert_resource(DebugPickingMode::Normal)
        .add_systems(Startup, setup)
        .run();
}

/// Set up a simple 2D scene
fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: Srgba::RED.into(),
                custom_size: Some(Vec2::new(200.0, 200.0)),
                ..Default::default()
            },
            ..Default::default()
        },
        PickableBundle::default(),
        Signals::from_sender::<PickingInteractionChange>(Value::new_arc())
            .with_sender::<PickingSelected>(Value::new_arc()),
        AsyncSystems::from_iter([
            async_system!(|sender: Sender<PickingInteractionChange>,
                           sp: AsyncComponent<Sprite>| {
                match sender.recv().await.to {
                    PickingInteraction::Pressed => sp.set(|x| x.color = Srgba::BLUE.into())?,
                    _ => sp.set(|x| x.color = Srgba::RED.into())?,
                };
            }),
            async_system!(
                |sender: Sender<PickingSelected>, sp: AsyncComponent<Sprite>| {
                    if sender.recv().await {
                        sp.set(|s| s.custom_size.as_mut().unwrap().x = 400.0)?;
                    } else {
                        sp.set(|s| s.custom_size.as_mut().unwrap().x = 200.0)?;
                    }
                }
            ),
        ]),
    ));
}
