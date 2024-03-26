use bevy_ecs::entity::Entity;
use bevy_ecs::event::{Event, EventId, Events, ManualEventReader};
use bevy_ecs::world::World;
use futures::Future;
use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use crate::async_systems::AsyncEntityParam;
use crate::channels::channel;
use crate::executor::AsyncQueryQueue;
use crate::{AsyncFailure, AsyncResult};
use crate::{access::AsyncWorldMut, QueryCallback, CHANNEL_CLOSED};

impl AsyncWorldMut {
    /// Send an [`Event`].
    pub fn send_event<E: Event>(&self, event: E) -> impl Future<Output = AsyncResult<EventId<E>>> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.send_event(event).ok_or(AsyncFailure::EventNotRegistered)
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    pub fn event<E: Event>(&self) -> AsyncEventReader<E> {
        AsyncEventReader { queue: self.queue.clone(), reader: Default::default() }
    }
}

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
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let result = reader.borrow_mut().read(events).next().cloned();
                result
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    /// Poll an [`Event`].
    /// 
    /// # Note
    /// 
    /// Only receives events sent after this call.
    pub fn poll_mapped<T: Clone + 'static>(&self, mut f: impl FnMut(&E) -> T + 'static) -> impl Future<Output = T> {
        let (sender, receiver) = channel();
        let reader = self.reader.clone();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let result = reader.borrow_mut().read(events).next().map(&mut f);
                result
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    /// Poll [`Event`]s until done, result must have at least one value.
    /// 
    /// # Note
    /// 
    /// Only receives events sent after this call.
    pub fn poll_all(&self) -> impl Future<Output = Vec<E>> where E: Clone {
        let (sender, receiver) = channel();
        let reader = self.reader.clone();
        let query = QueryCallback::repeat(
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
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    /// Poll [`Event`]s until done, result must have at least one value.
    /// 
    /// # Note
    /// 
    /// Only receives events sent after this call.
    pub fn poll_all_mapped<T: Clone + 'static>(&self, mut f: impl FnMut(&E) -> T + 'static) -> impl Future<Output = Vec<T>> {
        let (sender, receiver) = channel();
        let reader = self.reader.clone();
        let query = QueryCallback::repeat(
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
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }
}

impl<E: Event> AsyncEntityParam for AsyncEventReader<E> {
    type Signal = ();

    fn fetch_signal(_: &crate::signals::Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &AsyncWorldMut,
        _: Self::Signal,
        _: &[Entity],
    ) -> Option<Self> {
        Some(executor.event::<E>())
    }
}

/// Add method to [`AsyncEventReaderDeref`] through deref.
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