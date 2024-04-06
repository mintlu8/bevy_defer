use std::{any::{Any, TypeId}, borrow::Borrow, convert::Infallible, marker::PhantomData};

use bevy_ecs::{change_detection::DetectChanges, schedule::{State, States}, system::{Local, Res, Resource}};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;

use crate::{signals::{Signal, SignalId}, tls_resource};

/// Signal sending changed value of a [`States`].
#[derive(Debug, Clone, Copy)]
pub struct StateSignal<T: States + Clone + Default>(PhantomData<T>, Infallible);

impl<T: States + Clone + Default> SignalId for StateSignal<T> {
    type Data = T;
}

/// Named synchronization primitives.
#[derive(Resource, Default)]
pub struct Reactors {
    typed: Mutex<FxHashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    named: Mutex<FxHashMap<(String, TypeId), Box<dyn Any + Send + Sync>>>,
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
        if let Some(data) = lock.get(&(name.borrow(), TypeId::of::<T>()) as &dyn KeyPair){
            data.downcast_ref::<Signal<T::Data>>().expect("Unexpected signal type.").clone()
        } else {
            let signal = Signal::<T::Data>::default();
            lock.insert((name.into(), TypeId::of::<T>()), Box::new(signal.clone()));
            signal
        }
    }
}

/// React to a [`States`] changing, signals can be subscribed from [`Reactors`] with [`StateSignal`].
pub fn react_to_state<T: States + Clone + Default>(
    mut signal: Local<Option<Signal<T>>>,
    reactors: Res<Reactors>,
    state: Res<State<T>>
) {
    if !state.is_changed() { return; }
    let value = state.get().clone();
    if let Some(signal) = signal.as_ref() {
        signal.send_if_changed(value)
    } else {
        let sig = reactors.get_typed::<StateSignal<T>>();
        sig.send_if_changed(value);
        *signal = Some(sig)
    };
}

trait KeyPair {
    fn str(&self) -> &str;
    fn id(&self) -> &TypeId;
}


impl<'a> Borrow<dyn KeyPair + 'a> for (String, TypeId){
    fn borrow(&self) -> &(dyn KeyPair + 'a) {
        self
    }
}

impl std::hash::Hash for dyn KeyPair + '_ {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.str().hash(state);
        self.id().hash(state);
    }
}

impl PartialEq for dyn KeyPair + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.str() == other.str() && self.id() == other.id()
    }
}

impl Eq for dyn KeyPair + '_ {}

impl KeyPair for (String, TypeId) {
    fn str(&self) -> &str {
        &self.0
    }

    fn id(&self) -> &TypeId {
        &self.1
    }
}
impl KeyPair for (&str, TypeId) {
    fn str(&self) -> &str {
        self.0
    }

    fn id(&self) -> &TypeId {
        &self.1
    }
}