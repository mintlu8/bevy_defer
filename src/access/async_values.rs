use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::Resource;
use std::marker::PhantomData;

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
#[derive(Debug)]
pub struct AsyncComponent<C: Component> {
    pub(crate) entity: Entity,
    pub(crate) p: PhantomData<C>,
}

impl<C: Component> AsyncComponent<C> {
    pub fn entity(&self) -> AsyncEntityMut {
        AsyncEntityMut(self.entity)
    }
}

impl<C: Component> Copy for AsyncComponent<C> {}

impl<C: Component> Clone for AsyncComponent<C> {
    fn clone(&self) -> Self {
        *self
    }
}

#[allow(unused)]
pub use bevy::ecs::system::NonSend;

use super::AsyncEntityMut;

/// An `AsyncSystemParam` that gets or sets a `!Send` resource on the `World`.
#[derive(Debug)]
pub struct AsyncNonSend<R: 'static>(pub(crate) PhantomData<R>);

impl<R: 'static> Copy for AsyncNonSend<R> {}

impl<R: 'static> Clone for AsyncNonSend<R> {
    fn clone(&self) -> Self {
        *self
    }
}

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug)]
pub struct AsyncResource<R: Resource>(pub(crate) PhantomData<R>);

impl<R: Resource> Copy for AsyncResource<R> {}

impl<R: Resource> Clone for AsyncResource<R> {
    fn clone(&self) -> Self {
        *self
    }
}
