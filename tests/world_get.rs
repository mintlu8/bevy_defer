use std::sync::{atomic::{AtomicBool, AtomicI64, Ordering}, Arc};
use bevy::{sprite::{Sprite, SpriteBundle}, transform::components::{GlobalTransform, Transform}, MinimalPlugins};
use bevy_app::App;
use bevy_core::FrameCountPlugin;
use bevy_ecs::{component::Component, query::With};
use bevy_defer::{system_future, world, AsyncAccess, AsyncExtension, AsyncFailure, AsyncPlugin};
use bevy_time::TimePlugin;

#[derive(Component)]
pub struct Int(i32);

#[derive(Component)]
pub struct String(&'static str);


#[test]
pub fn main() {
    let mut app = App::new();
    app.add_plugins(TimePlugin);
    app.add_plugins(FrameCountPlugin);
    app.add_plugins(AsyncPlugin::default_settings().with_world_access());
    let a = app.world.spawn(Int(69)).id();
    let b = app.world.spawn(String("ferris")).id();

    static LOCK: AtomicBool = AtomicBool::new(false);
    app.spawn_task(async move {
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);

        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        LOCK.store(true, Ordering::Relaxed);
        Ok(())
    });
    // With world access can resolve in one frame.
    app.update();
    assert!(LOCK.load(Ordering::SeqCst))
}

#[test]
pub fn control_group() {
    let mut app = App::new();
    app.add_plugins(TimePlugin);
    app.add_plugins(FrameCountPlugin);
    // Without world access cannot resolve in one frame.
    app.add_plugins(AsyncPlugin::default_settings());//.with_world_access());
    let a = app.world.spawn(Int(69)).id();
    let b = app.world.spawn(String("ferris")).id();

    static LOCK: AtomicBool = AtomicBool::new(false);
    app.spawn_task(async move {
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0).await?, 69);

        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        assert_eq!(world().entity(b).component::<String>().get(|x| x.0).await?, "ferris");
        LOCK.store(true, Ordering::Relaxed);
        Ok(())
    });
    // Without world access cannot resolve in one frame.
    app.update();
    assert!(!LOCK.load(Ordering::SeqCst));
    app.update();
    assert!(!LOCK.load(Ordering::SeqCst));
}


#[test]
pub fn system_future() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicI64::new(0));
    let lock2 = lock.clone();
    app.spawn_task(system_future!(
        |w: AsyncWorldMut, q: AsyncQuery<(&mut Transform, &Sprite), With<GlobalTransform>>| {
            w.spawn_bundle(SpriteBundle::default()).await;
            let cloned = lock.clone();
            q.for_each(move |_| {
                cloned.fetch_add(1, Ordering::Relaxed);
            }).await;
            if lock.load(Ordering::Relaxed) > 10 {
                w.quit().await;
            }
            // Uses AsyncSystem semantics so failure does not cancel the system.
            Err(AsyncFailure::ComponentNotFound)?;
        }
    ));
    app.run();
    assert!(lock2.load(Ordering::Relaxed) > 10);
}