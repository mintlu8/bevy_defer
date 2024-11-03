use super::SignalId;
use async_shared::Value;
use bevy::ecs::component::Component;
use bevy::reflect::Reflect;
use rustc_hash::FxHashMap;
use std::{fmt::Debug, sync::Arc};
use ty_map_gen::type_map;

type_map! {
    /// A type map of signals.
    #[derive(Clone)]
    pub SignalMap where T [SignalId] => Arc<Value<T::Data>> [Clone + Send + Sync] as FxHashMap
}

/// A composable component that contains type-erased signals on an `Entity`.
#[derive(Component, Default, Reflect)]
pub struct Signals {
    #[reflect(ignore)]
    pub senders: SignalMap,
    #[reflect(ignore)]
    pub receivers: SignalMap,
}

impl Debug for Signals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signals")
            .field("senders", &self.senders.len())
            .field("receivers", &self.receivers.len())
            .finish()
    }
}

impl Signals {
    pub fn new() -> Self {
        Self {
            senders: SignalMap::new(),
            receivers: SignalMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.senders.is_empty() && self.receivers.is_empty()
    }

    pub fn from_sender<T: SignalId>(signal: Arc<Value<T::Data>>) -> Self {
        let mut this = Self::new();
        this.add_sender::<T>(signal);
        this
    }

    pub fn from_receiver<T: SignalId>(signal: Arc<Value<T::Data>>) -> Self {
        let mut this = Self::new();
        this.add_receiver::<T>(signal);
        this
    }

    pub fn with_sender<T: SignalId>(mut self, signal: Arc<Value<T::Data>>) -> Self {
        self.add_sender::<T>(signal);
        self
    }

    pub fn with_receiver<T: SignalId>(mut self, signal: Arc<Value<T::Data>>) -> Self {
        self.add_receiver::<T>(signal);
        self
    }

    /// Send a signal, can be polled through the sender.
    ///
    /// Returns `true` if the signal exists.
    pub fn send<T: SignalId>(&self, item: T::Data) -> bool {
        if let Some(sig) = self.senders.get::<T>() {
            sig.write(item);
            true
        } else {
            false
        }
    }

    /// Send a signal, can be polled through the sender.
    ///
    /// Returns `true` if the signal exists.
    pub fn send_if_changed<T: SignalId>(&self, item: T::Data) -> bool
    where
        T::Data: PartialEq,
    {
        if let Some(sig) = self.senders.get::<T>() {
            sig.write_if_changed(item);
            true
        } else {
            false
        }
    }

    /// Send a signal, cannot be polled through the sender.
    ///
    /// Returns `true` if the signal exists.
    pub fn broadcast<T: SignalId>(&self, item: T::Data) -> bool {
        if let Some(sig) = self.senders.get::<T>() {
            sig.write_and_tick(item);
            true
        } else {
            false
        }
    }

    /// Poll a signal from a receiver or an adaptor.
    pub fn poll_once<T: SignalId>(&self) -> Option<T::Data> {
        self.receivers.get::<T>().and_then(|x| x.read())
    }

    /// Poll a signal from a sender.
    pub fn poll_sender_once<T: SignalId>(&self) -> Option<T::Data> {
        self.senders.get::<T>().and_then(|x| x.read())
    }

    /// Borrow a sender's inner, this shares read tick compared to `clone`.
    pub fn borrow_sender<T: SignalId>(&self) -> Option<Arc<Value<T::Data>>> {
        self.senders.get::<T>().cloned()
    }

    /// Borrow a receiver's inner, this shares read tick compared to `clone`.
    pub fn borrow_receiver<T: SignalId>(&self) -> Option<Arc<Value<T::Data>>> {
        self.receivers.get::<T>().cloned()
    }

    /// Borrow a sender's inner, this shares read tick compared to `clone`.
    #[allow(clippy::box_default)]
    pub fn init_sender<T: SignalId>(&mut self) -> Arc<Value<T::Data>> {
        match self.borrow_sender::<T>() {
            Some(borrow) => borrow,
            None => {
                let signal = Arc::new(Value::<T::Data>::new());
                self.senders.insert::<T>(signal.clone());
                signal
            }
        }
    }

    /// Borrow a receiver's inner, this shares read tick compared to `clone`.
    #[allow(clippy::box_default)]
    pub fn init_receiver<T: SignalId>(&mut self) -> Arc<Value<T::Data>> {
        match self.borrow_receiver::<T>() {
            Some(borrow) => borrow,
            None => {
                let signal = Arc::new(Value::<T::Data>::new());
                self.senders.insert::<T>(signal.clone());
                signal
            }
        }
    }

    pub fn add_sender<T: SignalId>(&mut self, signal: Arc<Value<T::Data>>) {
        self.senders.insert::<T>(signal);
    }

    pub fn add_receiver<T: SignalId>(&mut self, signal: Arc<Value<T::Data>>) {
        self.receivers.insert::<T>(signal);
    }

    pub fn remove_sender<T: SignalId>(&mut self) {
        self.senders.remove::<T>();
    }

    pub fn remove_receiver<T: SignalId>(&mut self) {
        self.receivers.remove::<T>();
    }

    pub fn has_sender<T: SignalId>(&self) -> bool {
        self.senders.contains::<T>()
    }
    pub fn has_receiver<T: SignalId>(&self) -> bool {
        self.receivers.contains::<T>()
    }

    pub fn extend(mut self, other: Signals) -> Signals {
        self.senders.extend(other.senders);
        self.receivers.extend(other.receivers);
        self
    }

    pub fn into_signals(self) -> Signals {
        self
    }
}
