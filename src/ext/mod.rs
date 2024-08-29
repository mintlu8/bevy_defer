//! Extensions to various components.

#[cfg(feature = "bevy_animation")]
pub mod anim;
pub mod picking;
#[cfg(feature = "bevy_scene")]
pub mod scene;
#[cfg(feature = "bevy_sprite")]
pub mod sprite;
