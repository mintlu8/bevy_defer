use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::resource::Resource;
use std::any::type_name;
use std::fmt::Debug;
use std::marker::PhantomData;

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
pub struct AsyncComponent<C: Component> {
    pub(crate) entity: Entity,
    pub(crate) p: PhantomData<C>,
}

impl<C: Component> Debug for AsyncComponent<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncComponent")
            .field("type", &type_name::<C>())
            .field("entity", &self.entity)
            .finish()
    }
}

impl<C: Component> AsyncComponent<C> {
    pub fn entity(&self) -> AsyncEntityMut {
        AsyncEntityMut(self.entity)
    }

    pub fn id(&self) -> Entity {
        self.entity
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
pub struct AsyncNonSend<R: 'static>(pub(crate) PhantomData<R>);

impl<R: 'static> Debug for AsyncNonSend<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AsyncNonSend")
            .field(&type_name::<R>())
            .finish()
    }
}

impl<R: 'static> Copy for AsyncNonSend<R> {}

impl<R: 'static> Clone for AsyncNonSend<R> {
    fn clone(&self) -> Self {
        *self
    }
}

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
pub struct AsyncResource<R: Resource>(pub(crate) PhantomData<R>);

impl<R: Resource> Debug for AsyncResource<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AsyncResource")
            .field(&type_name::<R>())
            .finish()
    }
}

impl<R: Resource> Copy for AsyncResource<R> {}

impl<R: Resource> Clone for AsyncResource<R> {
    fn clone(&self) -> Self {
        *self
    }
}
