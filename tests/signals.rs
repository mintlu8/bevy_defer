use std::{sync::atomic::{AtomicBool, Ordering}, time::Duration};

use bevy::MinimalPlugins;
use bevy_app::{App, Startup, Update};
use bevy_core::FrameCountPlugin;
use bevy_ecs::{component::Component, event::{Event, EventWriter}, query::With, system::{Commands, Local, Query}};
use bevy_defer::{async_system, async_systems::AsyncSystems, signal_ids, signals::{Signal, SignalSender}, world, AsyncExtension, AsyncPlugin};
use bevy_defer::signals::Signals;
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
        .add_systems(Update, update);
    app.update();
    app.update();
    app.update();
    app.update();
    assert!(LOCK.load(Ordering::SeqCst))
}

pub fn init(mut commands: Commands) {
    let signal = Signal::default();
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
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        let world = world();
        assert_eq!(world.signal::<Message>("Alice").poll().await, "Hello, Alice.");
        world.sleep(Duration::from_millis(16)).await;
        world.signal::<Message>("Bob").send("Hello, Bob.".to_owned());
        ALICE.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(16)).await;
        world.signal::<Message>("Alice").send("Hello, Alice.".to_owned());
        assert_eq!(world.signal::<Message>("Bob").poll().await, "Hello, Bob.");
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
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_event::<AliceChat>();
    app.add_event::<BobChat>();
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        let world = world();
        assert_eq!(world.event_reader::<AliceChat>().poll().await.0, "Hello, Alice.");
        world.sleep(Duration::from_millis(16)).await;
        world.send_event(BobChat("Hello, Bob.".to_owned())).await?;
        ALICE.store(true, Ordering::Relaxed);
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        world.sleep(Duration::from_millis(16)).await;
        world.send_event(AliceChat("Hello, Alice.".to_owned())).await?;
        assert_eq!(world.event_reader::<BobChat>().poll().await.0, "Hello, Bob.");
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

#[derive(Debug, Clone, Event, PartialEq)]
pub struct Chat(char);

#[test]
pub fn stream() {
    static DONE: AtomicBool = AtomicBool::new(false);
    let mut app = App::new();
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_event::<Chat>();
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        let world = world();
        let mut stream = world.event_reader::<Chat>().into_stream();
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
        if DONE.swap(true, Ordering::Relaxed){
            world.quit().await;
        }
        Ok(())
    });
    app.spawn_task(async {
        let world = world();
        let mut stream = world.event_reader::<Chat>().into_mapped_stream(|c| c.0.to_ascii_uppercase());
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
        if DONE.swap(true, Ordering::Relaxed){
            world.quit().await;
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