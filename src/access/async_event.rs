use crate::access::AsyncWorld;
use crate::async_systems::AsyncWorldParam;
use crate::executor::{with_world_mut, REACTORS};
use crate::reactors::Reactors;
use crate::{AccessError, AccessResult};
use bevy_core::FrameCount;
use bevy_ecs::event::{Event, EventId, EventReader};
use bevy_ecs::system::{Local, Res};
use bevy_ecs::world::World;
use event_listener::EventListener;
use futures::{Future, Stream};
use parking_lot::RwLock;
use std::cell::OnceCell;
use std::pin::{pin, Pin};
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
pub struct DoubleBufferedEvent<E: Event> {
    last: RwLock<Vec<E>>,
    this: RwLock<Vec<E>>,
    wakers: event_listener::Event,
    current_frame: AtomicU32,
}

impl<E: Event + Clone> DoubleBufferedEvent<E> {
    pub fn into_stream(self: Arc<Self>) -> EventStream<E> {
        EventStream {
            frame: self.current_frame.load(Ordering::Relaxed).wrapping_sub(1),
            index: 0,
            listener: None,
            event: self,
        }
    }
}

/// A [`Stream`] of [`Event`]s, requires system [`react_to_event`] to function.
///
/// This follows bevy's double buffering semantics.
#[derive(Debug)]
pub struct EventStream<E: Event + Clone> {
    frame: u32,
    index: usize,
    listener: Option<EventListener>,
    event: Arc<DoubleBufferedEvent<E>>,
}

impl<E: Event + Clone> Clone for EventStream<E> {
    fn clone(&self) -> Self {
        Self {
            frame: self.frame,
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
        let current_frame = this.event.current_frame.load(Ordering::Relaxed);
        let listener = this.listener.get_or_insert_with(||this.event.wakers.listen());
        loop {
            if current_frame != this.frame {
                if this.frame != current_frame.wrapping_sub(1) {
                    this.frame = current_frame.wrapping_sub(1);
                    this.index = 0;
                }
                let lock = this.event.last.read();
                if let Some(event) = lock.get(this.index).cloned() {
                    this.index += 1;
                    this.listener = None;
                    return Poll::Ready(Some(event));
                } else {
                    this.frame = current_frame;
                    this.index = 0;
                }
            }
            let lock = this.event.this.read();
            if let Some(event) = lock.get(this.index).cloned() {
                this.index += 1;
                this.listener = None;
                return Poll::Ready(Some(event))
            } else {
                match pin!(&mut *listener).poll(cx) {
                    Poll::Ready(()) => (),
                    Poll::Pending => return Poll::Pending,
                }
            }
        }
    }
}

impl<E: Event> Default for DoubleBufferedEvent<E> {
    fn default() -> Self {
        Self {
            last: Default::default(),
            this: Default::default(),
            wakers: Default::default(),
            current_frame: Default::default(),
        }
    }
}

/// React to an event, this system is safe to be repeated in the schedule.
pub fn react_to_event<E: Event + Clone>(
    cached: Local<OnceCell<Arc<DoubleBufferedEvent<E>>>>,
    frame: Res<FrameCount>,
    reactors: Res<Reactors>,
    mut reader: EventReader<E>,
) {
    let buffers = cached.get_or_init(|| reactors.get_event::<E>());
    if !reader.is_empty() {
        buffers.wakers.notify(usize::MAX);
    }
    if buffers.current_frame.swap(frame.0, Ordering::Relaxed) == frame.0 {
        buffers.this.write().extend(reader.read().cloned());
    } else {
        let mut this = buffers.this.write();
        let mut last = buffers.last.write();
        std::mem::swap::<Vec<_>>(this.as_mut(), last.as_mut());
        this.clear();
        this.extend(reader.read().cloned());
    }
}

impl<E: Event + Clone> AsyncWorldParam for EventStream<E> {
    fn from_async_context(reactors: &Reactors) -> Option<Self> {
        Some(reactors.get_event().into_stream())
    }
}
