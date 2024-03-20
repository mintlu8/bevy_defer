use std::{cell::{Cell, OnceCell}, ops::{Add, Mul}, rc::Rc, time::Duration};
use bevy_ecs::{component::Component, world::World};
use bevy_time::{Fixed, Time};
use crate::channels::channel;
use ref_cast::RefCast;

use crate::{AsyncComponent, AsyncFailure, AsyncResult, AsyncWorldMut, CHANNEL_CLOSED};

#[derive(Debug, Clone)]
pub struct Cancellation(Rc<Cell<bool>>);

impl Cancellation {
    pub fn new(&self) -> Cancellation {
        Cancellation(Rc::new(Cell::new(false)))
    }

    pub fn cancel(&self) {
        self.0.set(true)
    }
}

pub struct FixedTask {
    task: Box<dyn FnMut(&mut World, Duration) -> bool>,
    cancel: Option<Cancellation>,
}


/// A non-send thread-local queue running on `FixedUpdate`.
#[derive(Default)]
pub struct FixedQueue{
    inner: Vec<FixedTask>
}

pub fn run_fixed_queue(
    world: &mut World
) {
    let Some(mut queue) = world.remove_non_send_resource::<FixedQueue>() else { return; };
    let delta_time = world.resource::<Time<Fixed>>().delta();
    queue.inner.retain_mut(|x| {
        if let Some(cell) = &x.cancel {
            if cell.0.get() {
                return false;
            }
        }
        !(x.task)(world, delta_time)
    });
    world.insert_non_send_resource(queue);
}

impl AsyncWorldMut {
    /// Run a repeatable routine on `FixedUpdate`, with access to delta time.
    pub async fn fixed_routine<T: Send + 'static>(
        &self, 
        mut f: impl FnMut(&mut World, Duration) -> Option<T> + Send + 'static
    ) -> T {
        let (sender, receiver) = channel();
        let mut sender = Some(sender);
        self.run(|w| {
            w.non_send_resource_mut::<FixedQueue>().inner.push(
                FixedTask {
                    task: Box::new(move |world, dt| {
                        if let Some(item) = f(world, dt) {
                            if let Some(sender) = sender.take() {
                                // We do not log errors here.
                                let _ = sender.send(item);
                            }
                            true
                        } else {
                            false
                        }
                    }),
                    cancel: None
                }
            )
        }).await;
        receiver.await.expect(CHANNEL_CLOSED)
    }

    /// Run a repeatable routine on `FixedUpdate`, with access to delta time.
    /// 
    /// Use cancellation to cancel the routine.
    pub async fn fixed_routine_with_cancellation<T: Send + 'static>(
        &self, 
        mut f: impl FnMut(&mut World, Duration) -> Option<T> + Send + 'static, 
        cancellation: Cancellation
    ) -> T {
        let (sender, receiver) = channel();
        let mut sender = Some(sender);
        self.run(|w| {
            w.non_send_resource_mut::<FixedQueue>().inner.push(
                FixedTask {
                    task: Box::new(move |world, dt| {
                        if let Some(item) = f(world, dt) {
                            if let Some(sender) = sender.take() {
                                // We do not log errors here.
                                let _ = sender.send(item);
                            }
                            true
                        } else {
                            false
                        }
                    }),
                    cancel: Some(cancellation)
                }
            )
        }).await;
        receiver.await.expect(CHANNEL_CLOSED)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Playback {
    #[default]
    Once,
    Loop,
    Bounce,
}

pub trait Lerp: Clone + Send + 'static {
    fn lerp(from: Self, to: Self, fac: f32) -> Self;
}

impl<T> Lerp for T where T: Add<T, Output = T> + Mul<f32, Output = T> + Clone + Send + 'static {
    fn lerp(from: Self, to: Self, fac: f32) -> Self {
        from * (1.0 - fac) + to * fac
    }
}

/// [`f32`] or [`Duration`].
pub trait AsSeconds {
    fn as_secs(&self) -> f32;
}

impl AsSeconds for f32 {
    fn as_secs(&self) -> f32 {
        *self
    }
}

impl AsSeconds for Duration {
    fn as_secs(&self) -> f32 {
        self.as_secs_f32()
    }
}

impl<T: Component> AsyncComponent<'_, T> {

    /// Interpolate to a new value from the previous value.
    pub async fn interpolate_to<V: Lerp>(
        &self, 
        mut get: impl FnMut(&T) -> V + Send + 'static,
        mut set: impl FnMut(&mut T, V) + Send + 'static,
        mut curve: impl FnMut(f32) -> f32 + Send + 'static,
        duration: impl AsSeconds,
        to: V,
    ) -> AsyncResult<()> {
        let world = AsyncWorldMut::ref_cast(self.executor.as_ref());
        let entity = self.entity;
        let mut t = 0.0;
        let duration = duration.as_secs();
        let source = OnceCell::new();
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
                return Some(Ok(()))
            } else {
                let fac = curve(t / duration);
                set(component.as_mut(), V::lerp(source, to.clone(), fac));
                None
            }
        }).await
    }

    /// Run an animation.
    pub async fn interpolate<V>(
        &self, 
        mut access: impl FnMut(&mut T, V) + Send + 'static,
        mut curve: impl FnMut(f32) -> f32 + Send + 'static,
        duration: impl AsSeconds,
        mut span: impl FnMut(f32) -> V + Send + 'static,
        playback: Playback,
        cancel: Cancellation,
    ) -> AsyncResult<()> {
        let world = AsyncWorldMut::ref_cast(self.executor.as_ref());
        let entity = self.entity;
        let duration = duration.as_secs();
        let mut t = 0.0;
        world.fixed_routine_with_cancellation(move |world, dt| {
            let Some(mut entity) = world.get_entity_mut(entity) else {
                return Some(Err(AsyncFailure::EntityNotFound));
            };
            let Some(mut component) = entity.get_mut::<T>() else {
                return Some(Err(AsyncFailure::ComponentNotFound));
            };
            t += dt.as_secs_f32() / duration;
            if t > 1.0 {
                match playback {
                    Playback::Once => {
                        access(component.as_mut(), span(1.0));
                        return Some(Ok(()))
                    },
                    Playback::Loop => {
                        t = t.fract();
                    },
                    Playback::Bounce => {
                        t = 1.0 - (1.0 - t % 2.0).abs();
                    }
                }
            } 
            access(component.as_mut(), span(curve(t)));
            None
        }, cancel).await
    }
}