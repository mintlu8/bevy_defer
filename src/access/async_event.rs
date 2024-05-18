use crate::access::AsyncWorld;
use crate::async_systems::AsyncWorldParam;
use crate::executor::{with_world_mut, REACTORS};
use crate::reactors::Reactors;
use crate::{AccessError, AccessResult};
use bevy_ecs::event::{Event, EventId, EventReader};
use bevy_ecs::system::{Local, Res};
use bevy_ecs::world::World;
use event_listener::EventListener;
use event_listener_strategy::{NonBlocking, Strategy};
use futures::Stream;
use parking_lot::RwLock;
use std::cell::OnceCell;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

impl AsyncWorld {
    /// Send an [`Event`].
    pub fn send_event<E: Event>(&self, event: E) -> AccessResult<EventId<E>> {
        with_world_mut(move |world: &mut World| {
            world
                .send_event(event)
                .ok_or(AccessError::EventNotRegistered)
        })
    }

    /// Create a stream to an [`Event`], requires the corresponding [`react_to_event`] system.
    ///
    /// This requires [`Clone`] and duplicates all events sent in bevy.
    pub fn event_stream<E: Event + Clone>(&self) -> EventStream<E> {
        if !REACTORS.is_set() {
            panic!("Can only be used within a `bevy_defer` future.")
        }
        REACTORS.with(|r| r.get_event::<E>()).into_stream()
    }
}

/// A double buffered [`Stream`] of [`Event`]s.
#[derive(Debug)]
pub struct EventBuffer<E: Event> {
    buffer: RwLock<Vec<E>>,
    notify: event_listener::Event,
    tick: AtomicU32,
}

impl<E: Event + Clone> EventBuffer<E> {
    pub fn into_stream(self: Arc<Self>) -> EventStream<E> {
        EventStream {
            tick: self.tick.load(Ordering::Acquire).wrapping_sub(1),
            index: 0,
            listener: None,
            event: self,
        }
    }
}

/// A [`Stream`] of [`Event`]s, requires system [`react_to_event`] to function.
#[derive(Debug)]
pub struct EventStream<E: Event + Clone> {
    tick: u32,
    index: usize,
    listener: Option<EventListener>,
    event: Arc<EventBuffer<E>>,
}

impl<E: Event + Clone> Clone for EventStream<E> {
    fn clone(&self) -> Self {
        Self {
            tick: self.tick,
            index: self.index,
            listener: None,
            event: self.event.clone(),
        }
    }
}

impl<E: Event + Clone> Unpin for EventStream<E> {}

impl<E: Event + Clone> Stream for EventStream<E> {
    type Item = E;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        loop {
            let current_tick = this.event.tick.load(Ordering::Acquire);
            if current_tick != this.tick {
                this.tick = current_tick;
                this.index = 0;
            }
            let lock = this.event.buffer.read();
            let value = lock.get(this.index).cloned();
            this.listener
                .get_or_insert_with(|| this.event.notify.listen());
            if let Some(event) = value {
                this.index += 1;
                return Poll::Ready(Some(event));
            } else {
                drop(lock);
                match NonBlocking::default().poll(&mut this.listener, cx) {
                    Poll::Ready(()) => (),
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
    }
}

impl<E: Event> Default for EventBuffer<E> {
    fn default() -> Self {
        Self {
            notify: Default::default(),
            tick: Default::default(),
            buffer: Default::default(),
        }
    }
}

/// React to an event.
///
/// Consecutive calls will flush the stream, make sure to order this against the executor correctly.
pub fn react_to_event<E: Event + Clone>(
    cached: Local<OnceCell<Arc<EventBuffer<E>>>>,
    reactors: Res<Reactors>,
    mut reader: EventReader<E>,
) {
    let buffers = cached.get_or_init(|| reactors.get_event::<E>());
    buffers.tick.fetch_add(1, Ordering::AcqRel);
    if !reader.is_empty() {
        buffers.notify.notify(usize::MAX);
        let mut lock = buffers.buffer.write();
        lock.drain(..);
        lock.extend(reader.read().cloned());
    };
}

impl<E: Event + Clone> AsyncWorldParam for EventStream<E> {
    fn from_async_context(reactors: &Reactors) -> Option<Self> {
        Some(reactors.get_event().into_stream())
    }
}
