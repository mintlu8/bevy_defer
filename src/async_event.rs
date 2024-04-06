use bevy_ecs::event::{Event, EventId, Events, ManualEventReader};
use bevy_ecs::world::World;
use futures::{Future, FutureExt, Stream};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::mem;
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use crate::async_systems::AsyncWorldParam;
use crate::channels::channel;
use crate::queue::AsyncQueryQueue;
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
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
    /// Poll an [`Event`] through cloning.
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

    /// Poll an [`Event`] through mapping.
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

    /// Poll [`Event`]s through cloning until done, result must have at least one value.
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

    /// Poll [`Event`]s through mapping until done, result must have at least one value.
    pub fn poll_all_mapped<T: 'static>(&self, mut f: impl FnMut(&E) -> T + 'static) -> impl Future<Output = Vec<T>> {
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

    /// Poll [`Event`]s until done, optimized for the stream use case.
    fn poll_all_stream<T: 'static>(&self, mut f: impl FnMut(&E) -> T + 'static, mut queue: VecDeque<T>) -> impl Future<Output = VecDeque<T>> {
        let (sender, receiver) = channel();
        let reader = self.reader.clone();
        self.queue.repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let len = queue.len();
                queue.extend(reader.borrow_mut().read(events).map(&mut f));
                if queue.len() == len {
                    return None;
                }
                Some(mem::take(&mut queue))
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Convert the [`AsyncEventReader`] into a [`Stream`] through cloning.
    pub fn into_stream(self) -> EventStream<E, E, impl FnMut(&E) -> E + Clone> where E: Clone{
        EventStream {
            reader: self,
            mapper: E::clone,
            cached: VecDeque::new(),
            future: None,
        }
    }

    /// Convert the [`AsyncEventReader`] into a mapped [`Stream`].
    pub fn into_mapped_stream<F: FnMut(&E) -> O + Clone + 'static, O: 'static>(self, f: F) -> EventStream<E, O, F> {
        EventStream {
            reader: self,
            mapper: f,
            cached: VecDeque::new(),
            future: None,
        }
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

/// A [`Stream`] implementation of [`Events`].
pub struct EventStream<E: Event, Out, F: FnMut(&E) -> Out + Clone> {
    reader: AsyncEventReader<E>,
    mapper: F,
    cached: VecDeque<Out>,
    future: Option<Pin<Box<dyn Future<Output = VecDeque<Out>>>>>
}

impl<E: Event, O, F: FnMut(&E) -> O + Clone> Unpin for EventStream<E, O, F> {}

impl<E: Event, O: 'static, F: FnMut(&E) -> O + Clone + 'static> Stream for EventStream<E, O, F> {
    type Item = O;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(cached) = self.cached.pop_front() {
            return Poll::Ready(Some(cached))
        }
        if let Some(fut) = &mut self.future {
            match fut.poll_unpin(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(v) => {
                    self.future = None;
                    self.cached = v;
                    if let Some(first) = self.cached.pop_front() {
                        return Poll::Ready(Some(first))
                    }
                },
            }
        }
        let queue = mem::take(&mut self.cached);
        self.future = Some(Box::pin(self.reader.poll_all_stream(self.mapper.clone(), queue)));
        self.poll_next(cx)
    }
}