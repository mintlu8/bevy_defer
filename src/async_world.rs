use std::{borrow::Cow, future::Future, marker::PhantomData, ops::Deref};
use futures::channel::oneshot::channel;
use futures::executor::LocalSpawner;
use futures::task::LocalSpawnExt;
use futures::FutureExt;
use triomphe::Arc;
use bevy_ecs::{component::Component, entity::Entity, query::{QueryData, QueryFilter}, system::{Res, Resource, SystemParam}};
use crate::{AsyncComponent, AsyncEntityParam, AsyncEntityQuery, AsyncQuery, AsyncResource, AsyncSystemParam, QueryQueue};
use ref_cast::RefCast;
use super::AsyncQueryQueue;

/// [`SystemParam`] for obtaining an [`AsyncWorldMut`].
#[derive(SystemParam)]
pub struct AsyncWorld<'w, 's> {
    executor: Res<'w, QueryQueue>,
    p: PhantomData<&'s ()>
}

impl Deref for AsyncWorld<'_, '_> {
    type Target = AsyncWorldMut;

    fn deref(&self) -> &Self::Target {
        AsyncWorldMut::ref_cast(&self.executor.as_ref().0)
    }
}

scoped_tls::scoped_thread_local!(static ASYNC_WORLD: AsyncWorldMut);
scoped_tls::scoped_thread_local!(static SPAWNER: LocalSpawner);

pub(crate) fn world_scope<T>(executor: &Arc<AsyncQueryQueue>, pool: LocalSpawner, f: impl FnOnce() -> T) -> T{
    ASYNC_WORLD.set(AsyncWorldMut::ref_cast(executor), ||{
        SPAWNER.set(&pool, f)
    })
}

/// Spawn a `bevy_defer` compatible future.
/// 
/// # Handle
/// 
/// The handle can be used to obtain the result,
/// if dropped, the associated future will be dropped by the executor.
///
/// Can only be used inside a `bevy_defer` future.
pub fn spawn<T: Send + Sync + 'static>(fut: impl Future<Output = T> + Send + 'static) -> impl Future<Output = T> {
    if !SPAWNER.is_set() {
        panic!("bevy_defer::spawn can only be used in a bevy_defer future.")
    }
    let (mut send, recv) = channel();
    let _ = SPAWNER.with(|s| s.spawn_local(
        async move {
            futures::select_biased!(
                _ = send.cancellation().fuse() => (), 
                out = fut.fuse() => {
                    let _ = send.send(out);
                },
            )
        }
    ));
    recv.map(|x| x.unwrap())
}

/// Obtain the [`AsyncWorldMut`] of the currently running `bevy_defer` executor.
///
/// Can only be used inside a `bevy_defer` future.
pub fn world() -> AsyncWorldMut {
    if !ASYNC_WORLD.is_set() {
        panic!("bevy_defer::world can only be used in a bevy_defer future.")
    }
    ASYNC_WORLD.with(|w| AsyncWorldMut{ executor: w.executor.clone() })
}

#[allow(unused)]
use bevy_ecs::{world::World, system::Commands};

/// Async version of [`World`] or [`Commands`].
#[derive(Debug, RefCast)]
#[repr(transparent)]
pub struct AsyncWorldMut {
    pub(crate) executor: Arc<AsyncQueryQueue>,
}

impl AsyncWorldMut {
    /// Obtain an [`AsyncEntityMut`] of the entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the entity exists in the world.
    pub fn entity(&self, entity: Entity) -> AsyncEntityMut {
        AsyncEntityMut { 
            entity, 
            executor: Cow::Borrowed(&self.executor)
        }
    }

    /// Obtain an [`AsyncResource`] of the entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the resource exists in the world.
    pub fn resource<R: Resource>(&self) -> AsyncResource<R> {
        AsyncResource { 
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData
        }

    }

    /// Obtain an [`AsyncQuery`].
    pub fn query<Q: QueryData>(&self) -> AsyncQuery<Q> {
        AsyncQuery { 
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData
        }
    }

    /// Obtain an [`AsyncQuery`].
    pub fn query_filtered<Q: QueryData, F: QueryFilter>(&self) -> AsyncQuery<Q, F> {
        AsyncQuery { 
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData
        }
    }

    /// Obtain an [`AsyncSystemParam`].
    pub fn system<P: SystemParam>(&self) -> AsyncSystemParam<P> {
        AsyncSystemParam { 
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData
        }
    }
}

/// Async version of `EntityMut` or `EntityCommands`.
pub struct AsyncEntityMut<'t> {
    pub(crate) entity: Entity,
    pub(crate) executor: Cow<'t, Arc<AsyncQueryQueue>>,
}

impl Deref for AsyncEntityMut<'_> {
    type Target = AsyncWorldMut;

    fn deref(&self) -> &Self::Target {
        AsyncWorldMut::ref_cast(self.executor.as_ref())
    }
}

impl AsyncEntityMut<'_> {
    /// Obtain the underlying [`AsyncWorldMut`]
    pub fn world(&self) -> &AsyncWorldMut {
        AsyncWorldMut::ref_cast(self.executor.as_ref())
    }

    /// Get an [`AsyncComponent`] on this entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the component or the entity exists in the world.
    pub fn component<C: Component>(&self) -> AsyncComponent<C> {
        AsyncComponent {
            entity: self.entity,
            executor: Cow::Borrowed(self.executor.as_ref()),
            p: PhantomData,
        }
    }

    /// Get an [`AsyncEntityQuery`] on this entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the component or the entity exists in the world.
    pub fn query<T: QueryData>(&self) -> AsyncEntityQuery<'_, T, ()> {
        AsyncEntityQuery {
            entity: self.entity,
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData,
        }
    }

    /// Get an [`AsyncEntityQuery`] on this entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the component or the entity exists in the world.
    pub fn query_filtered<T: QueryData, F: QueryFilter>(&self) -> AsyncEntityQuery<'_, T, F> {
        AsyncEntityQuery {
            entity: self.entity,
            executor: Cow::Borrowed(&self.executor),
            p: PhantomData,
        }
    }
}

impl<'t> AsyncEntityParam<'t> for AsyncWorldMut {
    type Signal = ();

    fn fetch_signal(_: &crate::signals::Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &'t Arc<AsyncQueryQueue>,
        _: Self::Signal,
    ) -> Self {
        AsyncWorldMut{
            executor: executor.clone()
        }
    }
}

impl<'t> AsyncEntityParam<'t> for &'t AsyncWorldMut {
    type Signal = ();

    fn fetch_signal(_: &crate::signals::Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &'t Arc<AsyncQueryQueue>,
        _: Self::Signal,
    ) -> Self {
        AsyncWorldMut::ref_cast(executor)
    }
}

impl<'t> AsyncEntityParam<'t> for AsyncEntityMut<'t> {
    type Signal = ();

    fn fetch_signal(_: &crate::signals::Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        entity: Entity,
        executor: &'t Arc<AsyncQueryQueue>,
        _: Self::Signal,
    ) -> Self {
        AsyncEntityMut{
            entity,
            executor: Cow::Borrowed(executor)
        }
    }
}