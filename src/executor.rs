use std::{mem, ops::Deref};
use bevy_ecs::system::{NonSend, NonSendMut};
use bevy_ecs::{entity::Entity, system::{Query, Res, Resource}, world::World};
use bevy_log::trace;
use bevy_tasks::{ComputeTaskPool, ParallelSliceMut};
use futures::channel::oneshot::Sender;
use futures::executor::LocalPool;
use futures::task::LocalSpawnExt;
use parking_lot::Mutex;
use triomphe::Arc;
use crate::{world_scope, AsyncSystems};
use crate::signals::{Signals, DUMMY_SIGNALS};

/// Standard errors for the async runtime.
#[derive(Debug, thiserror::Error)]
pub enum AsyncFailure {
    #[error("async channel closed")]
    ChannelClosed,
    #[error("entity not found")]
    EntityNotFound,
    #[error("entity not found in query")]
    EntityQueryNotFound,
    #[error("component not found")]
    ComponentNotFound,
    #[error("resource not found")]
    ResourceNotFound,
    #[error("signal not found")]
    SignalNotFound,
}

/// A shared storage that cleans up associated futures
/// when their associated entity is destroyed.
#[derive(Debug, Clone, Default)]
pub(crate) struct KeepAlive(Arc<()>);

impl KeepAlive {
    pub fn new() -> Self {
        KeepAlive::default()
    }
    pub fn other_alive(&self) -> bool {
        Arc::count(&self.0) > 1
    }
}

/// A deferred parallelizable query on a `World`.
pub struct BoxedReadonlyCallback {
    command: Option<Box<dyn FnOnce(&World) + Send + 'static>>
}

impl BoxedReadonlyCallback {
    pub fn new<Out: Send + Sync + 'static>(
        query: impl (FnOnce(&World) -> Out) + Send + 'static,
        channel: Sender<Out>
    ) -> Self {
        Self {
            command: Some(Box::new(move |w| {
                let result = query(w);
                if channel.send(result).is_err() {
                    trace!("Error: one-shot channel closed.")
                }
            }))
        }
    }
}

/// A deferred query on a `World`.
pub struct BoxedQueryCallback {
    command: Box<dyn FnOnce(&mut World) -> Option<BoxedQueryCallback> + Send + 'static>
}

impl BoxedQueryCallback {
    pub fn fire_and_forget(
        query: impl (FnOnce(&mut World)) + Send + 'static,
    ) -> Self {
        Self {
            command: Box::new(move |w| {
                query(w);
                None
            })
        }
    }

    pub fn once<Out: Send + Sync + 'static>(
        query: impl (FnOnce(&mut World) -> Out) + Send + 'static,
        channel: Sender<Out>
    ) -> Self {
        Self {
            command: Box::new(move |w| {
                let result = query(w);
                if channel.send(result).is_err() {
                    trace!("Error: one-shot channel closed.")
                }
                None
            })
        }
    }

    pub fn repeat<Out: Send + Sync + 'static>(
        mut query: impl (FnMut(&mut World) -> Option<Out>) + Send + 'static,
        channel: Sender<Out>
    ) -> Self {
        Self {
            command: Box::new(move |w| {
                match query(w) {
                    Some(x) => {
                        if channel.send(x).is_err() {
                            trace!("Error: one-shot channel closed.")
                        }
                        None
                    }
                    None => {
                        Some(BoxedQueryCallback::repeat(query, channel))
                    }
                }

            })
        }
    }
}

/// Queue foe deferred queries applied on the [`World`].
#[derive(Default)]
pub struct AsyncQueryQueue {
    pub readonly: Mutex<Vec<BoxedReadonlyCallback>>,
    pub queries: Mutex<Vec<BoxedQueryCallback>>,
}

/// Queue for deferred queries applied on the [`World`].
#[derive(Debug, Default)]
pub struct AsyncExecutor(pub(crate) LocalPool);

impl std::fmt::Debug for AsyncQueryQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncExecutor")
            .field("readonly", &self.readonly.lock().len())
            .field("queries", &self.queries.lock().len())
            .finish()
    }
}

/// Resource containing a reference to an async executor.
#[derive(Default, Resource)]
pub struct QueryQueue(pub(crate) Arc<AsyncQueryQueue>);

impl Deref for QueryQueue {
    type Target = AsyncQueryQueue;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

/// Try resolve queries sent to the queue.
pub fn run_async_queries(
    w: &mut World,
) {
    let executor = w.resource::<QueryQueue>().0.clone();
    // always dropped
    let mut readonly: Vec<_> = {
        let mut lock = executor.readonly.lock();
        mem::take(lock.as_mut())
    };

    if !readonly.is_empty() {
        let pool = ComputeTaskPool::get();
        readonly.par_splat_map_mut(pool, None, |chunks| for item in chunks {
            if let Some(f) = item.command.take() { f(w) }
        });
    }
    
    {
        let mut lock = executor.queries.lock();
        let inner: Vec<_> = mem::take(lock.as_mut());
        *lock = inner.into_iter().filter_map(|query| (query.command)(w)).collect();
    }
}

/// Run [`AsyncExecutor`]
pub fn run_async_executor(
    queue: Res<QueryQueue>,
    mut executor: NonSendMut<AsyncExecutor>
) {
    world_scope(&queue.0, executor.0.spawner(), || {
        executor.0.run_until_stalled();
    })
}

pub fn push_async_systems(
    executor: Res<QueryQueue>,
    exec: NonSend<AsyncExecutor>,
    mut query: Query<(Entity, Option<&Signals>, &mut AsyncSystems)>
) {
    let dummy = DUMMY_SIGNALS.deref();
    let spawner = exec.0.spawner();
    for (entity, signals, mut systems) in query.iter_mut() {
        let signals = signals.unwrap_or(dummy);
        for system in systems.systems.iter_mut(){
            if !system.marker.other_alive() {
                let Some(fut) = (system.function)(entity, &executor.0, signals) else {continue};
                let alive = system.marker.clone();
                let _ = spawner.spawn_local(async move {
                    let _ = fut.await;
                    drop(alive)
                });
            }
        }
    }
}