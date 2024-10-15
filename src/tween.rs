//! Tweening support for `bevy_defer`.
use bevy::math::Quat;
use bevy::transform::prelude::Transform;
use ref_cast::RefCast;
use std::{
    ops::{Add, Deref, DerefMut, Mul},
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
    /// Linear interpolate.
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

/// Performs a specialized linear interpolation on bevy types that does not
/// use multiply and add for linear interpolation.
///
/// # Type Supported
///
/// * `Quat`: Performs spherical interpolation `Quat::slerp`.
/// * `Transform`: Performs interpolation on all three fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, RefCast)]
#[repr(transparent)]
pub struct MakeLerp<T>(pub T);

impl<T> MakeLerp<T>
where
    Self: Lerp,
{
    pub fn lerp(from: T, to: T, fac: f32) -> T {
        Lerp::lerp(Self(from), Self(to), fac).0
    }
}

impl<T> Deref for MakeLerp<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for MakeLerp<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[deprecated(note = "Use MakeLerp<Quat> instead.")]
pub type SLerp = MakeLerp<Quat>;

impl Lerp for MakeLerp<Quat> {
    fn lerp(from: Self, to: Self, fac: f32) -> Self {
        MakeLerp(Quat::slerp(from.0, to.0, fac))
    }
}

impl Lerp for MakeLerp<Transform> {
    fn lerp(from: Self, to: Self, fac: f32) -> Self {
        MakeLerp(Transform {
            translation: Lerp::lerp(from.translation, to.translation, fac),
            rotation: Quat::slerp(from.rotation, to.rotation, fac),
            scale: Lerp::lerp(from.scale, to.scale, fac),
        })
    }
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
