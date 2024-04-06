//! Signals for `bevy_defer`.

mod signal_utils;
mod signal_inner;
mod signal_component;

pub use signal_utils::*;
pub use signal_inner::{Signal, SignalInner};
pub use signal_component::Signals;