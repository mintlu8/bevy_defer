use bevy::prelude::*;
use bevy_defer::{world, AsyncExtension, DefaultAsyncPlugin};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DefaultAsyncPlugin)
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
            }).await;
            entity.component::<Transform>().interpolate_to(
                |t| t.translation, 
                |t, v| t.translation = v, 
                |x| x * x, 
                2.0, 
                Vec3::new(-100.0, -100.0, 0.0)
            ).await.unwrap();
            entity.component::<Transform>().interpolate_to(
                |t| t.translation, 
                |t, v| t.translation = v, 
                |x| x * x, 
                2.0, 
                Vec3::new(-100.0, 100.0, 0.0)
            ).await.unwrap();
            entity.component::<Transform>().interpolate_to(
                |t| t.translation, 
                |t, v| t.translation = v, 
                |x| x * x, 
                2.0, 
                Vec3::new(100.0, 0.0, 0.0)
            ).await.unwrap();
            entity.component::<Transform>().interpolate_to(
                |t| t.translation, 
                |t, v| t.translation = v, 
                |x| x * x, 
                2.0, 
                Vec3::new(0.0, 0.0, 0.0)
            ).await.unwrap();
            Ok(())
        })
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}