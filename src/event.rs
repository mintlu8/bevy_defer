use bevy_ecs::event::{Event, EventId, Events, ManualEventReader};
use bevy_ecs::world::World;
use futures::Future;
use std::sync::Arc;
use crate::channels::channel;
use crate::{AsyncFailure, AsyncResult};
use crate::{signals::SignalInner, AsyncExtension, access::AsyncWorldMut, QueryCallback, CHANNEL_CLOSED};
use crate::signals::{SignalData, SignalId};

impl AsyncWorldMut {
    /// Obtain a named signal.
    pub fn signal<T: SignalId>(&self, name: impl Into<String>) -> impl Future<Output = Arc<SignalData<T::Data>>> {
        let (sender, receiver) = channel();
        let name = name.into();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.signal::<T>(name)
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

    /// Poll a named signal.
    pub fn poll<T: SignalId>(&self, name: impl Into<String>) -> impl Future<Output = T::Data> {
        let (sender, receiver) = channel();
        let name = name.into();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.signal::<T>(name)
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        async {
            let signal = receiver.await.expect(CHANNEL_CLOSED);
            SignalInner::from(signal).async_read().await
        }
    }

    /// Send data through a named signal.
    pub fn send<T: SignalId>(&self, name: impl Into<String>, value: T::Data) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let name = name.into();
        let query = QueryCallback::once(
            move |world: &mut World| {
                SignalInner::from(world.signal::<T>(name.clone())).write(value);
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

    /// Poll an [`Event`].
    /// 
    /// # Note
    /// 
    /// Only receives events sent after this call.
    pub fn poll_event<E: Event + Clone>(&self) -> impl Future<Output = E> {
        let (sender, receiver) = channel();
        let mut reader = None;
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let reader = match &mut reader {
                    Some(reader) => reader,
                    None => {
                        reader = Some({
                            let mut reader = ManualEventReader::default();
                            reader.clear(events);
                            reader
                        });
                        reader.as_mut().unwrap()
                    }
                };
                let result = reader.read(events).next().cloned();
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
    pub fn poll_events<E: Event + Clone>(&self) -> impl Future<Output = Vec<E>> {
        let (sender, receiver) = channel();
        let mut reader = None;
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let events = world.get_resource::<Events<E>>()?;
                let reader = match &mut reader {
                    Some(reader) => reader,
                    None => {
                        reader = Some({
                            let mut reader = ManualEventReader::default();
                            reader.clear(events);
                            reader
                        });
                        reader.as_mut().unwrap()
                    }
                };
                let result: Vec<_> = reader.read(events).cloned().collect();
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
