use std::{sync::atomic::{AtomicBool, Ordering}, time::Duration};

use bevy::MinimalPlugins;
use bevy_app::{App, Startup, Update};
use bevy_ecs::{component::Component, event::Event, query::With, system::{Commands, Local, Query}};
use bevy_defer::{async_system, signal_ids, world, AsyncExtension, AsyncSystems, DefaultAsyncPlugin};
use bevy_defer::signals::{SignalSender, Signals, TypedSignal};
signal_ids! {
    SigText: &'static str,
}

#[derive(Component)]
pub struct Marker1;

#[derive(Component)]
pub struct Marker2;

static LOCK: AtomicBool = AtomicBool::new(false);

#[test]
pub fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultAsyncPlugin)
        .add_systems(Startup, init)
        .add_systems(Update, update);
    app.update();
    app.update();
    app.update();
    app.update();
    assert!(LOCK.load(Ordering::SeqCst))
}

pub fn init(mut commands: Commands) {
    let signal = TypedSignal::new();
    commands.spawn((
        Marker1, 
        Signals::from_sender::<SigText>(signal.clone())
    ));
    commands.spawn((
        Marker2, 
        Signals::from_receiver::<SigText>(signal.clone()),
        AsyncSystems::from_single(
            async_system!(|sig: Receiver<SigText>|{
                let sig = &sig;
                assert_eq!(sig.await, "hello");
                assert_eq!(sig.await, "rust");
                assert_eq!(sig.await, "and");
                assert_eq!(sig.await, "bevy");
                LOCK.store(true, Ordering::SeqCst)
            })
        )
    ));
}

fn update(mut i: Local<usize>, q: Query<SignalSender<SigText>, With<Marker1>>) {
    let s = ["hello", "rust", "and", "bevy"];
    if let Some(s) = s.get(*i){
        q.single().send(*s);
    }
    *i += 1;
}

signal_ids! {
    Message: String,
}

#[test]
pub fn chat() {
    static ALICE: AtomicBool = AtomicBool::new(false);
    static BOB: AtomicBool = AtomicBool::new(false);
    let mut app = App::new();
    app.add_plugins(DefaultAsyncPlugin);
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        let world = world();
        assert_eq!(world.poll::<Message>("Alice").await, "Hello, Alice.");
        world.sleep(Duration::from_millis(16)).await;
        world.send::<Message>("Bob", "Hello, Bob.".to_owned()).await;
        ALICE.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(16)).await;
        world.send::<Message>("Alice", "Hello, Alice.".to_owned()).await;
        assert_eq!(world.poll::<Message>("Bob").await, "Hello, Bob.");
        BOB.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(100)).await;
        world.quit().await;
        Ok(())
    });
    app.run();
    assert!(ALICE.load(Ordering::SeqCst));
    assert!(BOB.load(Ordering::SeqCst));
}

#[derive(Debug, Clone, Event)]
pub struct AliceChat(String);


#[derive(Debug, Clone, Event)]
pub struct BobChat(String);

#[test]
pub fn events() {
    static ALICE: AtomicBool = AtomicBool::new(false);
    static BOB: AtomicBool = AtomicBool::new(false);
    let mut app = App::new();
    app.add_plugins(DefaultAsyncPlugin);
    app.add_event::<AliceChat>();
    app.add_event::<BobChat>();
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        let world = world();
        assert_eq!(world.poll_event::<AliceChat>().await.0, "Hello, Alice.");
        world.sleep(Duration::from_millis(16)).await;
        world.send_event(BobChat("Hello, Bob.".to_owned())).await?;
        ALICE.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(16)).await;
        world.send_event(AliceChat("Hello, Alice.".to_owned())).await?;
        assert_eq!(world.poll_event::<BobChat>().await.0, "Hello, Bob.");
        BOB.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(100)).await;
        world.quit().await;
        Ok(())
    });
    app.run();
    assert!(ALICE.load(Ordering::SeqCst));
    assert!(BOB.load(Ordering::SeqCst));
}
