use bevy::{
    sprite::{Sprite, SpriteBundle},
    transform::components::{GlobalTransform, Transform},
    MinimalPlugins,
};
use bevy_app::App;
use bevy_core::FrameCountPlugin;
use bevy_defer::{system_future, world, AsyncAccess, AsyncExtension, AccessError, AsyncPlugin};
use bevy_ecs::{component::Component, query::With};
use bevy_time::TimePlugin;
use std::sync::{
    atomic::{AtomicBool, AtomicI64, Ordering},
    Arc,
};

#[derive(Component)]
pub struct Int(i32);

#[derive(Component)]
pub struct String(&'static str);

#[test]
pub fn main() {
    let mut app = App::new();
    app.add_plugins(TimePlugin);
    app.add_plugins(FrameCountPlugin);
    app.add_plugins(AsyncPlugin::default_settings());
    let a = app.world.spawn(Int(69)).id();
    let b = app.world.spawn(String("ferris")).id();

    static LOCK: AtomicBool = AtomicBool::new(false);
    app.spawn_task(async move {
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(world().entity(a).component::<Int>().get(|x| x.0)?, 69);

        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            world().entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        LOCK.store(true, Ordering::Relaxed);
        Ok(())
    });
    // With world access can resolve in one frame.
    app.update();
    assert!(LOCK.load(Ordering::SeqCst))
}

#[test]
pub fn system_future() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicI64::new(0));
    let lock2 = lock.clone();
    app.spawn_task(system_future!(|w: AsyncWorldMut,
                                   q: AsyncQuery<
        (&mut Transform, &Sprite),
        With<GlobalTransform>,
    >| {
        let cloned = lock.clone();
        w.spawn_bundle(SpriteBundle::default());
        q.for_each(move |_| {
            cloned.fetch_add(1, Ordering::Relaxed);
        });
        if lock.load(Ordering::Relaxed) > 10 {
            w.quit();
        }
        w.yield_now().await;
        // Uses AsyncSystem semantics so failure does not cancel the system.
        Err(AccessError::ComponentNotFound)?;
    }));
    app.run();
    assert!(lock2.load(Ordering::Relaxed) > 10);
}
