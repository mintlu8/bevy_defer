use std::time::Duration;

use bevy::prelude::*;
use bevy_defer::{
    cancellation::Cancellation, spawn, tween::Playback, world, AsyncAccess, AsyncExtension,
    AsyncPlugin,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AsyncPlugin::default_settings())
        .add_systems(Startup, setup)
        .spawn_task(async {
            let world = world();
            let entity = world.spawn_bundle(SpriteBundle {
                sprite: Sprite {
                    color: Color::BLUE,
                    custom_size: Some(Vec2::splat(40.0)),
                    ..Default::default()
                },
                ..Default::default()
            });
            entity
                .component::<Transform>()
                .interpolate_to(
                    Vec3::new(-100.0, -100.0, 0.0),
                    |t| t.translation,
                    |t, v| t.translation = v,
                    |x| x * x,
                    2.0,
                    (),
                )
                .await
                .unwrap();
            entity
                .component::<Transform>()
                .interpolate_to(
                    Vec3::new(-100.0, 100.0, 0.0),
                    |t| t.translation,
                    |t, v| t.translation = v,
                    |x| x * x,
                    2.0,
                    (),
                )
                .await
                .unwrap();
            entity
                .component::<Transform>()
                .interpolate_to(
                    Vec3::new(100.0, 0.0, 0.0),
                    |t| t.translation,
                    |t, v| t.translation = v,
                    |x| x * x,
                    2.0,
                    (),
                )
                .await
                .unwrap();
            entity
                .component::<Transform>()
                .interpolate_to(
                    Vec3::new(0.0, 0.0, 0.0),
                    |t| t.translation,
                    |t, v| t.translation = v,
                    |x| x * x,
                    2.0,
                    (),
                )
                .await
                .unwrap();
            Ok(())
        })
        .spawn_task(async {
            let world = world();
            let entity = world.spawn_bundle(SpriteBundle {
                sprite: Sprite {
                    color: Color::GREEN,
                    custom_size: Some(Vec2::splat(40.0)),
                    ..Default::default()
                },
                transform: Transform::from_translation(Vec3::new(0.0, 200.0, 0.0)),
                ..Default::default()
            });
            let cancel = Cancellation::new();
            let comp = entity.component::<Transform>();
            spawn(comp.interpolate(
                |x| Vec3::new(0.0, 200.0, 0.0).lerp(Vec3::new(0.0, -200.0, 0.0), x),
                |t, v| t.translation = v,
                |x| x * x,
                2.0,
                Playback::Bounce,
                &cancel,
            ));
            world.sleep(Duration::from_secs(6)).await;
            cancel.cancel();
            comp.interpolate_to(
                Vec3::new(0.0, 0.0, 0.0),
                |t| t.translation,
                |t, v| t.translation = v,
                |x| x * x,
                2.0,
                (),
            )
            .await
            .unwrap();

            let cancel = Cancellation::new();
            let comp = entity.component::<Transform>();
            spawn(comp.interpolate(
                |x| Vec3::new(0.0, 0.0, 0.0).lerp(Vec3::new(200.0, 0.0, 0.0), x),
                |t, v| t.translation = v,
                |x| x,
                2.0,
                Playback::Loop,
                &cancel,
            ));
            world.sleep(Duration::from_secs(6)).await;
            cancel.cancel();
            world.quit();
            Ok(())
        })
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}
