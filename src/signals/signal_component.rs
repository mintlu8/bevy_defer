use super::signal_inner::SignalBorrow;
use super::{Signal, SignalId};
use bevy_ecs::component::Component;
use bevy_reflect::Reflect;
use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};

/// A composable component that contains type-erased signals on an `Entity`.
#[derive(Debug, Component, Default, Reflect)]
pub struct Signals {
    #[reflect(ignore)]
    pub senders: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
    #[reflect(ignore)]
    pub receivers: FxHashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Signals {
    pub fn new() -> Self {
        Self {
            senders: FxHashMap::default(),
            receivers: FxHashMap::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.senders.is_empty() && self.receivers.is_empty()
    }

    pub fn from_sender<T: SignalId>(signal: Signal<T::Data>) -> Self {
        let mut this = Self::new();
        this.add_sender::<T>(signal);
        this
    }

    pub fn from_receiver<T: SignalId>(signal: Signal<T::Data>) -> Self {
        let mut this = Self::new();
        this.add_receiver::<T>(signal);
        this
    }

    pub fn with_sender<T: SignalId>(mut self, signal: Signal<T::Data>) -> Self {
        self.add_sender::<T>(signal);
        self
    }

    pub fn with_receiver<T: SignalId>(mut self, signal: Signal<T::Data>) -> Self {
        self.add_receiver::<T>(signal);
        self
    }

    /// Send a signal, can be polled through the sender.
    ///
    /// Returns `true` if the signal exists.
    pub fn send<T: SignalId>(&self, item: T::Data) -> bool {
        if let Some(sig) = self
            .senders
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<Signal<T::Data>>())
        {
            sig.send(item);
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
        if let Some(sig) = self
            .senders
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<Signal<T::Data>>())
        {
            sig.send_if_changed(item);
            true
        } else {
            false
        }
    }

    /// Send a signal, cannot be polled through the sender.
    ///
    /// Returns `true` if the signal exists.
    pub fn broadcast<T: SignalId>(&self, item: T::Data) -> bool {
        if let Some(sig) = self
            .senders
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<Signal<T::Data>>())
        {
            sig.broadcast(item);
            true
        } else {
            false
        }
    }

    /// Poll a signal from a receiver or an adaptor.
    pub fn poll_once<T: SignalId>(&self) -> Option<T::Data> {
        self.receivers
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<Signal<T::Data>>())
            .and_then(|x| x.try_read())
    }

    /// Poll a signal from a sender.
    pub fn poll_sender_once<T: SignalId>(&self) -> Option<T::Data> {
        self.senders
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<Signal<T::Data>>())
            .and_then(|x| x.try_read())
    }

    /// Borrow a sender's inner, this shares read tick compared to `clone`.
    pub fn borrow_sender<T: SignalId>(&self) -> Option<SignalBorrow<T::Data>> {
        self.senders
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<Signal<T::Data>>())
            .map(|x| x.borrow_inner())
    }

    /// Borrow a receiver's inner, this shares read tick compared to `clone`.
    pub fn borrow_receiver<T: SignalId>(&self) -> Option<SignalBorrow<T::Data>> {
        self.receivers
            .get(&TypeId::of::<T>())
            .and_then(|x| x.downcast_ref::<Signal<T::Data>>())
            .map(|x| x.borrow_inner())
    }

    /// Borrow a sender's inner, this shares read tick compared to `clone`.
    #[allow(clippy::box_default)]
    pub fn init_sender<T: SignalId>(&mut self) -> SignalBorrow<T::Data> {
        self.senders
            .entry(TypeId::of::<T>())
            .or_insert(Box::new(Signal::<T::Data>::default()))
            .downcast_ref::<Signal<T::Data>>()
            .unwrap()
            .borrow_inner()
    }

    /// Borrow a receiver's inner, this shares read tick compared to `clone`.
    #[allow(clippy::box_default)]
    pub fn init_receiver<T: SignalId>(&mut self) -> SignalBorrow<T::Data> {
        self.receivers
            .entry(TypeId::of::<T>())
            .or_insert(Box::new(Signal::<T::Data>::default()))
            .downcast_ref::<Signal<T::Data>>()
            .unwrap()
            .borrow_inner()
    }

    pub fn add_sender<T: SignalId>(&mut self, signal: Signal<T::Data>) {
        self.senders
            .insert(TypeId::of::<T>(), Box::new(signal.clone()));
    }
    pub fn add_receiver<T: SignalId>(&mut self, signal: Signal<T::Data>) {
        self.receivers
            .insert(TypeId::of::<T>(), Box::new(signal.clone()));
    }

    pub fn remove_sender<T: SignalId>(&mut self) {
        self.senders.remove(&TypeId::of::<T>());
    }
    pub fn remove_receiver<T: SignalId>(&mut self) {
        self.receivers.remove(&TypeId::of::<T>());
    }

    pub fn has_sender<T: SignalId>(&self) -> bool {
        self.senders.contains_key(&TypeId::of::<T>())
    }
    pub fn has_receiver<T: SignalId>(&self) -> bool {
        self.receivers.contains_key(&TypeId::of::<T>())
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
