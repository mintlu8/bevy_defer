use crate::{AccessResult, AsyncAccess, AsyncWorld};
use bevy::ecs::{
    event::EventReader,
    system::{ResMut, Resource},
};
use event_listener::Event;
use std::collections::VecDeque;

/// An event queue that functions as a Mpmc channel.
///
/// Unlike bevy's events, each event can be read at most once, which is
/// more consistent for the async use case.
///
/// Items can be pushed and popped synchronously in systems via [`ResMut<EventChannel<E>>`]
/// or asynchronously in a `bevy_defer` future.
/// Use [`AsyncWorld::send_oneshot_event`] to send and
/// [`AsyncWorld::next_event`] to read as a stream.
///
/// Add [`react_to_event`] to react to actual bevy events.
#[derive(Debug, Resource)]
pub struct EventChannel<T: Send + Sync> {
    queue: VecDeque<T>,
    event: Event,
}
impl<T: Send + Sync> Default for EventChannel<T> {
    fn default() -> Self {
        Self {
            queue: Default::default(),
            event: Default::default(),
        }
    }
}

impl<T: Send + Sync> EventChannel<T> {
    pub fn take(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    pub fn push(&mut self, value: T) {
        if self.queue.is_empty() {
            self.event.notify(usize::MAX);
        }
        self.queue.push_back(value);
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

impl<T: Send + Sync> Extend<T> for EventChannel<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        if self.queue.is_empty() {
            self.event.notify(usize::MAX);
        }
        self.queue.extend(iter);
    }
}

impl AsyncWorld {
    /// Obtain and remove the next event from a [`EventChannel`].
    ///
    /// # Panics
    ///
    /// If the event is not registered.
    /// Call `react_to_event` if that happens.
    pub async fn next_event<E: Clone + Send + Sync + 'static>(&self) -> E {
        loop {
            let result = AsyncWorld
                .resource::<EventChannel<E>>()
                .get_mut(|x| x.take())
                .expect("Event not registered");
            if let Some(result) = result {
                return result;
            } else {
                AsyncWorld
                    .resource::<EventChannel<E>>()
                    .get(|x| x.event.listen())
                    .expect("Event not registered")
                    .await;
            }
        }
    }

    /// Obtain and remove the next event from a [`EventChannel`].
    pub async fn get_next_event<E: Clone + Send + Sync + 'static>(&self) -> AccessResult<E> {
        loop {
            let result = AsyncWorld
                .resource::<EventChannel<E>>()
                .get_mut(|x| x.take())?;
            if let Some(result) = result {
                return Ok(result);
            } else {
                AsyncWorld
                    .resource::<EventChannel<E>>()
                    .get(|x| x.event.listen())?
                    .await;
            }
        }
    }

    /// Send an one-shot event via [`EventChannel`].
    pub fn send_oneshot_event<E: Send + Sync + 'static>(&self, event: E) -> AccessResult {
        AsyncWorld
            .resource::<EventChannel<E>>()
            .get_mut(|x| x.push(event))
    }
}

/// Copy an event from an [`EventReader`] to an [`EventChannel`].
pub fn react_to_event<E: bevy::prelude::Event + Clone>(
    mut reader: EventReader<E>,
    mut channel: ResMut<EventChannel<E>>,
) {
    channel.clear();
    if !reader.is_empty() {
        channel.extend(reader.read().cloned());
    };
}
