use crate::sync::oneshot::ChannelOutOrCancel;
use crate::sync::waitlist::WaitList;
use crate::{
    access::AsyncWorldMut, cancellation::TaskCancellation, channel, sync::oneshot::Sender,
    QueryQueue,
};
use bevy_core::FrameCount;
use bevy_ecs::system::NonSend;
use bevy_ecs::system::Res;
use bevy_ecs::world::World;
use bevy_time::Time;
use bevy_utils::Duration;
use std::{cell::Cell, cell::RefCell, collections::BinaryHeap};

#[allow(unused)]
use bevy_app::Update;

/// A Task running on [`Update`] once per frame.
pub(crate) struct FixedTask {
    task: Box<dyn FnMut(&mut World, Duration) -> bool>,
    cancel: TaskCancellation,
}

/// A deferred query on a `World`.
pub(crate) struct QueryCallback {
    command: Box<dyn FnMut(&mut World) -> bool + 'static>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TimeIndex<T: Ord, V>(T, V);

impl<T: Ord, V> PartialEq for TimeIndex<T, V> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Ord, V> Eq for TimeIndex<T, V> {}

impl<T: Ord, V> PartialOrd for TimeIndex<T, V> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord, V> Ord for TimeIndex<T, V> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0).reverse()
    }
}

/// Queue for deferred `!Send` queries applied on the [`World`].
#[derive(Default)]
pub struct AsyncQueryQueue {
    pub(crate) repeat_queue: RefCell<Vec<QueryCallback>>,
    pub(crate) fixed_queue: RefCell<Vec<FixedTask>>,
    pub(crate) time_series: RefCell<BinaryHeap<TimeIndex<Duration, Sender<()>>>>,
    pub(crate) frame_series: RefCell<BinaryHeap<TimeIndex<u32, Sender<()>>>>,
    pub(crate) yielded: WaitList,
    pub(crate) now: Cell<Duration>,
    pub(crate) frame: Cell<u32>,
}

impl std::fmt::Debug for AsyncQueryQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncQueryQueue")
            .field("repeat_queue", &self.repeat_queue.borrow().len())
            .field("fixed_queue", &self.fixed_queue.borrow().len())
            .field("time_series", &self.time_series.borrow().len())
            .field("frames_series", &self.frame_series.borrow().len())
            .field("yielded", &self.yielded.len())
            .field("now", &self.now.get())
            .field("frame", &self.frame.get())
            .finish_non_exhaustive()
    }
}

impl QueryCallback {
    fn new<Out: 'static>(
        mut query: impl (FnMut(&mut World) -> Option<Out>) + 'static,
        channel: Sender<Out>,
    ) -> Self {
        let mut channel = channel.by_ref();
        Self {
            command: Box::new(move |w| {
                if channel.is_closed() {
                    return false;
                }
                match query(w) {
                    Some(result) => {
                        channel.send(result);
                        false
                    }
                    None => true,
                }
            }),
        }
    }
}

impl AsyncQueryQueue {
    /// Spawn a `!Send` command and wait until it returns `Some`.
    ///
    /// If receiver is dropped, the command will be cancelled.
    pub fn repeat<Out: 'static>(
        &self,
        query: impl (FnMut(&mut World) -> Option<Out>) + 'static,
        channel: Sender<Out>,
    ) {
        self.repeat_queue
            .borrow_mut()
            .push(QueryCallback::new(query, channel))
    }

    pub fn timed(&self, duration: Duration, channel: Sender<()>) {
        self.time_series
            .borrow_mut()
            .push(TimeIndex(self.now.get() + duration, channel))
    }

    pub fn timed_frames(&self, duration: u32, channel: Sender<()>) {
        self.frame_series
            .borrow_mut()
            .push(TimeIndex(self.frame.get() + duration, channel))
    }
}

/// System that tries to resolve queries sent to the queue.
pub fn run_async_queries(w: &mut World) {
    let queue = w.non_send_resource::<QueryQueue>().0.clone();
    queue
        .repeat_queue
        .borrow_mut()
        .retain_mut(|f| (f.command)(w));
    queue.yielded.wake();
}

/// Run `fixed_queue` on [`Update`].
pub fn run_fixed_queue(world: &mut World) {
    let executor = world.non_send_resource::<QueryQueue>().0.clone();
    let delta_time = world.resource::<Time>().delta();
    executor.fixed_queue.borrow_mut().retain_mut(|x| {
        if x.cancel.cancelled() {
            return false;
        }
        !(x.task)(world, delta_time)
    });
}

/// Run `sleep` and `sleep_frames` reactors.
pub fn run_time_series(queue: NonSend<QueryQueue>, time: Res<Time>, frames: Res<FrameCount>) {
    let now = time.elapsed();
    queue.now.set(now);
    let mut time_series = queue.time_series.borrow_mut();
    while time_series.peek().map(|x| x.0 <= now).unwrap_or(false) {
        let _ = time_series.pop().unwrap().1.send(());
    }
    queue.frame.set(frames.0);
    let mut frame_series = queue.frame_series.borrow_mut();
    while frame_series
        .peek()
        .map(|x| x.0 <= frames.0)
        .unwrap_or(false)
    {
        let _ = frame_series.pop().unwrap().1.send(());
    }
}

impl AsyncWorldMut {
    /// Run a repeatable routine on [`Update`], with access to delta time.
    pub fn timed_routine<T: 'static>(
        &self,
        mut f: impl FnMut(&mut World, Duration) -> Option<T> + 'static,
        cancellation: impl Into<TaskCancellation>,
    ) -> ChannelOutOrCancel<T> {
        let (sender, receiver) = channel();
        let mut sender = sender.by_ref();
        let cancel = cancellation.into();
        self.queue.fixed_queue.borrow_mut().push(FixedTask {
            task: Box::new(move |world, dt| {
                if let Some(item) = f(world, dt) {
                    sender.send(item);
                    true
                } else {
                    false
                }
            }),
            cancel,
        });
        receiver.into_option()
    }
}
