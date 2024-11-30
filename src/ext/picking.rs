//! Reactors for cursor interactions.

use crate::{access::AsyncEntityMut, AsyncWorld};
use bevy::{
    prelude::{Pointer, Trigger},
    reflect::Reflect,
};
use std::fmt::Debug;

use futures::Stream;

impl AsyncEntityMut {
    /// Create a [`Stream`] of a specific event in `bevy_picking`.
    ///
    /// `T` corresponds to `Trigger<Pointer<T>>`.
    ///
    /// # Filtering
    ///
    /// You might want to `filter` the stream to ignore certain buttons when using this feature.
    pub fn on<T: Debug + Reflect + Clone>(&self) -> impl Stream<Item = T> + 'static {
        let entity = self.id();
        let (sender, receiver) = flume::unbounded();
        AsyncWorld.run(|world| {
            world
                .entity_mut(entity)
                .observe(move |trigger: Trigger<Pointer<T>>| {
                    let _ = sender.send(trigger.event().event.clone());
                });
        });
        receiver.into_stream()
    }
}
