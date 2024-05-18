use crate::async_systems::AsyncEntityParam;
use crate::async_systems::AsyncWorldParam;
use crate::reactors::Reactors;
use crate::signals::Signals;
use bevy_ecs::component::Component;
use bevy_ecs::system::Resource;
use bevy_ecs::entity::Entity;
use std::marker::PhantomData;

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
#[derive(Debug)]
pub struct AsyncComponent<C: Component> {
    pub(crate) entity: Entity,
    pub(crate) p: PhantomData<C>,
}

impl<C: Component> Copy for AsyncComponent<C> {}

impl<C: Component> Clone for AsyncComponent<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Component> AsyncEntityParam for AsyncComponent<C> {
    type Signal = ();

    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(entity: Entity, _: &Reactors, _: (), _: &[Entity]) -> Option<Self> {
        Some(Self {
            entity,
            p: PhantomData,
        })
    }
}

#[allow(unused)]
pub use bevy_ecs::system::NonSend;

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug)]
pub struct AsyncNonSend<R: 'static>(pub(crate) PhantomData<R>);

impl<R: 'static> Copy for AsyncNonSend<R> {}

impl<R: 'static> Clone for AsyncNonSend<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: 'static> AsyncWorldParam for AsyncNonSend<R> {
    fn from_async_context(_: &Reactors) -> Option<Self> {
        Some(Self(PhantomData))
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

impl<R: Resource> AsyncWorldParam for AsyncResource<R> {
    fn from_async_context(_: &Reactors) -> Option<Self> {
        Some(Self(PhantomData))
    }
}
