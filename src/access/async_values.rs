use crate::async_systems::AsyncEntityParam;
use crate::async_systems::AsyncWorldParam;
use crate::reactors::Reactors;
use crate::signals::Signals;
use bevy_asset::Asset;
use bevy_asset::Handle;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::Resource;
use std::marker::PhantomData;

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
#[derive(Debug)]
pub struct AsyncComponent<C: Component> {
    pub(crate) entity: Entity,
    pub(crate) p: PhantomData<C>,
}

impl<C: Component> AsyncComponent<C> {
    pub fn entity(&self) -> Entity {
        self.entity
    }
}

impl<A: Asset> AsyncComponent<Handle<A>> {
    pub fn into_handle(self) -> AsyncComponentHandle<A> where {
        AsyncComponentHandle {
            entity: self.entity,
            p: PhantomData,
        }
    }
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

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
#[derive(Debug)]
pub struct AsyncComponentHandle<A: Asset> {
    pub(crate) entity: Entity,
    pub(crate) p: PhantomData<A>,
}

impl<A: Asset> AsyncComponentHandle<A> {
    pub fn entity(&self) -> Entity {
        self.entity
    }

    pub fn into_component(self) -> AsyncComponent<Handle<A>> {
        AsyncComponent {
            entity: self.entity,
            p: PhantomData,
        }
    }
}

impl<A: Asset> Copy for AsyncComponentHandle<A> {}

impl<A: Asset> Clone for AsyncComponentHandle<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: Asset> AsyncEntityParam for AsyncComponentHandle<A> {
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

/// An `AsyncSystemParam` that gets or sets a `!Send` resource on the `World`.
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
