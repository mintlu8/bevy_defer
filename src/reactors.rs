//! Signals and synchronization primitives for reacting to standard bevy events.
use async_shared::Value;
use bevy_ecs::{
    change_detection::DetectChanges,
    component::Component,
    entity::Entity,
    event::Event,
    query::{Changed, With},
    system::{Local, Query, Res, Resource},
};
use bevy_state::state::{State, States};
use rustc_hash::FxHashMap;
use std::{
    cell::OnceCell,
    convert::Infallible,
    marker::PhantomData,
    sync::{Arc, Mutex},
};
use ty_map_gen::type_map;

use crate::signals::{Receiver, SignalId, SignalSender, Signals};
use crate::{access::async_event::EventBuffer, signals::WriteValue};

/// Signal that sends changed values of a [`States`].
#[derive(Debug, Clone, Copy)]
pub struct StateSignal<T: States + Clone>(PhantomData<T>, Infallible);

impl<T: States + Clone> SignalId for StateSignal<T> {
    type Data = T;
}

/// Named or typed synchronization primitives of `bevy_defer`.
#[derive(Resource, Default, Clone)]
pub struct Reactors(Arc<ReactorsInner>);

type_map!(
    /// A type map of signals.
    pub SignalMap where T [SignalId] => Value<T::Data> [Send + Sync] as FxHashMap
);

type_map!(
    /// A type map of signals.
    pub NamedSignalMap where (T, String) [SignalId] => Value<T::Data> [Send + Sync] as FxHashMap
);

type_map!(
    /// A type map of signals.
    #[derive(Clone)]
    pub EventBufferMap where E [Event] => Arc<EventBuffer<E>> [Clone + Send + Sync] as FxHashMap
);

/// Named or typed synchronization primitives of `bevy_defer`.
#[derive(Default)]
pub(crate) struct ReactorsInner {
    typed: Mutex<SignalMap>,
    named: Mutex<NamedSignalMap>,
    event_buffers: Mutex<EventBufferMap>,
}

impl std::fmt::Debug for ReactorsInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reactors")
            .field("typed", &self.typed.lock().unwrap().len())
            .field("named", &self.named.lock().unwrap().len())
            .finish()
    }
}

impl Reactors {
    /// Obtain a typed signal.
    #[allow(clippy::box_default)]
    pub fn get_typed<T: SignalId>(&self) -> WriteValue<T::Data> {
        let mut lock = self.0.typed.lock().unwrap();
        if let Some(data) = lock.get::<T>() {
            WriteValue(data.clone_uninit())
        } else {
            let signal = Value::<T::Data>::default();
            lock.insert::<T>(signal.clone_raw());
            WriteValue(signal)
        }
    }

    /// Obtain a named signal.
    pub fn get_named<T: SignalId>(&self, name: &str) -> WriteValue<T::Data> {
        let mut lock = self.0.named.lock().unwrap();
        if let Some(data) = lock.get::<T, _>(name) {
            WriteValue(data.clone_uninit())
        } else {
            let signal = Value::<T::Data>::default();
            lock.insert::<T>(name.to_owned(), signal.clone_raw());
            WriteValue(signal)
        }
    }

    /// Obtain an event buffer by event type.
    pub fn get_event<E: Event + Clone>(&self) -> Arc<EventBuffer<E>> {
        let mut lock = self.0.event_buffers.lock().unwrap();
        if let Some(data) = lock.get::<E>() {
            data.clone()
        } else {
            let signal = <Arc<EventBuffer<E>>>::default();
            lock.insert::<E>(signal.clone());
            signal
        }
    }
}

/// React to a [`States`] changing, signals can be subscribed from [`Reactors`] with [`StateSignal`].
pub fn react_to_state<T: States + Clone>(
    signal: Local<OnceCell<WriteValue<T>>>,
    reactors: Res<Reactors>,
    state: Res<State<T>>,
) {
    if !state.is_changed() {
        return;
    }
    let value = state.get().clone();
    let signal = signal.get_or_init(|| reactors.get_typed::<StateSignal<T>>());
    signal.write_if_changed(value);
}

/// [`SignalId`] and data for a change in a component state machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Change<T> {
    pub from: Option<T>,
    pub to: T,
}

impl<T: Send + Sync + 'static + Clone> SignalId for Change<T> {
    type Data = Change<T>;
}

/// React to a [`Component`] change, usually for a state machine like `bevy_ui::Interaction`.
/// Returns the current and previous value as a [`Change`] signal.
///
/// # Guarantee
///
/// `from` and `to` are not equal.
pub fn react_to_component_change<M: Component + Clone + PartialEq>(
    mut prev: Local<FxHashMap<Entity, M>>,
    query: Query<(Entity, &M, SignalSender<Change<M>>), (Changed<M>, With<Signals>)>,
) {
    for (entity, state, sender) in query.iter() {
        let previous = prev.get(&entity);
        if Some(state) == previous {
            continue;
        }
        sender.send(Change {
            from: previous.cloned(),
            to: state.clone(),
        });
        prev.insert(entity, state.clone());
    }
}

/// Alias for the [`Change<T>`] signal receiver, that reacts to a change in a component.
///
/// Requires corresponding [`react_to_component_change`] system.
pub type StateMachine<T> = Receiver<Change<T>>;
