//! Signals for `bevy_defer`.

mod signal_utils;
mod signal_inner;
mod signal_component;

use std::{borrow::Borrow, marker::PhantomData};
use std::sync::Arc;
use bevy_ecs::system::Resource;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
pub use signal_utils::*;
pub use signal_inner::{Signal, SignalData};
pub use signal_component::Signals;
pub(crate) use signal_inner::SignalInner;

/// A resource containing named signals.
#[derive(Resource)]
pub struct NamedSignals<T: SignalId>{
    map: Mutex<FxHashMap<String, Arc<SignalData<T::Data>>>>,
    p: PhantomData<T>
}

impl<T: SignalId> std::fmt::Debug for NamedSignals<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamedSignals").field("map", &self.map.lock().len()).finish()
    }
}

impl<T: SignalId> Default for NamedSignals<T> {
    fn default() -> Self {
        Self { map: Default::default(), p: Default::default() }
    }
}

impl<T: SignalId> NamedSignals<T> {
    /// Obtain a named signal.
    pub fn get(&mut self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>>{
        if let Some(data) = self.map.get_mut().get(name.borrow()){
            data.clone()
        } else {
            let data = Arc::new(SignalData::default());
            self.map.get_mut().insert(name.into(), data.clone());
            data
        }
    }

    /// Obtain a named signal through locking.
    pub fn get_from_ref(&self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>>{
        let mut map = self.map.lock();
        if let Some(data) = map.get(name.borrow()){
            data.clone()
        } else {
            let data = Arc::new(SignalData::default());
            map.insert(name.into(), data.clone());
            data
        }
    }
}
