use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::resource::Resource;
use std::any::type_name;
use std::fmt::Debug;
use std::marker::PhantomData;

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
pub struct AsyncComponent<C: Component, E: VirtualEntity = Entity> {
    pub(crate) entity: E,
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

impl<C: Component, E: VirtualEntity> AsyncComponent<C, E> {
    pub fn entity(self) -> AsyncEntity<E> {
        AsyncEntity(self.entity)
    }
}

impl<C: Component> AsyncComponent<C, Entity> {
    pub fn id(&self) -> Entity {
        self.entity
    }
}

impl<C: Component, E: VirtualEntity + Copy> Copy for AsyncComponent<C, E> {}

impl<C: Component, E: VirtualEntity + Clone> Clone for AsyncComponent<C, E> {
    fn clone(&self) -> Self {
        AsyncComponent {
            entity: self.entity.clone(),
            p: PhantomData,
        }
    }
}

impl<C: Component> From<Entity> for AsyncComponent<C> {
    fn from(entity: Entity) -> Self {
        AsyncComponent {
            entity,
            p: PhantomData,
        }
    }
}

#[allow(unused)]
pub use bevy::ecs::system::NonSend;

use crate::access::get_entity::VirtualEntity;
use crate::access::AsyncEntity;
use crate::executor::with_world_ref;
use crate::{AccessError, AccessResult, AsyncWorld};

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

impl<R: Resource> AsyncResource<R> {
    /// Get the underlying [`AsyncEntity`] of a resource.
    pub fn entity(&self) -> AccessResult<AsyncEntity> {
        AsyncWorld.resource_entity::<R>()
    }

    /// Get the underlying [`Entity`] of a resource.
    pub fn id(&self) -> AccessResult<Entity> {
        AsyncWorld.resource_entity::<R>().map(|x| x.id())
    }
}

impl AsyncWorld {
    /// Get the underlying [`AsyncEntity`] of a resource.
    pub fn resource_entity<R: Resource>(&self) -> AccessResult<AsyncEntity> {
        with_world_ref(|world| {
            if let Some(id) = world.component_id::<R>() {
                if let Some(entity) = world.resource_entities().get(id) {
                    return Ok(AsyncEntity(entity));
                }
            }
            Err(AccessError::ResourceNotFound {
                name: type_name::<R>(),
            })
        })
    }
}
