use std::{
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    time::Duration,
};

use bevy::MinimalPlugins;
use bevy_app::{App, PostUpdate, PreUpdate, Startup, Update};
use bevy_core::FrameCountPlugin;
use bevy_defer::{
    access::AsyncWorld,
    async_system,
    async_systems::AsyncSystems,
    signal_ids,
    signals::{Signal, SignalSender},
    AppReactorExtension, AsyncExtension, AsyncPlugin,
};
use bevy_defer::{signals::Signals, systems::run_async_executor};
use bevy_ecs::{
    component::Component,
    event::{Event, EventWriter},
    query::With,
    schedule::IntoSystemConfigs,
    system::{Commands, Local, Query},
};
use bevy_tasks::futures_lite::StreamExt;
use bevy_time::TimePlugin;
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

pub fn init(mut commands: Commands) {
    let signal = Signal::default();
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
            world.named_signal::<Message>("Alice").poll().await,
            "Hello, Alice."
        );
        world.sleep(Duration::from_millis(16)).await;
        world
            .named_signal::<Message>("Bob")
            .send("Hello, Bob.".to_owned());
        ALICE.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(16)).await;
        world
            .named_signal::<Message>("Alice")
            .send("Hello, Alice.".to_owned());
        assert_eq!(
            world.named_signal::<Message>("Bob").poll().await,
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
        assert_eq!(
            world.event_stream::<AliceChat>().next().await.unwrap().0,
            "Hello, Alice."
        );
        world.sleep(Duration::from_millis(16)).await;
        world.send_event(BobChat("Hello, Bob.".to_owned()))?;
        ALICE.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = AsyncWorld;
        world.sleep(Duration::from_millis(16)).await;
        world.send_event(AliceChat("Hello, Alice.".to_owned()))?;
        assert_eq!(
            world.event_stream::<BobChat>().next().await.unwrap().0,
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
        let mut stream = AsyncWorld.event_stream::<Chat>();
        assert_eq!(stream.next().await, Some(Chat('r')));
        assert_eq!(stream.next().await, Some(Chat('u')));
        assert_eq!(stream.next().await, Some(Chat('s')));
        assert_eq!(stream.next().await, Some(Chat('t')));
        assert_eq!(stream.next().await, Some(Chat(' ')));
        assert_eq!(stream.next().await, Some(Chat('n')));
        assert_eq!(stream.next().await, Some(Chat(' ')));
        assert_eq!(stream.next().await, Some(Chat('b')));
        assert_eq!(stream.next().await, Some(Chat('e')));
        assert_eq!(stream.next().await, Some(Chat('v')));
        assert_eq!(stream.next().await, Some(Chat('y')));
        if DONE.swap(true, Ordering::Relaxed) {
            AsyncWorld.quit();
        }
        Ok(())
    });
    app.spawn_task(async {
        let mut stream = AsyncWorld
            .event_stream::<Chat>()
            .map(|c| c.0.to_ascii_uppercase());
        assert_eq!(stream.next().await, Some('R'));
        assert_eq!(stream.next().await, Some('U'));
        assert_eq!(stream.next().await, Some('S'));
        assert_eq!(stream.next().await, Some('T'));
        assert_eq!(stream.next().await, Some(' '));
        assert_eq!(stream.next().await, Some('N'));
        assert_eq!(stream.next().await, Some(' '));
        assert_eq!(stream.next().await, Some('B'));
        assert_eq!(stream.next().await, Some('E'));
        assert_eq!(stream.next().await, Some('V'));
        assert_eq!(stream.next().await, Some('Y'));
        if DONE.swap(true, Ordering::Relaxed) {
            AsyncWorld.quit();
        }
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
        let mut stream = AsyncWorld.event_stream::<IntegerEvent>();
        while let Some(val) = stream.next().await {
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
