//! Signals for `bevy_defer`.

mod signal_utils;
mod signal_inner;
mod signal_component;
mod named;

pub use signal_utils::*;
pub use signal_inner::{Signal, SignalInner};
pub use signal_component::Signals;
pub use named::NamedSignals;
pub(crate) use named::NAMED_SIGNALS;