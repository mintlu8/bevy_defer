use std::{borrow::Cow, future::Future, marker::PhantomData, ops::Deref, rc::Rc};
use bevy_log::error;
use crate::channels::channel;
use futures::executor::LocalSpawner;
use futures::task::LocalSpawnExt;
use futures::FutureExt;
use bevy_ecs::{component::Component, entity::Entity, query::{QueryData, QueryFilter}, system::{NonSend, Resource, SystemParam}};
use crate::{AsyncComponent, AsyncEntityParam, AsyncEntityQuery, AsyncExecutor, AsyncQuery, AsyncResource, AsyncResult, AsyncSystemParam, QueryQueue};
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
        let _ = self.executor.0.spawner().spawn_local(async move {
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

/// Spawn a `bevy_defer` compatible future.
/// 
/// # Handle
/// 
/// The handle can be used to obtain the result,
/// if dropped, the associated future will be dropped by the executor.
///
/// Can only be used inside a `bevy_defer` future.
pub fn spawn<T: 'static>(fut: impl Future<Output = T> + 'static) -> impl Future<Output = T> {
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
pub fn spawn_and_forget<T: Send + 'static>(fut: impl Future<Output = T> + Send + 'static) {
    if !SPAWNER.is_set() {
        panic!("bevy_defer::spawn_and_forget can only be used in a bevy_defer future.")
    }
    let _ = SPAWNER.with(|s| s.spawn_local(async {let _ = fut.await; }));
}

/// Obtain the [`AsyncWorldMut`] of the currently running `bevy_defer` executor.
///
/// Can only be used inside a `bevy_defer` future.
pub fn world() -> AsyncWorldMut {
    if !ASYNC_WORLD.is_set() {
        panic!("bevy_defer::world can only be used in a bevy_defer future.")
    }
    ASYNC_WORLD.with(|w| AsyncWorldMut{ queue: w.queue.clone() })
}

#[allow(unused)]
use bevy_ecs::{world::World, system::Commands};

/// Async version of [`World`] or [`Commands`].
#[derive(Debug, RefCast)]
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
            executor: Cow::Borrowed(&self.queue)
        }
    }

    /// Obtain an [`AsyncResource`] of the entity.
    /// 
    /// # Note
    /// 
    /// This does not mean the resource exists in the world.
    pub fn resource<R: Resource>(&self) -> AsyncResource<R> {
        AsyncResource { 
            executor: Cow::Borrowed(&self.queue),
            p: PhantomData
        }

    }

    /// Obtain an [`AsyncQuery`].
    pub fn query<Q: QueryData>(&self) -> AsyncQuery<Q> {
        AsyncQuery { 
            executor: Cow::Borrowed(&self.queue),
            p: PhantomData
        }
    }

    /// Obtain an [`AsyncQuery`].
    pub fn query_filtered<Q: QueryData, F: QueryFilter>(&self) -> AsyncQuery<Q, F> {
        AsyncQuery { 
            executor: Cow::Borrowed(&self.queue),
            p: PhantomData
        }
    }

    /// Obtain an [`AsyncSystemParam`].
    pub fn system<P: SystemParam>(&self) -> AsyncSystemParam<P> {
        AsyncSystemParam { 
            executor: Cow::Borrowed(&self.queue),
            p: PhantomData
        }
    }
}

/// Async version of `EntityMut` or `EntityCommands`.
pub struct AsyncEntityMut<'t> {
    pub(crate) entity: Entity,
    pub(crate) executor: Cow<'t, Rc<AsyncQueryQueue>>,
}

impl Deref for AsyncEntityMut<'_> {
    type Target = AsyncWorldMut;

    fn deref(&self) -> &Self::Target {
        AsyncWorldMut::ref_cast(self.executor.as_ref())
    }
}

impl AsyncEntityMut<'_> {
    /// Obtain the underlying [`Entity`] id.
    pub fn id(&self) -> Entity {
        self.entity
    }

    /// Reborrow an [`AsyncEntityMut`] to a new lifetime.
    pub fn reborrow(&self) -> AsyncEntityMut {
        AsyncEntityMut {
            entity: self.entity,
            executor: Cow::Borrowed(&self.executor),
        }
    }

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
        executor: &'t Rc<AsyncQueryQueue>,
        _: Self::Signal,
    ) -> Self {
        AsyncWorldMut{
            queue: executor.clone()
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
        executor: &'t Rc<AsyncQueryQueue>,
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
        executor: &'t Rc<AsyncQueryQueue>,
        _: Self::Signal,
    ) -> Self {
        AsyncEntityMut{
            entity,
            executor: Cow::Borrowed(executor)
        }
    }
}
