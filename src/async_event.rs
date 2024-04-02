use bevy_ecs::event::{Event, EventId, Events, ManualEventReader};
use bevy_ecs::world::World;
use futures::{Future, FutureExt};
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use crate::async_systems::AsyncWorldParam;
use crate::channels::channel;
use crate::executor::AsyncQueryQueue;
use crate::{AsyncFailure, AsyncResult};
use crate::{access::AsyncWorldMut, CHANNEL_CLOSED};

impl AsyncWorldMut {
    /// Send an [`Event`].
    pub fn send_event<E: Event>(&self, event: E) -> impl Future<Output = AsyncResult<EventId<E>>> {
        let (sender, receiver) = channel();
        self.queue.once(
            move |world: &mut World| {
                world.send_event(event).ok_or(AsyncFailure::EventNotRegistered)
            },
            sender
        );
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    /// Create an [`AsyncEventReader`].
    pub fn event_reader<E: Event>(&self) -> AsyncEventReader<E> {
        AsyncEventReader { queue: self.queue.clone(), reader: Default::default() }
    }
}

/// Async version of `EventReader`.
/// 
/// # Note
/// 
/// Unlike most accessors this struct holds internal state 
/// as a [`ManualEventReader`],
/// which keeps track of the current event tick.
#[derive(Debug, Clone)]
pub struct AsyncEventReader<E: Event> {
    queue: Rc<AsyncQueryQueue>,
    reader: Rc<RefCell<ManualEventReader<E>>>
}

impl<E: Event> AsyncEventReader<E> {
    /// Poll an [`Event`].
    /// 
    /// # Note
    /// 
    /// Only receives events sent after this call.
    pub fn poll(&self) -> impl Future<Output = E> where E: Clone {
        let (sender, receiver) = channel();
        let reader = self.reader.clone();
        self.queue.repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let result = reader.borrow_mut().read(events).next().cloned();
                result
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Poll an [`Event`].
    /// 
    /// # Note
    /// 
    /// Only receives events sent after this call.
    pub fn poll_mapped<T: Clone + 'static>(&self, mut f: impl FnMut(&E) -> T + 'static) -> impl Future<Output = T> {
        let (sender, receiver) = channel();
        let reader = self.reader.clone();
        self.queue.repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let result = reader.borrow_mut().read(events).next().map(&mut f);
                result
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Poll [`Event`]s until done, result must have at least one value.
    /// 
    /// # Note
    /// 
    /// Only receives events sent after this call.
    pub fn poll_all(&self) -> impl Future<Output = Vec<E>> where E: Clone {
        let (sender, receiver) = channel();
        let reader = self.reader.clone();
        self.queue.repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let result: Vec<_> = reader.borrow_mut().read(events).cloned().collect();
                if result.is_empty() {
                    return None;
                }
                Some(result)
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Poll [`Event`]s until done, result must have at least one value.
    /// 
    /// # Note
    /// 
    /// Only receives events sent after this call.
    pub fn poll_all_mapped<T: Clone + 'static>(&self, mut f: impl FnMut(&E) -> T + 'static) -> impl Future<Output = Vec<T>> {
        let (sender, receiver) = channel();
        let reader = self.reader.clone();
        self.queue.repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let result: Vec<_> = reader.borrow_mut().read(events).map(&mut f).collect();
                if result.is_empty() {
                    return None;
                }
                Some(result)
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}

impl<E: Event> AsyncWorldParam for AsyncEventReader<E> {
    fn from_async_context(executor: &AsyncWorldMut) -> Option<Self> {
        Some(executor.event_reader::<E>())
    }
}

/// Add method to [`AsyncEventReader`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncEventReaderDeref: Event + Sized {
    type Target;
    fn async_deref(this: &AsyncEventReader<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncEventReader<C> where C: AsyncEventReaderDeref{
    type Target = <C as AsyncEventReaderDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncEventReaderDeref::async_deref(self)
    }
}