//! Extensions to various components.

#[cfg(feature="bevy_animation")]
mod anim;
mod scene;

pub use scene::AsyncScene;

#[cfg(feature="bevy_animation")]
pub use anim::{AsyncAnimationPlayer, AnimationChange, react_to_animation};