use crate::access::{AsyncComponent, AsyncEntityQuery, AsyncNonSend, AsyncQuery, AsyncResource};
use crate::executor::{QUERY_QUEUE, WORLD};
use crate::in_async_context;
use bevy::ecs::{
    component::Component,
    entity::Entity,
    name::Name,
    query::{QueryData, QueryFilter},
    resource::Resource,
};
use ref_cast::RefCast;
use std::borrow::Borrow;
use std::fmt::Display;
use std::time::Duration;
use std::{marker::PhantomData, ops::Deref};

#[allow(unused)]
use bevy::ecs::{system::Commands, world::World};

#[allow(unused)]
use crate::{AsyncExecutor, QueryQueue};

use super::AsyncQuerySingle;

/// Async version of [`World`] or [`Commands`].
///
/// This type only works inside a `bevy_defer` future,
/// calling any function outside of a `bevy_defer` future
/// or inside a world access function (a closure with `World` as a parameter)
/// will likely panic.
///
/// If you need the functionalities defined here in sync code, see non-send resources
/// [`AsyncExecutor`] and [`QueryQueue`].
#[derive(Debug, Copy, Clone)]
pub struct AsyncWorld;

impl AsyncWorld {
    /// Acquire an `AsyncWorld`, checks if in a `bevy_defer` future.
    ///
    /// # Panics
    ///
    /// When used outside a `bevy_defer` future.
    ///
    /// # Note
    ///
    /// You can construct [`AsyncWorld`] directly without this error.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        if !in_async_context() {
            panic!("AsyncWorld can only be used in a bevy_defer future.")
        }
        AsyncWorld
    }

    /// Obtain an [`AsyncEntityMut`] of the entity.
    ///
    /// # Note
    ///
    /// This does not mean the entity exists in the world.
    pub fn entity(&self, entity: Entity) -> AsyncEntityMut {
        AsyncEntityMut(entity)
    }

    /// Obtain an [`struct@AsyncResource`].
    ///
    /// # Note
    ///
    /// This does not mean the resource exists in the world.
    pub fn resource<R: Resource>(&self) -> AsyncResource<R> {
        AsyncResource(PhantomData)
    }

    /// Obtain an [`struct@AsyncNonSend`].
    ///
    /// # Note
    ///
    /// This does not mean the resource exists in the world.
    pub fn non_send_resource<R: 'static>(&self) -> AsyncNonSend<R> {
        AsyncNonSend(PhantomData)
    }

    /// Obtain an [`AsyncQuery`].
    pub fn query<Q: QueryData>(&self) -> AsyncQuery<Q> {
        AsyncQuery(PhantomData)
    }

    /// Obtain an [`AsyncQuerySingle`].
    pub fn query_single<Q: QueryData>(&self) -> AsyncQuerySingle<Q> {
        AsyncQuerySingle(PhantomData)
    }

    /// Obtain an [`AsyncQuery`].
    pub fn query_filtered<Q: QueryData, F: QueryFilter>(&self) -> AsyncQuery<Q, F> {
        AsyncQuery(PhantomData)
    }

    /// Obtain duration from `init`, according to the executor.
    pub fn now(&self) -> Duration {
        QUERY_QUEUE.with(|q| q.now.get())
    }

    /// Obtain frame count since `init`, according to the executor.
    pub fn frame_count(&self) -> u32 {
        QUERY_QUEUE.with(|q| q.frame.get())
    }
}

/// Async version of `EntityMut` or `EntityCommands`.
///
/// This type only works inside a `bevy_defer` future,
/// calling any function outside of a `bevy_defer` future
/// or inside a world access function (a closure with `World` as a parameter)
/// will likely panic.
///
/// If you need the functionalities defined here in sync code, see non-send resources
/// [`AsyncExecutor`] and [`QueryQueue`].
#[derive(Debug, Clone, Copy)]
pub struct AsyncEntityMut(pub(crate) Entity);

impl Display for AsyncEntityMut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let e = self.0;
        if WORLD.is_set() {
            WORLD.with(|world| {
                if let Some(name) = world.get_entity(e).ok().and_then(|e| e.get::<Name>()) {
                    write!(f, "Entity(\"{}\", {}, {})", name, e.index(), e.generation())
                } else {
                    write!(f, "Entity({}, {})", e.index(), e.generation())
                }
            })
        } else {
            write!(f, "Entity({}, {})", e.index(), e.generation())
        }
    }
}

impl Borrow<Entity> for AsyncEntityMut {
    fn borrow(&self) -> &Entity {
        &self.0
    }
}

impl Borrow<Entity> for &AsyncEntityMut {
    fn borrow(&self) -> &Entity {
        &self.0
    }
}

impl Borrow<Entity> for &&AsyncEntityMut {
    fn borrow(&self) -> &Entity {
        &self.0
    }
}

impl AsyncEntityMut {
    /// Obtain the underlying [`Entity`] id.
    pub fn id(&self) -> Entity {
        self.0
    }

    /// Obtain the underlying [`AsyncWorld`]
    pub fn world(&self) -> AsyncWorld {
        AsyncWorld
    }

    /// Get an [`struct@AsyncComponent`] on this entity.
    ///
    /// # Note
    ///
    /// This does not mean the component or the entity exists in the world.
    pub fn component<C: Component>(&self) -> AsyncComponent<C> {
        AsyncComponent {
            entity: self.0,
            p: PhantomData,
        }
    }

    /// Get an [`AsyncEntityQuery`] on this entity.
    ///
    /// # Note
    ///
    /// This does not mean the component or the entity exists in the world.
    pub fn query<T: QueryData>(&self) -> AsyncEntityQuery<T, ()> {
        AsyncEntityQuery {
            entity: self.0,
            p: PhantomData,
        }
    }

    /// Get an [`AsyncEntityQuery`] on this entity.
    ///
    /// # Note
    ///
    /// This does not mean the component or the entity exists in the world.
    pub fn query_filtered<T: QueryData, F: QueryFilter>(&self) -> AsyncEntityQuery<T, F> {
        AsyncEntityQuery {
            entity: self.0,
            p: PhantomData,
        }
    }
}

/// [`AsyncEntityParam`] on an indexed child.
#[derive(Debug, Clone, RefCast)]
#[repr(transparent)]
pub struct AsyncChild<const N: usize = 0>(AsyncEntityMut);

impl Deref for AsyncChild {
    type Target = AsyncEntityMut;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
