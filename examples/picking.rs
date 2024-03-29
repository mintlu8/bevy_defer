use bevy::{prelude::*, sprite::SpriteBundle};
use bevy_defer::{async_system, async_systems::AsyncSystems, picking::{picking_reactor, PickingInteractionChange, PickingSelected}, signals::{Signal, Signals}, AsyncPlugin};
use bevy_mod_picking::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DefaultPickingPlugins)
        .add_plugins(AsyncPlugin::default_settings())
        .add_systems(Update, picking_reactor)
        .insert_resource(DebugPickingMode::Normal)
        .add_systems(Startup, setup)
        .run();
}

/// Set up a simple 2D scene
fn setup(
    mut commands: Commands,
) {
    commands.spawn(Camera2dBundle::default());
    commands.spawn((
        SpriteBundle{
            sprite: Sprite { 
                color: Color::RED, 
                custom_size: Some(Vec2::new(200.0, 200.0)), 
                ..Default::default()
            },
            ..Default::default()
        },
        PickableBundle::default(),
        Signals::from_sender::<PickingInteractionChange>(Signal::new((PickingInteraction::None, PickingInteraction::None)))
            .with_sender::<PickingSelected>(Signal::new(false)),
        AsyncSystems::from_iter([
            async_system!(|sender: Sender<PickingInteractionChange>, sp: AsyncComponent<Sprite>| {
                match sender.recv().await.1 {
                    PickingInteraction::Pressed => sp.set(|x| x.color = Color::BLUE).await?,
                    _ => sp.set(|x| x.color = Color::RED).await?,
                };
            }),
            async_system!(|sender: Sender<PickingSelected>, sp: AsyncComponent<Sprite>| {
                if sender.recv().await{
                    sp.set(|s| s.custom_size.as_mut().unwrap().x = 400.0).await?;
                } else {
                    sp.set(|s| s.custom_size.as_mut().unwrap().x = 200.0).await?;
                }
            }),
        ])
    ));
    
}