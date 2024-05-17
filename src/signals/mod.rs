//! Signals for `bevy_defer`.

mod signal_component;
mod signal_inner;
mod signal_utils;

pub use signal_component::{SignalMap, Signals};
pub use signal_inner::{Signal, SignalBorrow, SignalFuture, SignalStream};
pub use signal_utils::*;
