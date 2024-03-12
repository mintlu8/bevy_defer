use std::sync::atomic::{AtomicBool, Ordering};

use bevy_app::{App, Startup, Update};
use bevy_ecs::{component::Component, query::With, system::{Commands, Local, Query}};
use bevy_defer::{async_system, signal_ids, AsyncSystems, DefaultAsyncPlugin, SigRecv, SignalSender, Signals, TypedSignal};

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
            async_system!(|sig: SigRecv<SigText>|{
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
