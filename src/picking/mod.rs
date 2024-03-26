//! Reactors for cursor interactions.

use crate::signal_ids;
use bevy_math::Vec2;

#[cfg(feature = "bevy_ui")]
mod ui;
#[cfg(feature = "bevy_ui")]
pub use ui::*;


#[cfg(feature = "bevy_mod_picking")]
mod mod_picking;
#[cfg(feature = "bevy_mod_picking")]
pub use mod_picking::*;

signal_ids! {
    /// [`Interaction`](bevy_ui::Interaction) from any to `Pressed`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui,
    /// [`PointerLocation`](bevy_mod_picking::pointer::PointerLocation) if sender is `bevy_mod_picking`.
    pub Pressed: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Pressed` to `Hovered`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui,
    /// [`PointerLocation`](bevy_mod_picking::pointer::PointerLocation) if sender is `bevy_mod_picking`.
    pub Click: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `None` to `Hovered|Pressed`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui,
    /// [`PointerLocation`](bevy_mod_picking::pointer::PointerLocation) if sender is `bevy_mod_picking`.
    pub ObtainFocus: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Hovered|Pressed` to `None`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui,
    /// [`PointerLocation`](bevy_mod_picking::pointer::PointerLocation) if sender is `bevy_mod_picking`.
    pub LoseFocus: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Pressed` to `None`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition) if sender is ui,
    /// [`PointerLocation`](bevy_mod_picking::pointer::PointerLocation) if sender is `bevy_mod_picking`.
    pub ClickCancelled: Vec2,
}
