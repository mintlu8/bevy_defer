use bevy_ecs::world::World;
use bevy_time::{Fixed, Time};
use bevy_utils::Duration;
use futures::Future;

use crate::{async_world::AsyncWorldMut, cancellation::TaskCancellation, channel};

#[allow(unused)]
use bevy_app::FixedUpdate;

/// A Task running on [`FixedUpdate`].
pub(crate) struct FixedTask {
    task: Box<dyn FnMut(&mut World, Duration) -> bool>,
    cancel: TaskCancellation,
}

/// A `!Send` thread-local queue running on [`FixedUpdate`].
#[derive(Default)]
pub struct FixedQueue{
    inner: Vec<FixedTask>
}

/// Run [`FixedQueue`] on [`FixedUpdate`].
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
    /// Run a repeatable routine on [`FixedUpdate`], with access to delta time.
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