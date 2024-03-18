use std::{convert::Infallible, sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};

use bevy::MinimalPlugins;
use bevy_app::App;
use bevy_asset::{Asset, AssetApp, AssetLoader, AssetPlugin, AsyncReadExt, Handle};
use bevy_defer::{world, AsyncExtension, DefaultAsyncPlugin};
use bevy_reflect::TypePath;

#[derive(Asset, TypePath)]
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
    app.add_plugins(DefaultAsyncPlugin);
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = world();
        let one: Handle<JsonNumber> = world.load_asset("1.json").await?;
        let four: Handle<JsonNumber> = world.load_asset("4.json").await?;
        let sixty_nine: Handle<JsonNumber> = world.load_asset("69.json").await?;
        assert_eq!(world.asset(one, |x| x.0).await?, 1);
        assert_eq!(world.asset(four, |x| x.0).await?, 4);
        assert_eq!(world.asset(sixty_nine, |x| x.0).await?, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(100)).await;
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
    app.add_plugins(DefaultAsyncPlugin);
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = world();
        let (one, four, sixty_nine) = futures::try_join!(
            world.load_asset::<JsonNumber>("1.json"),
            world.load_asset::<JsonNumber>("4.json"),
            world.load_asset::<JsonNumber>("69.json"),
        )?;

        let (one, four, sixty_nine) = futures::try_join!(
            world.asset(one, |x| x.0),
            world.asset(four, |x| x.0),
            world.asset(sixty_nine, |x| x.0),
        )?;
        assert_eq!(one, 1);
        assert_eq!(four, 4);
        assert_eq!(sixty_nine, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(100)).await;
        world.quit().await;
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(100)).await;
        world.quit().await;
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}

#[test]
pub fn direct(){
    let mut app = App::new();
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(MinimalPlugins);
    app.init_asset_loader::<JsonNumberLoader>();
    app.init_asset::<JsonNumber>();
    app.add_plugins(DefaultAsyncPlugin);
    let lock = Arc::new(AtomicBool::new(false));
    let lock2 = lock.clone();
    app.spawn_task(async move {
        let world = world();
        let (one, four, sixty_nine) = futures::try_join!(
            world.load_direct::<JsonNumber, _>("1.json", |x| x.0),
            world.load_direct::<JsonNumber, _>("4.json", |x| x.0),
            world.load_direct::<JsonNumber, _>("69.json", |x| x.0),
        )?;
        assert_eq!(one, 1);
        assert_eq!(four, 4);
        assert_eq!(sixty_nine, 69);
        lock2.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(100)).await;
        world.quit().await;
        Ok(())
    });
    app.run();
    assert!(lock.load(Ordering::Relaxed));
}