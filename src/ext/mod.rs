//! Extensions to various components.

mod anim;
mod scene;

pub use scene::AsyncScene;
pub use anim::{AsyncAnimationPlayer, AnimationChange, react_to_animation};