//! Tweening support for `bevy_defer`.
use bevy_math::Quat;
use ref_cast::RefCast;
use std::{
    ops::{Add, Mul},
    time::Duration,
};

/// Looping information for tweening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    #[default]
    Once,
    Loop,
    Bounce,
}

/// Types that can be linearly interpolated.
pub trait Lerp: Clone + Send + 'static {
    fn lerp(from: Self, to: Self, fac: f32) -> Self;
}

impl<T> Lerp for T
where
    T: Add<T, Output = T> + Mul<f32, Output = T> + Clone + Send + 'static,
{
    fn lerp(from: Self, to: Self, fac: f32) -> Self {
        from * (1.0 - fac) + to * fac
    }
}

/// Performs a spherical linear interpolation on [`Quat`].
#[derive(Debug, Clone, Copy, Default, PartialEq, RefCast)]
#[repr(transparent)]
pub struct SLerp(pub Quat);

impl Lerp for SLerp {
    fn lerp(from: Self, to: Self, fac: f32) -> Self {
        SLerp(Quat::slerp(from.0, to.0, fac))
    }
}

/// [`f32`] or [`Duration`].
pub trait AsSeconds {
    fn as_secs(&self) -> f32;

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
