use std::fmt::Debug;
use std::{borrow::Borrow, marker::PhantomData};

use bevy_ecs::event::{Event, EventId, Events, ManualEventReader};
use bevy_ecs::world::FromWorld;
use bevy_ecs::{system::Resource, world::World};
use futures::{channel::oneshot::channel, Future};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use triomphe::Arc;

use crate::{signal_inner::SignalInner, AsyncExtension, AsyncWorldMut, BoxedQueryCallback, CHANNEL_CLOSED};
use crate::signals::{SignalData, SignalId};

/// A resource containing named signals.
#[derive(Resource)]
pub struct NamedSignals<T: SignalId>{
    map: Mutex<FxHashMap<String, Arc<SignalData<T::Data>>>>,
    p: PhantomData<T>
}

impl<T: SignalId> Debug for NamedSignals<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamedSignals").field("map", &self.map.lock().len()).finish()
    }
}

impl<T: SignalId> Default for NamedSignals<T> {
    fn default() -> Self {
        Self { map: Default::default(), p: Default::default() }
    }
}

impl<T: SignalId> NamedSignals<T> {
    /// Obtain a named signal.
    pub fn get(&mut self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>>{
        if let Some(data) = self.map.get_mut().get(name.borrow()){
            data.clone()
        } else {
            let data = Arc::new(SignalData::default());
            self.map.get_mut().insert(name.into(), data.clone());
            data
        }
    }

    /// Obtain a named signal through locking.
    pub fn get_from_ref(&self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>>{
        let mut map = self.map.lock();
        if let Some(data) = map.get(name.borrow()){
            data.clone()
        } else {
            let data = Arc::new(SignalData::default());
            map.insert(name.into(), data.clone());
            data
        }
    }
}

impl AsyncWorldMut {
    /// Obtain a named signal.
    pub fn signal<T: SignalId>(&self, name: impl Into<String>) -> impl Future<Output = Arc<SignalData<T::Data>>> {
        let (sender, receiver) = channel();
        let name = name.into();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.signal::<T>(name)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
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
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.signal::<T>(name)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
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
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                SignalInner::from(world.signal::<T>(name.clone())).write(value);
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    /// Send an [`Event`].
    pub fn send_event<E: Event>(&self, event: E) -> impl Future<Output = Option<EventId<E>>> {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.send_event(event)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
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
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                let mut reader = AsyncEventReader::from_world(world);
                let events = world.get_resource::<Events<E>>()?;
                let result = reader.0.read(events).next().cloned();
                result
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
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
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                let mut reader = AsyncEventReader::from_world(world);
                let events = world.get_resource::<Events<E>>()?;
                let result: Vec<_> = reader.0.read(events).cloned().collect();
                if result.len() == 0 {
                    return None;
                }
                Some(result)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }
}

#[derive(Debug, Resource)]
struct AsyncEventReader<E: Event>(ManualEventReader<E>);

impl<E: Event> FromWorld for AsyncEventReader<E> {
    fn from_world(world: &mut World) -> Self {
        let mut reader = ManualEventReader::default();
        if let Some(events) = world.get_resource::<Events<E>>() {
            reader.clear(events)
        }
        Self(reader)
    }
}