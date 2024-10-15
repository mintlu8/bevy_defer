use std::{
    convert::Infallible,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use bevy::{asset::{AssetLoader, AsyncReadExt}, utils::ConditionalSendFuture};
use bevy::prelude::*;
use bevy_defer::{access::AsyncWorld, AsyncAccess, AsyncExtension, AsyncPlugin};

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
        reader: &'a mut bevy::asset::io::Reader,
        _: &'a Self::Settings,
        _: &'a mut bevy::asset::LoadContext,
    ) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async {
            let mut buf = String::new();
            reader.read_to_string(&mut buf).await.unwrap();
            Ok(JsonNumber(buf.parse().unwrap()))
        })
    }
}

#[test]
pub fn procedural() {
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = AsyncWorld;
        let one = world.load_asset::<JsonNumber>("1.json");
        let four = world.load_asset::<JsonNumber>("4.json");
        let sixty_nine = world.load_asset::<JsonNumber>("69.json");
        assert_eq!(one.get_on_load(|x| x.0).await?, 1);
        assert_eq!(four.get_on_load(|x| x.0).await?, 4);
        assert_eq!(sixty_nine.get_on_load(|x| x.0).await?, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(500)).await;
        world.quit();
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}

#[test]
pub fn concurrent() {
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = AsyncWorld;
        let (one, four, sixty_nine) = (
            world.load_asset::<JsonNumber>("1.json"),
            world.load_asset::<JsonNumber>("4.json"),
            world.load_asset::<JsonNumber>("69.json"),
        );

        let (one, four, sixty_nine) = futures::try_join!(
            one.get_on_load(|x| x.0),
            four.get_on_load(|x| x.0),
            sixty_nine.get_on_load(|x| x.0),
        )?;
        assert_eq!(one, 1);
        assert_eq!(four, 4);
        assert_eq!(sixty_nine, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(500)).await;
        world.quit();
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}

#[test]
pub fn cloned() {
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = AsyncWorld;
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
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(500)).await;
        world.quit();
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}

#[test]
pub fn take() {
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(AsyncPlugin::default_settings());
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = AsyncWorld;
        let (one, four, sixty_nine) = (
            world.load_asset::<JsonNumber>("1.json"),
            world.load_asset::<JsonNumber>("4.json"),
            world.load_asset::<JsonNumber>("69.json"),
        );
        let (one, four, sixty_nine) = futures::try_join!(
            one.take_on_load(),
            four.take_on_load(),
            sixty_nine.take_on_load(),
        )?;
        assert_eq!(one.0, 1);
        assert_eq!(four.0, 4);
        assert_eq!(sixty_nine.0, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(500)).await;
        world.quit();
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}
