use std::sync::atomic::{AtomicBool, Ordering};
use bevy_app::App;
use bevy_ecs::component::Component;
use bevy_defer::{world, AsyncExtension, AsyncPlugin};

#[derive(Component)]
pub struct Int(i32);

#[derive(Component)]
pub struct String(&'static str);


#[test]
pub fn main() {
    let mut app = App::new();
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
