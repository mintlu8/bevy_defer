use std::{borrow::Cow, marker::PhantomData, ops::Deref, future::Future};
use triomphe::Arc;
use bevy_ecs::{entity::Entity, query::{QueryData, QueryFilter}, system::{Res, Resource, SystemParam}};
use crate::{AsyncEntityQuery, AsyncQuery, AsyncResource, AsyncSystemParams, ResAsyncExecutor};
use ref_cast::RefCast;
use super::AsyncExecutor;

/// [`SystemParam`] for obtaining an [`AsyncWorldMut`].
#[derive(SystemParam)]
pub struct AsyncWorld<'w, 's> {
    executor: Res<'w, ResAsyncExecutor>,
    p: PhantomData<&'s ()>
}

impl Deref for AsyncWorld<'_, '_> {
    type Target = AsyncWorldMut;

    fn deref(&self) -> &Self::Target {
        AsyncWorldMut::ref_cast(&self.executor.as_ref().0)
    }
}

scoped_tls::scoped_thread_local!(static ASYNC_WORLD: AsyncWorldMut);

pub(crate) fn world_scope<T>(executor: &Arc<AsyncExecutor>, f: impl FnOnce() -> T) -> T{
    ASYNC_WORLD.set(AsyncWorldMut::ref_cast(executor), f)
}

/// Spawn a `bevy_defer` compatible future.
/// 
/// # Handle
/// 
/// The handle can be used to obtain the result,
/// if dropped, the associated future will be dropped by the executor.
///
/// Can only be used inside a `bevy_defer` future.
pub fn spawn<T: Send + Sync + 'static>(future: impl Future<Output = T> + Send + 'static) -> impl Future<Output = T> {
    if !ASYNC_WORLD.is_set() {
        panic!("bevy_defer::spawn can only be used in a bevy_defer future.")
    }
    ASYNC_WORLD.with(|w| w.spawn_task(future))
}

/// Obtain the [`AsyncWorldMut`] of the currently running `bevy_defer` executor.
///
/// Can only be used inside a `bevy_defer` future.
pub fn world<T: Send + Sync + 'static>() -> AsyncWorldMut {
    if !ASYNC_WORLD.is_set() {
        panic!("bevy_defer::world can only be used in a bevy_defer future.")
    }
    ASYNC_WORLD.with(|w| AsyncWorldMut{ executor: w.executor.clone() })
}



#[derive(Debug, RefCast)]
#[repr(transparent)]
pub struct AsyncWorldMut {
    pub(crate) executor: Arc<AsyncExecutor>,
}

impl AsyncWorldMut {
    pub fn entity(&self, entity: Entity) -> AsyncEntityMut {
        AsyncEntityMut { 
            entity, 
            executor: Cow::Borrowed(&self.executor)
        }
    }

    pub fn resource<R: Resource>(&self) -> AsyncResource<R> {
        AsyncResource { 
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData
        }

    }

    pub fn query<Q: QueryData>(&self) -> AsyncQuery<Q> {
        AsyncQuery { 
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData
        }
    }

    pub fn query_filtered<Q: QueryData, F: QueryFilter>(&self) -> AsyncQuery<Q, F> {
        AsyncQuery { 
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData
        }
    }

    pub fn system<P: SystemParam>(&self) -> AsyncSystemParams<P> {
        AsyncSystemParams { 
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData
        }
    }
}

/// A fast readonly query for multiple components.
pub struct AsyncEntityMut<'t> {
    pub(crate) entity: Entity,
    pub(crate) executor: Cow<'t, Arc<AsyncExecutor>>,
}

impl Deref for AsyncEntityMut<'_> {
    type Target = AsyncWorldMut;

    fn deref(&self) -> &Self::Target {
        AsyncWorldMut::ref_cast(self.executor.as_ref())
    }
}

impl AsyncEntityMut<'_> {
    pub fn world(&self) -> &AsyncWorldMut {
        AsyncWorldMut::ref_cast(self.executor.as_ref())
    }

    pub fn query<T: QueryData>(&self) -> AsyncEntityQuery<'_, T, ()> {
        AsyncEntityQuery {
            entity: self.entity,
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData,
        }
    }

    pub fn query_filtered<T: QueryData, F: QueryFilter>(&self) -> AsyncEntityQuery<'_, T, F> {
        AsyncEntityQuery {
            entity: self.entity,
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData,
        }
    }
}