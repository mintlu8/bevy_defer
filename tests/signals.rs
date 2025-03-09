use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use async_shared::Value;
use bevy::core::FrameCountPlugin;
use bevy::prelude::*;
use bevy::time::TimePlugin;
use bevy::MinimalPlugins;
#[allow(deprecated)]
use bevy_defer::{
    access::AsyncWorld, signal_ids, signals::SignalSender, AppReactorExtension, AsyncExtension,
    AsyncPlugin,
};
use bevy_defer::{signals::Signals, systems::run_async_executor};
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
    app.add_plugins(TimePlugin);
    app.add_plugins(FrameCountPlugin);
    app.add_plugins(AsyncPlugin::default_settings())
        .add_systems(Startup, init)
        .add_systems(Update, update.before(run_async_executor));
    app.update();
    app.update();
    app.update();
    app.update();
    app.update();
    assert!(LOCK.load(Ordering::SeqCst))
}

#[allow(deprecated)]
pub fn init(mut commands: Commands) {
    let signal = Arc::new(Value::default());
    commands.spawn((Marker1, Signals::from_sender::<SigText>(signal.clone())));
    commands.spawn((
        Marker2,
        Signals::from_receiver::<SigText>(signal.clone()),
        AsyncSystems::from_single(async_system!(|sig: Receiver<SigText>| {
            let mut stream = sig.into_stream();
            assert_eq!(stream.next().await, Some("hello"));
            assert_eq!(stream.next().await, Some("rust"));
            assert_eq!(stream.next().await, Some("and"));
            assert_eq!(stream.next().await, Some("bevy"));
            LOCK.store(true, Ordering::SeqCst)
        })),
    ));
}

fn update(mut i: Local<usize>, q: Query<SignalSender<SigText>, With<Marker1>>) {
    let s = ["hello", "rust", "and", "bevy"];
    if let Some(s) = s.get(*i) {
        dbg!(q.single().send(*s));
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
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        let world = AsyncWorld;
        assert_eq!(
            world.named_signal::<Message>("Alice").read_async().await,
            "Hello, Alice."
        );
        world.sleep(Duration::from_millis(16)).await;
        world
            .named_signal::<Message>("Bob")
            .write("Hello, Bob.".to_owned());
        ALICE.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(16)).await;
        world
            .named_signal::<Message>("Alice")
            .write("Hello, Alice.".to_owned());
        assert_eq!(
            world.named_signal::<Message>("Bob").read_async().await,
            "Hello, Bob."
        );
        BOB.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(100)).await;
        world.quit();
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
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_event::<AliceChat>();
    app.add_event::<BobChat>();
    app.react_to_event::<AliceChat>();
    app.react_to_event::<BobChat>();
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        let world = AsyncWorld;
        assert_eq!(world.next_event::<AliceChat>().await.0, "Hello, Alice.");
        world.sleep(Duration::from_millis(16)).await;
        world.send_event(BobChat("Hello, Bob.".to_owned()))?;
        ALICE.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(16)).await;
        world.send_event(AliceChat("Hello, Alice.".to_owned()))?;
        assert_eq!(world.next_event::<BobChat>().await.0, "Hello, Bob.");
        BOB.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(100)).await;
        world.quit();
        Ok(())
    });
    app.run();
    assert!(ALICE.load(Ordering::SeqCst));
    assert!(BOB.load(Ordering::SeqCst));
}

#[derive(Debug, Clone, Event, PartialEq)]
pub struct Chat(char);

#[test]
pub fn stream() {
    static DONE: AtomicBool = AtomicBool::new(false);
    let mut app = App::new();
    app.add_event::<Chat>();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AsyncPlugin::default_settings());
    app.react_to_event::<Chat>();
    app.spawn_task(async {
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('r'));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('u'));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('s'));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('t'));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat(' '));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('n'));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat(' '));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('b'));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('e'));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('v'));
        assert_eq!(AsyncWorld.next_event::<Chat>().await, Chat('y'));
        DONE.store(true, Ordering::Release);
        AsyncWorld.quit();
        Ok(())
    });

    let mut msgs = vec!["bevy", "n ", "rust "];

    app.add_systems(Update, move |mut w: EventWriter<Chat>| {
        if let Some(s) = msgs.pop() {
            w.send_batch(s.chars().map(Chat));
        };
    });

    app.run();
    assert!(DONE.load(Ordering::SeqCst));
}

#[derive(Debug, Event, Clone)]
struct IntegerEvent(u32);

static CELL: AtomicU32 = AtomicU32::new(0);
#[test]
pub fn event_stream() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_event::<IntegerEvent>();
    app.react_to_event::<IntegerEvent>();
    app.spawn_task(async {
        let mut i = 0;
        loop {
            let val = AsyncWorld.next_event::<IntegerEvent>().await;
            assert_eq!(val.0, i);
            i += 1;
            if i > 100 {
                break;
            }
        }
        AsyncWorld.quit();
        Ok(())
    });
    app.add_systems(Update, sys_update);
    app.add_systems(Update, sys_update);
    app.add_systems(Update, sys_update);
    app.add_systems(PreUpdate, sys_update);
    app.add_systems(PostUpdate, sys_update);
    app.run();
}

fn sys_update(mut event: EventWriter<IntegerEvent>) {
    for _ in 0..fastrand::usize(0..5) {
        event.send(IntegerEvent(CELL.fetch_add(1, Ordering::SeqCst)));
    }
}
