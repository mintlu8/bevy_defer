//! Signals and synchronization primitives for reacting to standard bevy events.
use std::{any::{Any, TypeId}, borrow::Borrow, cell::OnceCell, convert::Infallible, marker::PhantomData, sync::Arc};
use bevy_ecs::{change_detection::DetectChanges, component::Component, entity::Entity, event::Event, query::{Changed, With}, schedule::{State, States}, system::{Local, Query, Res, Resource}};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;

use crate::{access::async_event::DoubleBufferedEvent, tls_resource};
use crate::signals::{Signal, SignalId, SignalSender, Signals, Receiver};

/// Signal sending changed value of a [`States`].
#[derive(Debug, Clone, Copy)]
pub struct StateSignal<T: States + Clone + Default>(PhantomData<T>, Infallible);

impl<T: States + Clone + Default> SignalId for StateSignal<T> {
    type Data = T;
}

/// Named or typed synchronization primitives of `bevy_defer`.
#[derive(Resource, Default)]
pub struct Reactors {
    typed: Mutex<FxHashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    named: Mutex<FxHashMap<(String, TypeId), Box<dyn Any + Send + Sync>>>,
    event_buffers: Mutex<FxHashMap<TypeId, Box<dyn Any + Send + Sync>>>
}

tls_resource!(pub(crate) REACTORS: Reactors);

impl std::fmt::Debug for Reactors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reactors")
            .field("typed", &self.typed.lock().len())
            .field("named", &self.named.lock().len())
            .finish()
    }
}

impl Reactors {
    /// Obtain a typed signal.
    #[allow(clippy::box_default)]
    pub fn get_typed<T: SignalId>(&self) -> Signal<T::Data> {
        self.typed.lock().entry(TypeId::of::<T>())
            .or_insert(Box::new(Signal::<T::Data>::default()))
            .downcast_ref::<Signal<T::Data>>()
            .expect("Unexpected signal type.")
            .clone()
    }

    /// Obtain a named signal.
    pub fn get_named<T: SignalId>(&self, name: impl Borrow<str> + Into<String>) -> Signal<T::Data> {
        let mut lock = self.named.lock();
        if let Some(data) = lock.get(&(name.borrow(), TypeId::of::<T>()) as &dyn NameAndType){
            data.downcast_ref::<Signal<T::Data>>().expect("Unexpected signal type.").clone()
        } else {
            let signal = Signal::<T::Data>::default();
            lock.insert((name.into(), TypeId::of::<T>()), Box::new(signal.clone()));
            signal
        }
    }

    /// Obtain an event buffer by event type.
    pub fn get_event<E: Event + Clone>(&self) -> Arc<DoubleBufferedEvent<E>> {
        let mut lock = self.event_buffers.lock();
        if let Some(data) = lock.get(&TypeId::of::<E>()){
            data.downcast_ref::<Arc<DoubleBufferedEvent<E>>>().expect("Unexpected event buffer type.").clone()
        } else {
            let signal = <Arc<DoubleBufferedEvent<E>>>::default();
            lock.insert(TypeId::of::<E>(), Box::new(signal.clone()));
            signal
        }
    }
}

/// React to a [`States`] changing, signals can be subscribed from [`Reactors`] with [`StateSignal`].
pub fn react_to_state<T: States + Clone + Default>(
    signal: Local<OnceCell<Signal<T>>>,
    reactors: Res<Reactors>,
    state: Res<State<T>>
) {
    if !state.is_changed() { return; }
    let value = state.get().clone();
    let signal = signal.get_or_init(||reactors.get_typed::<StateSignal<T>>());
    signal.send_if_changed(value)
}

/// [`SignalId`] and data for a change in a component state machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Change<T> {
    pub from: T,
    pub to: T
}

impl<T: Send + Sync + 'static + Clone + Default> SignalId for Change<T> {
    type Data = Change<T>;
}

/// React to a [`Component`] change, usually for a state machine like `bevy_ui::Interaction`.
/// Returns the current and previous value as a [`Change`] signal.
/// 
/// # Guarantee
/// 
/// `from` and `to` are not equal.
pub fn react_to_component_change<M: Component + Clone + Default + PartialEq>(
    mut prev: Local<FxHashMap<Entity, M>>,
    query: Query<(Entity, &M, SignalSender<Change<M>>), (Changed<M>, With<Signals>)>
) {
    for (entity, state, sender) in query.iter() {
        let previous = prev.get(&entity).cloned().unwrap_or_default();
        if state == &previous { continue }
        sender.send(Change {
            from: previous,
            to: state.clone(),
        });
        prev.insert(entity, state.clone());
    }
}

/// Alias for the [`Change<T>`] signal receiver, that reacts to a change in a component.
/// 
/// Requires corresponding [`react_to_component_change`] system.
pub type StateMachine<T> = Receiver<Change<T>>;

trait NameAndType {
    fn str(&self) -> &str;
    fn id(&self) -> &TypeId;
}


impl<'a> Borrow<dyn NameAndType + 'a> for (String, TypeId){
    fn borrow(&self) -> &(dyn NameAndType + 'a) {
        self
    }
}

impl std::hash::Hash for dyn NameAndType + '_ {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.str().hash(state);
        self.id().hash(state);
    }
}

impl PartialEq for dyn NameAndType + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.str() == other.str() && self.id() == other.id()
    }
}

impl Eq for dyn NameAndType + '_ {}

impl NameAndType for (String, TypeId) {
    fn str(&self) -> &str {
        &self.0
    }

    fn id(&self) -> &TypeId {
        &self.1
    }
}
impl NameAndType for (&str, TypeId) {
    fn str(&self) -> &str {
        self.0
    }

    fn id(&self) -> &TypeId {
        &self.1
    }
}