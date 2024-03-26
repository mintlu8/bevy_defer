//! Tweening for `bevy_defer`.


use std::{cell::{Cell, OnceCell}, ops::{Add, Mul}, rc::Rc, sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};
use bevy_ecs::{component::Component, world::World};
use bevy_math::Quat;
use bevy_time::{Fixed, Time};
use futures::Future;
use crate::channels::channel;
use ref_cast::RefCast;

use crate::{AsyncFailure, AsyncResult};
use crate::access::{AsyncComponent, AsyncWorldMut};

/// Shared object for cancelling a running task.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Rc<Cell<bool>>);

/// Shared object for cancelling a running task that is `send` and `sync`.
#[derive(Debug, Clone, Default)]
pub struct SyncCancellation(Arc<AtomicBool>);

/// Shared object for cancelling a running task on drop.
#[derive(Debug, Clone, Default)]
pub struct CancelOnDrop(pub Cancellation);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel()
    }
}

impl Cancellation {
    pub fn new() -> Cancellation {
        Cancellation(Rc::new(Cell::new(false)))
    }

    /// Cancel a running task.
    pub fn cancel(&self) {
        self.0.set(true)
    }

    pub fn cancel_on_drop(self) -> CancelOnDrop {
        CancelOnDrop(self)
    }
}

/// Shared object for cancelling a running task on drop that is `send` and `sync`.
#[derive(Debug, Clone, Default)]
pub struct CancelOnDropSync(pub SyncCancellation);

impl Drop for CancelOnDropSync {
    fn drop(&mut self) {
        self.0.cancel()
    }
}

impl SyncCancellation {
    pub fn new() -> SyncCancellation {
        SyncCancellation(Arc::new(AtomicBool::new(false)))
    }

    /// Cancel a running task.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed)
    }

    pub fn cancel_on_drop(self) -> CancelOnDropSync {
        CancelOnDropSync(self)
    }
}

/// Cancellation token for a running task.
#[derive(Debug, Clone)]
pub enum TaskCancellation {
    Unsync(Cancellation),
    Sync(SyncCancellation),
    None
}

impl TaskCancellation {
    pub fn cancelled(&self) -> bool {
        match self {
            TaskCancellation::Unsync(cell) => cell.0.get(),
            TaskCancellation::Sync(b) => b.0.load(Ordering::Relaxed),
            TaskCancellation::None => false,
        }
    }
}

impl From<()> for TaskCancellation {
    fn from(_: ()) -> Self {
        TaskCancellation::None
    }
}

impl From<Cancellation> for TaskCancellation {
    fn from(val: Cancellation) -> Self {
        TaskCancellation::Unsync(val)
    }
}

impl From<&Cancellation> for TaskCancellation {
    fn from(val: &Cancellation) -> Self {
        TaskCancellation::Unsync(val.clone())
    }
}

impl From<SyncCancellation> for TaskCancellation {
    fn from(val: SyncCancellation) -> Self {
        TaskCancellation::Sync(val)
    }
}

impl From<&SyncCancellation> for TaskCancellation {
    fn from(val: &SyncCancellation) -> Self {
        TaskCancellation::Sync(val.clone())
    }
}

impl From<&CancelOnDrop> for TaskCancellation {
    fn from(val: &CancelOnDrop) -> Self {
        TaskCancellation::Unsync(val.0.clone())
    }
}

impl From<&CancelOnDropSync> for TaskCancellation {
    fn from(val: &CancelOnDropSync) -> Self {
        TaskCancellation::Sync(val.0.clone())
    }
}


/// A Task running on `FixedUpdate`.
pub(crate) struct FixedTask {
    task: Box<dyn FnMut(&mut World, Duration) -> bool>,
    cancel: TaskCancellation,
}


/// A non-send thread-local queue running on `FixedUpdate`.
#[derive(Default)]
pub struct FixedQueue{
    inner: Vec<FixedTask>
}

/// Run [`FixedQueue`] on `FixedUpdate`.
pub fn run_fixed_queue(
    world: &mut World
) {
    let Some(mut queue) = world.remove_non_send_resource::<FixedQueue>() else { return; };
    let delta_time = world.resource::<Time<Fixed>>().delta();
    queue.inner.retain_mut(|x| {
        if x.cancel.cancelled() {
            return false;
        }
        !(x.task)(world, delta_time)
    });
    world.insert_non_send_resource(queue);
}

impl AsyncWorldMut {
    /// Run a repeatable routine on `FixedUpdate`, with access to delta time.
    /// 
    /// Use cancellation to cancel the routine.
    /// 
    /// `Into<TaskCancellation>` accepts `()`, 
    /// [`Cancellation`](Cancellation) or [`SyncCancellation`](SyncCancellation).
    pub fn fixed_routine<T: 'static>(
        &self, 
        mut f: impl FnMut(&mut World, Duration) -> Option<T> + 'static, 
        cancellation: impl Into<TaskCancellation>
    ) -> impl Future<Output = Option<T>> {
        let (sender, receiver) = channel();
        let mut sender = Some(sender);
        let cancel = cancellation.into();
        let fut = self.run(|w| {
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
                    cancel,
                }
            )
        });
        async {
            futures::join!(
                fut,
                receiver
            ).1.ok()
        }
       
    }
}

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
        let world = AsyncWorldMut::ref_cast(&self.executor).clone();
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
    /// let fut = spawn(interpolate(.., Playback::Loop, &cancel));
    /// cancel.cancel();
    /// fut.await
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
        let world =AsyncWorldMut::ref_cast(&self.executor).clone();
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