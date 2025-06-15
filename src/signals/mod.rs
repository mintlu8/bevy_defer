//! Signals for `bevy_defer`.
//!
//! Signals are the cornerstone of reactive programming in `bevy_defer`
//! that bridges the sync and async world.
//! The `Signals` component can be added to an entity,
//! and the `NamedSignals` resource can be used to
//! provide matching signals when needed.
//!
//! The implementation is similar to tokio's `Watch` channel and here are the guarantees:
//!
//! * A `Signal` can hold only one value.
//! * A `Signal` is read at most once per write for every reader.
//! * Values are not guaranteed to be read if updated in rapid succession.
//! * Value prior to reader creation will not be read by a new reader.
//!
//! `Signals` erases the underlying types and utilizes the `SignalId` trait to disambiguate signals,
//! this ensures no archetype fragmentation.
//!
//! In systems, you can use `SignalSender` and `SignalReceiver` just like you would in async,
//! you can build "reactors" this way by sending message to the async world through signals.
//! A common pattern is `react_to_component_change`, where you build a state machine like
//! `bevy_ui::Interaction` in bevy code, add `react_to_component_change` as a system,
//! then listen to the signal `Change<T>` as a `Stream` in async.
//!
//! `SignalSender` and `SignalReceiver` do not filter archetypes,
//! if you only care about sending signals, make sure to add `With<Signals>` for better performance.
//!
mod signal_component;
mod signal_utils;

use std::{any::type_name, sync::Arc};

pub use async_shared::Value;
use bevy::ecs::world::World;
pub use signal_component::{SignalMap, Signals};
pub use signal_utils::*;

use crate::{access::AsyncEntityMut, executor::with_world_mut, AccessError, AccessResult};

impl AsyncEntityMut {
    /// Send data through a signal on this entity.
    ///
    /// Returns `true` if the signal exists.
    pub fn send_signal<S: SignalId>(&self, data: S::Data) -> AccessResult<bool> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound(entity));
            };
            let Some(signals) = entity.get_mut::<Signals>() else {
                return Err(AccessError::ComponentNotFound {
                    name: type_name::<S>(),
                });
            };
            Ok(signals.send::<S>(data))
        })
    }

    /// Init or borrow a sender from an entity with shared read tick.
    pub fn signal_sender<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound(entity));
            };
            let mut signals = match entity.get_mut::<Signals>() {
                Some(sender) => sender,
                None => entity.insert(Signals::new()).get_mut::<Signals>().unwrap(),
            };
            Ok(signals.init_sender::<S>())
        })
    }

    /// Init or borrow a receiver from an entity with shared read tick.
    pub fn signal_receiver<S: SignalId>(&self) -> AccessResult<Arc<Value<S::Data>>> {
        let entity = self.0;
        with_world_mut(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else {
                return Err(AccessError::EntityNotFound(entity));
            };
            let mut signals = match entity.get_mut::<Signals>() {
                Some(sender) => sender,
                None => entity.insert(Signals::new()).get_mut::<Signals>().unwrap(),
            };
            Ok(signals.init_receiver::<S>())
        })
    }
}
