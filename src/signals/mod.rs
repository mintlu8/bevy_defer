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

pub use async_shared::Value;
pub use signal_component::{SignalMap, Signals};
pub use signal_utils::*;

#[deprecated = "Use `async_shared::Value` instead."]
pub type Signal<T> = Value<T>;
