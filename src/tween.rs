//! Tweening for `bevy_defer`.


use std::{cell::OnceCell, ops::{Add, Mul}, time::Duration};
use bevy_ecs::component::Component;
use bevy_math::Quat;
use futures::Future;
use crate::cancellation::TaskCancellation;
use ref_cast::RefCast;

use crate::{AsyncFailure, AsyncResult};
use crate::access::{AsyncComponent, AsyncWorldMut};

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

impl<T> Lerp for T where T: Add<T, Output = T> + Mul<f32, Output = T> + Clone + Send + 'static {
    fn lerp(from: Self, to: Self, fac: f32) -> Self {
        from * (1.0 - fac) + to * fac
    }
}

/// Performs a spherical linear interpolation on [`Quat`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
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

impl<T: Component> AsyncComponent<T> {

    /// Interpolate to a new value from the previous value.
    pub fn interpolate_to<V: Lerp>(
        &self, 
        to: V,
        mut get: impl FnMut(&T) -> V + Send + 'static,
        mut set: impl FnMut(&mut T, V) + Send + 'static,
        mut curve: impl FnMut(f32) -> f32 + Send + 'static,
        duration: impl AsSeconds,
        cancel: impl Into<TaskCancellation>,
    ) -> impl Future<Output = AsyncResult<()>> + 'static {
        let world = AsyncWorldMut::ref_cast(&self.queue).clone();
        let entity = self.entity;
        let mut t = 0.0;
        let duration = duration.as_secs();
        let source = OnceCell::new();
        let cancel = cancel.into();
        async move {
            world.fixed_routine(move |world, dt| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Some(Err(AsyncFailure::EntityNotFound));
                };
                let Some(mut component) = entity.get_mut::<T>() else {
                    return Some(Err(AsyncFailure::ComponentNotFound));
                };
                let source = source.get_or_init(||get(&component)).clone();
                t += dt.as_secs_f32();
                if t > duration {
                    set(component.as_mut(), to.clone());
                    Some(Ok(()))
                } else {
                    let fac = curve(t / duration);
                    set(component.as_mut(), V::lerp(source, to.clone(), fac));
                    None
                }
            }, cancel).await.unwrap_or(Ok(()))
        }
    }

    /// Run an animation, maybe repeatedly, that can be cancelled.
    /// 
    /// It is recommended to `spawn` the result instead of awaiting it directly
    /// if not [`Playback::Once`].
    /// 
    /// ```
    /// # /*
    /// spawn(interpolate(.., Playback::Loop, &cancel));
    /// cancel.cancel();
    /// # */
    /// ```
    pub fn interpolate<V>(
        &self, 
        mut span: impl FnMut(f32) -> V + 'static,
        mut write: impl FnMut(&mut T, V) + 'static,
        mut curve: impl FnMut(f32) -> f32 + 'static,
        duration: impl AsSeconds,
        playback: Playback,
        cancel: impl Into<TaskCancellation>,
    ) -> impl Future<Output = AsyncResult<()>> + 'static {
        let world = AsyncWorldMut::ref_cast(&self.queue).clone();
        let entity = self.entity;
        let duration = duration.as_secs();
        let mut t = 0.0;
        let cancel = cancel.into();
        async move {
            world.fixed_routine(move |world, dt| {
                let Some(mut entity) = world.get_entity_mut(entity) else {
                    return Some(Err(AsyncFailure::EntityNotFound));
                };
                let Some(mut component) = entity.get_mut::<T>() else {
                    return Some(Err(AsyncFailure::ComponentNotFound));
                };
                t += dt.as_secs_f32() / duration;
                let fac = if t > 1.0 {
                    match playback {
                        Playback::Once => {
                            write(component.as_mut(), span(1.0));
                            return Some(Ok(()))
                        },
                        Playback::Loop => {
                            t = t.fract();
                            t
                        },
                        Playback::Bounce => {
                            t %= 2.0;
                            1.0 - (1.0 - t % 2.0).abs()
                        }
                    } 
                } else {
                    t
                };
                write(component.as_mut(), span(curve(fac)));
                None
            }, cancel).await.unwrap_or(Ok(()))
        }
        
    }
}