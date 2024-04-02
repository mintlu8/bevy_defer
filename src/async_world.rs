use std::usize;
use std::{future::Future, marker::PhantomData, ops::Deref, rc::Rc};
use bevy_log::error;
use crate::async_systems::AsyncWorldParam;
use crate::channels::channel;
use crate::access::{AsyncComponent, AsyncNonSend, AsyncEntityQuery, AsyncQuery, AsyncResource, AsyncSystemParam};
use futures::executor::LocalSpawner;
use futures::task::LocalSpawnExt;
use futures::FutureExt;
use bevy_ecs::{component::Component, entity::Entity, query::{QueryData, QueryFilter}, system::{NonSend, Resource, SystemParam}};
use crate::{async_systems::AsyncEntityParam, AsyncExecutor, AsyncResult, QueryQueue};
use ref_cast::RefCast;
use super::AsyncQueryQueue;

/// [`SystemParam`] for obtaining [`AsyncWorldMut`] and spawning futures.
/// 
/// Note this `SystemParam` is [`NonSend`] and can only execute on the main thread.
#[derive(SystemParam)]
pub struct AsyncWorld<'w, 's> {
    queue: NonSend<'w, QueryQueue>,
    executor: NonSend<'w, AsyncExecutor>,
    p: PhantomData<&'s ()>
}

impl AsyncWorld<'_, '_> {
    pub fn spawn(&self, fut: impl Future<Output = AsyncResult> + 'static) {
        let _ = self.executor.spawner().spawn_local(async move {
            match fut.await {
                Ok(()) => (),
                Err(err) => error!("Async Failure: {err}.")
            }
        });
    }
}

impl Deref for AsyncWorld<'_, '_> {
    type Target = AsyncWorldMut;

    fn deref(&self) -> &Self::Target {
        AsyncWorldMut::ref_cast(&self.queue.0)
    }
}

scoped_tls::scoped_thread_local!(static ASYNC_WORLD: AsyncWorldMut);
scoped_tls::scoped_thread_local!(static SPAWNER: LocalSpawner);

pub(crate) fn world_scope<T>(executor: &Rc<AsyncQueryQueue>, pool: LocalSpawner, f: impl FnOnce() -> T) -> T{
    ASYNC_WORLD.set(AsyncWorldMut::ref_cast(executor), ||{
        SPAWNER.set(&pool, f)
    })
}

/// Spawn a `bevy_defer` compatible future with a handle.
/// 
/// # Handle
/// 
/// The handle can be used to obtain the result,
/// if dropped, the associated future will be dropped by the executor.
/// 
/// # Panics
///
/// If used outside a `bevy_defer` future.
pub fn spawn_scoped<T: 'static>(fut: impl Future<Output = T> + 'static) -> impl Future<Output = T> {
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

/// Spawn a `bevy_defer` compatible future.
/// 
/// The spawned future will not be dropped until finished.
/// 
/// # Panics
///
/// If used outside a `bevy_defer` future.
pub fn spawn<T: 'static>(fut: impl Future<Output = T> + 'static) {
    if !SPAWNER.is_set() {
        panic!("bevy_defer::spawn_and_forget can only be used in a bevy_defer future.")
    }
    let _ = SPAWNER.with(|s| s.spawn_local(async {let _ = fut.await; }));
}

/// Obtain the [`AsyncWorldMut`] of the currently running `bevy_defer` executor.
///
/// # Panics
///
/// If used outside a `bevy_defer` future.
pub fn world() -> AsyncWorldMut {
    if !ASYNC_WORLD.is_set() {
        panic!("bevy_defer::world can only be used in a bevy_defer future.")
    }
    ASYNC_WORLD.with(|w| AsyncWorldMut{ queue: w.queue.clone() })
}

/// Returns `true` if in async context, for diagnostics purpose only.
pub fn in_async_context() -> bool {
    ASYNC_WORLD.is_set()
}

#[allow(unused)]
use bevy_ecs::{world::World, system::Commands};

