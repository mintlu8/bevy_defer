
use crate::tls_resource;

use std::any::{Any, TypeId};
use std::borrow::Borrow;
use std::sync::Arc;
use bevy_ecs::system::Resource;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;

use super::{SignalData, SignalId};

/// A resource containing named signals.
#[derive(Resource, Default)]
pub struct NamedSignals{
    map: Mutex<FxHashMap<(String, TypeId), Box<dyn Any + Send + Sync>>>,
}

impl std::fmt::Debug for NamedSignals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamedSignals").field("map", &self.map.lock().len()).finish()
    }
}

impl NamedSignals {
    /// Obtain a named signal.
    /// 
    /// If you only have a non-mutable reference, see [`NamedSignals::get_from_ref`].
    pub fn get<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>>{
        if let Some(data) = self.map.get_mut().get(&(name.borrow(), TypeId::of::<T>()) as &dyn KeyPair){
            data.downcast_ref::<Arc<SignalData<T::Data>>>().expect("Signal Type Error").clone()
        } else {
            let data = Arc::new(SignalData::<T::Data>::default());
            self.map.get_mut().insert((name.into(), TypeId::of::<T>()), Box::new(data.clone()));
            data
        }
    }

    /// Obtain a named signal through locking.
    pub fn get_from_ref<T: SignalId>(&self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>>{
        let mut map = self.map.lock();
        if let Some(data) = map.get(&(name.borrow(), TypeId::of::<T>()) as &dyn KeyPair){
            data.downcast_ref::<Arc<SignalData<T::Data>>>().expect("Signal Type Error").clone()
        } else {
            let data = Arc::new(SignalData::<T::Data>::default());
            map.insert((name.into(), TypeId::of::<T>()), Box::new(data.clone()));
            data
        }
    }
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

// See explanation (3).
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

tls_resource!(pub NAMED_SIGNALS: NamedSignals);
