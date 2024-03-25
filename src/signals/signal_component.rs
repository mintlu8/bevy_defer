use std::any::TypeId;
use bevy_ecs::component::Component;
use bevy_log::debug;
use bevy_reflect::Reflect;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use crate::Object;
use super::{signal_inner::SignalInner, Signal, SignalId, SignalMapper, TypedSignal};


/// A composable component that contains signals on an `Entity`.
#[derive(Debug, Component, Default, Reflect)]
pub struct Signals {
    #[reflect(ignore)]
    pub senders: FxHashMap<TypeId, Signal<Object>>,
    #[reflect(ignore)]
    pub receivers: FxHashMap<TypeId, Signal<Object>>,
    #[reflect(ignore)]
    pub adaptors: FxHashMap<TypeId, (TypeId, SignalMapper)>
}

impl Signals {
    pub fn new() -> Self {
        Self { 
            senders: FxHashMap::default(), 
            receivers: FxHashMap::default(), 
            adaptors: FxHashMap::default(), 
        }
    }

    pub fn is_empty(&self) -> bool {
        self.senders.is_empty() && self.receivers.is_empty()
    }

    pub fn from_sender<T: SignalId>(signal: TypedSignal<T::Data>) -> Self {
        let mut this = Self::new();
        this.add_sender::<T>(signal);
        this
    }

    pub fn from_receiver<T: SignalId>(signal: TypedSignal<T::Data>) -> Self {
        let mut this = Self::new();
        this.add_receiver::<T>(signal);
        this
    }

    pub fn from_adaptor<T: SignalId>(ty: TypeId, mapper: SignalMapper) -> Self {
        let mut this = Self::new();
        this.add_adaptor::<T>(ty, mapper);
        this
    }


    pub fn with_sender<T: SignalId>(mut self, signal: TypedSignal<T::Data>) -> Self {
        self.add_sender::<T>(signal);
        self
    }

    pub fn with_receiver<T: SignalId>(mut self, signal: TypedSignal<T::Data>) -> Self {
        self.add_receiver::<T>(signal);
        self
    }

    pub fn with_adaptor<T: SignalId>(mut self, ty: TypeId, mapper: SignalMapper) -> Self {
        self.add_adaptor::<T>(ty, mapper);
        self
    }

    /// Send a signal, can be polled through the sender.
    pub fn send<T: SignalId>(&self, item: T::Data) {
        if let Some(x) = self.senders.get(&TypeId::of::<T>()) {
            debug!("Signal {} sent with value {:?}", std::any::type_name::<T>(), &item);
            x.write(Object::new(item))
        }
    }

    /// Send a signal, cannot be polled through the sender.
    pub fn broadcast<T: SignalId>(&self, item: T::Data) {
        if let Some(x) = self.senders.get(&TypeId::of::<T>()) {
            debug!("Signal {} sent value {:?}", std::any::type_name::<T>(), &item);
            x.broadcast(Object::new(item))
        }
    }

    /// Poll a signal from a receiver or an adaptor.
    pub fn poll_once<T: SignalId>(&self) -> Option<T::Data>{
        if let Some(sig) = self.receivers.get(&TypeId::of::<T>()) {
            sig.try_read().and_then(|x| x.get()).map(|x| {
            debug!("Signal {} received value {:?}", std::any::type_name::<T>(), &x);
            x
        })} else {
            match &self.adaptors.get(&TypeId::of::<T>()) {
                Some((ty, map)) => match self.receivers.get(ty){
                    Some(sig) => sig.try_read().and_then(|x| {
                        map.map(x).map(|x| {
                            debug!("Signal {} received and adapted value {:?}", std::any::type_name::<T>(), &x);
                            x
                        })
                    }),
                    None => None
                }
                None => None
            }
        }
    }

    /// Poll a signal from a sender.
    pub fn poll_sender_once<T: SignalId>(&self) -> Option<T::Data>{
        match self.senders.get(&TypeId::of::<T>()){
            Some(sig) => sig.try_read().and_then(|x| x.get()).map(|x| {
                debug!("Signal sender {} received value {:?}", std::any::type_name::<T>(), &x);
                x
            }),
            None => None,
        }
    }
    
    /// Borrow a sender's inner, this shares version number compared to `clone`.
    pub fn borrow_sender<T: SignalId>(&self) -> Option<Arc<SignalInner<Object>>> {
        self.senders.get(&TypeId::of::<T>()).map(|x| x.borrow_inner())
    }

    /// Borrow a receiver's inner, this shares version number compared to `clone`.
    pub fn borrow_receiver<T: SignalId>(&self) ->  Option<Arc<SignalInner<Object>>> {
        self.receivers.get(&TypeId::of::<T>()).map(|x| x.borrow_inner())
    }
    pub fn add_sender<T: SignalId>(&mut self, signal: TypedSignal<T::Data>) {
        self.senders.insert(TypeId::of::<T>(), Signal::from(signal));
    }
    pub fn add_receiver<T: SignalId>(&mut self, signal: TypedSignal<T::Data>) {
        self.receivers.insert(TypeId::of::<T>(), Signal::from(signal));
    }
    pub fn add_adaptor<T: SignalId>(&mut self, ty: TypeId, mapper: SignalMapper) {
        self.adaptors.insert(TypeId::of::<T>(), (ty, mapper));
    }

    pub fn remove_sender<T: SignalId>(&mut self) {
        self.senders.remove(&TypeId::of::<T>());
    }
    pub fn remove_receiver<T: SignalId>(&mut self) {
        self.receivers.remove(&TypeId::of::<T>());
    }
    pub fn remove_adaptor<T: SignalId>(&mut self) {
        self.adaptors.remove(&TypeId::of::<T>());
    }

    pub fn has_sender<T: SignalId>(&self) -> bool {
        self.senders.contains_key(&TypeId::of::<T>())
    }
    pub fn has_receiver<T: SignalId>(&self) ->  bool {
        self.receivers.contains_key(&TypeId::of::<T>())
    }
}