/// Async version of [`World`] or [`Commands`].
#[derive(Debug, RefCast, Clone)]
#[repr(transparent)]
pub struct AsyncWorldMut {
    pub(crate) queue: Rc<AsyncQueryQueue>,
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
            queue: self.queue.clone()
        }
    }

    /// Obtain an [`AsyncResource`] of the entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the resource exists in the world.
    pub fn resource<R: Resource>(&self) -> AsyncResource<R> {
        AsyncResource { 
            queue: self.queue.clone(),
            p: PhantomData
        }
    }


    /// Obtain an [`AsyncNonSend`] of the entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the resource exists in the world.
    pub fn non_send_resource<R: 'static>(&self) -> AsyncNonSend<R> {
        AsyncNonSend { 
            queue: self.queue.clone(),
            p: PhantomData
        }
    }

    /// Obtain an [`AsyncQuery`].
    pub fn query<Q: QueryData>(&self) -> AsyncQuery<Q> {
        AsyncQuery { 
            queue: self.queue.clone(),
            p: PhantomData
        }
    }

    /// Obtain an [`AsyncQuery`].
    pub fn query_filtered<Q: QueryData, F: QueryFilter>(&self) -> AsyncQuery<Q, F> {
        AsyncQuery { 
            queue: self.queue.clone(),
            p: PhantomData
        }
    }

    /// Obtain an [`AsyncSystemParam`].
    pub fn system<P: SystemParam>(&self) -> AsyncSystemParam<P> {
        AsyncSystemParam { 
            queue: self.queue.clone(),
            p: PhantomData
        }
    }
}

#[derive(Debug, Clone)]
/// Async version of `EntityMut` or `EntityCommands`.
pub struct AsyncEntityMut {
    pub(crate) entity: Entity,
    pub(crate) queue: Rc<AsyncQueryQueue>,
}

impl Deref for AsyncEntityMut {
    type Target = AsyncWorldMut;

    fn deref(&self) -> &Self::Target {
        AsyncWorldMut::ref_cast(&self.queue)
    }
}

impl AsyncEntityMut {
    /// Obtain the underlying [`Entity`] id.
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Obtain the underlying [`AsyncWorldMut`]
    pub fn world(&self) -> AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue).clone()
    }

    /// Get an [`AsyncComponent`] on this entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the component or the entity exists in the world.
    pub fn component<C: Component>(&self) -> AsyncComponent<C> {
        AsyncComponent {
            entity: self.entity,
            queue: self.queue.clone(),
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
            entity: self.entity,
            queue: self.queue.clone(),
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
            entity: self.entity,
            queue: self.queue.clone(),
            p: PhantomData,
        }
    }
}

impl AsyncWorldParam for AsyncWorldMut {
    fn from_async_context(
        executor: &AsyncWorldMut,
    ) -> Option<Self> {
        Some(AsyncWorldMut{
            queue: executor.queue.clone()
        })
    }
}

impl AsyncEntityParam for AsyncEntityMut {
    type Signal = ();

    fn fetch_signal(_: &crate::signals::Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        entity: Entity,
        executor: &AsyncWorldMut,
        _: Self::Signal,
        _: &[Entity],
    ) -> Option<Self> {
        Some(AsyncEntityMut{
            entity,
            queue: executor.queue.clone()
        })
    }
}



/// [`AsyncEntityParam`] on an indexed child.
#[derive(Debug, Clone, RefCast)]
#[repr(transparent)]
pub struct AsyncChild<const N: usize=0>(AsyncEntityMut);

impl Deref for AsyncChild {
    type Target = AsyncEntityMut;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> AsyncEntityParam for AsyncChild<N> {
    type Signal = ();

    fn fetch_signal(_: &crate::signals::Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &AsyncWorldMut,
        _: Self::Signal,
        children: &[Entity],
    ) -> Option<Self> {
        Some(AsyncChild(AsyncEntityMut{
            entity: children.get(N).copied()?,
            queue: executor.queue.clone()
        }))
    }
}
