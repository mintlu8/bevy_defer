use bevy_app::App;
use bevy_core::FrameCountPlugin;
use bevy_defer::{access::AsyncWorld, AsyncAccess, AsyncExtension, AsyncPlugin};
use bevy_ecs::component::Component;
use bevy_time::TimePlugin;
use std::sync::atomic::{AtomicBool, Ordering};

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
    let a = app.world_mut().spawn(Int(69)).id();
    let b = app.world_mut().spawn(String("ferris")).id();

    static LOCK: AtomicBool = AtomicBool::new(false);
    app.spawn_task(async move {
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);
        assert_eq!(AsyncWorld.entity(a).component::<Int>().get(|x| x.0)?, 69);

        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        assert_eq!(
            AsyncWorld.entity(b).component::<String>().get(|x| x.0)?,
            "ferris"
        );
        LOCK.store(true, Ordering::Relaxed);
        Ok(())
    });
    // With world access can resolve in one frame.
    app.update();
    assert!(LOCK.load(Ordering::SeqCst))
}
