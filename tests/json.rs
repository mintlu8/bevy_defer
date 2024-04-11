use std::{convert::Infallible, sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

use bevy::MinimalPlugins;
use bevy_app::App;
use bevy_asset::{Asset, AssetApp, AssetLoader, AssetPlugin, AsyncReadExt};
use bevy_defer::{world, AsyncExtension, AsyncPlugin};
use bevy_reflect::TypePath;

#[derive(Debug, Asset, TypePath, Clone, PartialEq)]
pub struct JsonNumber(i64);

#[derive(Default)]
pub struct JsonNumberLoader;

impl AssetLoader for JsonNumberLoader {
    type Asset = JsonNumber;

    type Settings = ();

    type Error = Infallible;

    fn load<'a>(
        &'a self,
        reader: &'a mut bevy_asset::io::Reader,
        _: &'a Self::Settings,
        _: &'a mut bevy_asset::LoadContext,
    ) -> bevy_asset::BoxedFuture<'a, Result<Self::Asset, Self::Error>> {
        Box::pin(async {
            let mut buf = String::new();
            reader.read_to_string(&mut buf).await.unwrap();
            Ok(JsonNumber(buf.parse().unwrap()))
        })
    }
}

#[test]
pub fn procedural(){
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = world();
        let one = world.load_asset::<JsonNumber>("1.json");
        let four = world.load_asset::<JsonNumber>("4.json");
        let sixty_nine = world.load_asset::<JsonNumber>("69.json");
        assert_eq!(one.get(|x| x.0).await?, 1);
        assert_eq!(four.get(|x| x.0).await?, 4);
        assert_eq!(sixty_nine.get(|x| x.0).await?, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(500)).await;
        world.quit().await;
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}

#[test]
pub fn concurrent(){
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = world();
        let (one, four, sixty_nine) = (
            world.load_asset::<JsonNumber>("1.json"),
            world.load_asset::<JsonNumber>("4.json"),
            world.load_asset::<JsonNumber>("69.json"),
        );

        let (one, four, sixty_nine) = futures::try_join!(
            one.get(|x| x.0),
            four.get(|x| x.0),
            sixty_nine.get(|x| x.0),
        )?;
        assert_eq!(one, 1);
        assert_eq!(four, 4);
        assert_eq!(sixty_nine, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(500)).await;
        world.quit().await;
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}

#[test]
pub fn cloned(){
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = world();
        let (one, four, sixty_nine) = (
            world.load_asset::<JsonNumber>("1.json"),
            world.load_asset::<JsonNumber>("4.json"),
            world.load_asset::<JsonNumber>("69.json"),
        );
        let (one, four, sixty_nine) = futures::try_join!(
            one.clone_on_load(),
            four.clone_on_load(),
            sixty_nine.clone_on_load(),
        )?;
        assert_eq!(one.0, 1);
        assert_eq!(four.0, 4);
        assert_eq!(sixty_nine.0, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(500)).await;
        world.quit().await;
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}

#[test]
pub fn take(){
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = world();
        let (one, four, sixty_nine) = (
            world.load_asset::<JsonNumber>("1.json"),
            world.load_asset::<JsonNumber>("4.json"),
            world.load_asset::<JsonNumber>("69.json"),
        );
        let (one, four, sixty_nine) = futures::try_join!(
            one.take(),
            four.take(),
            sixty_nine.take(),
        )?;
        assert_eq!(one.0, 1);
        assert_eq!(four.0, 4);
        assert_eq!(sixty_nine.0, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(500)).await;
        world.quit().await;
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}