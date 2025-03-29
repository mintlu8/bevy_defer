//! Tweening support for `bevy_defer`.
use std::{
    ops::{Add, Mul},
    time::Duration,
};

use bevy::math::StableInterpolate;
use ref_cast::RefCast;

/// Looping information for tweening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    #[default]
    Once,
    Loop,
    Bounce,
}

/// [`f32`] or [`Duration`].
pub trait AsSeconds {
    /// Convert to seconds in [`f32`].
    fn as_secs(&self) -> f32;

    /// Convert to [`Duration`].
    fn as_duration(&self) -> Duration;
}

impl AsSeconds for u64 {
    fn as_secs(&self) -> f32 {
        *self as f32
    }

    fn as_duration(&self) -> Duration {
        Duration::from_secs(*self)
    }
}

impl AsSeconds for f32 {
    fn as_secs(&self) -> f32 {
        *self
    }

    fn as_duration(&self) -> Duration {
        Duration::from_secs_f32(*self)
    }
}

impl AsSeconds for Duration {
    fn as_secs(&self) -> f32 {
        self.as_secs_f32()
    }

    fn as_duration(&self) -> Duration {
        *self
    }
}

/// Make a type that implements [`Add`] and [`Mul<f32>`] implement [`StableInterpolate`].
#[derive(Debug, Clone, Copy, Default, RefCast)]
#[repr(transparent)]
pub struct MakeLerp<T>(pub T);

impl<T> MakeLerp<T> {
    /// Cast a mutable reference to [`MakeLerp`].
    pub fn make(this: &mut T) -> &mut Self {
        MakeLerp::ref_cast_mut(this)
    }
}

impl<T> StableInterpolate for MakeLerp<T>
where
    T: Clone + Add<T, Output = T> + Mul<f32, Output = T>,
{
    fn interpolate_stable(&self, other: &Self, t: f32) -> Self {
        let u = 1.0 - t;
        Self(self.0.clone() * u + other.0.clone() * t)
    }
}
