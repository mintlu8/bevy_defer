//! Reactors for cursor interactions.

use crate::signal_ids;
use bevy_math::Vec2;

#[cfg(feature = "bevy_ui")]
mod ui;
#[cfg(feature = "bevy_ui")]
pub use ui::*;

signal_ids! {
    /// [`Interaction`](bevy_ui::Interaction) from any to `Pressed`.
    ///
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui.
    pub Pressed: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Pressed` to `Hovered`.
    ///
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui.
    pub Clicked: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `None` to `Hovered|Pressed`.
    ///
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui.
    pub ObtainedFocus: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Hovered|Pressed` to `None`.
    ///
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui.
    pub LostFocus: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Pressed` to `None`.
    ///
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui.
    pub ClickCancelled: Vec2,
}
