//! Extensions to various components.

#[cfg(feature = "bevy_animation")]
pub mod anim;
#[cfg(feature = "bevy_scene")]
pub mod scene;
pub mod transform;
#[cfg(feature = "bevy_world_serialization")]
pub mod world;
