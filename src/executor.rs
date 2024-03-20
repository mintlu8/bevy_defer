use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::{mem, ops::Deref};
use bevy_ecs::system::{NonSend, NonSendMut};
use bevy_ecs::{entity::Entity, system::Query, world::World};
use bevy_log::trace;
use bevy_tasks::{ComputeTaskPool, ParallelSliceMut};
use futures::executor::LocalPool;
use futures::task::LocalSpawnExt;
use futures::{Future, FutureExt};
use parking_lot::Mutex;
use triomphe::Arc;
use crate::channels::Sender;
use crate::{world_scope, AsyncSystems};
use crate::signals::{Signals, DUMMY_SIGNALS};

/// Standard errors for the async runtime.
/// 
/// This type is designed to be match friendly but not necessarily carry all the debugging information.
/// It might me more correct to either match or unwrap this error instead of propagating it.
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
    #[error("event not registered")]
    EventNotRegistered,
    #[error("signal not found")]
    SignalNotFound,
    #[error("schedule not found")]
    ScheduleNotFound,
    #[error("system param error")]
    SystemParamError,
}

/// A shared storage that cleans up associated futures
/// when their associated entity is destroyed.
#[derive(Debug, Default)]
pub(crate) struct ParentAlive(Arc<Mutex<Option<Waker>>>);

impl ParentAlive {
    pub fn new() -> Self {
        ParentAlive::default()
    }
    pub fn other_alive(&self) -> bool {
        Arc::count(&self.0) > 1
    }
    pub fn clone_child(&self) -> ChildAlive {
        ChildAlive {
            inner: self.0.clone(),
            init: false,
        }
    }
}

impl Drop for ParentAlive {
    fn drop(&mut self) {
        if let Some(waker) = self.0.lock().take() {
            waker.wake()
        }
    }
}


/// A shared storage that cleans up associated futures
/// when their associated entity is destroyed.
#[derive(Debug, Default)]
pub(crate) struct ChildAlive{
    inner: Arc<Mutex<Option<Waker>>>,
    init: bool,
}

impl Future for ChildAlive {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if Arc::count(&self.inner) <= 1 {
            Poll::Ready(())
        } else if !self.init {
            self.init = true;
            *self.inner.lock() = Some(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Pending
        }
    }
}

impl Drop for ChildAlive {
    fn drop(&mut self) {
        *self.inner.lock() = None
    }
}


/// A deferred parallelizable query on a `World`.
pub struct ParallelQueryCallback {
    command: Option<Box<dyn FnOnce(&World) + Send + 'static>>
}

impl ParallelQueryCallback {
    pub fn new<Out: Send + Sync + 'static>(
        query: impl (FnOnce(&World) -> Out) + Send + 'static,
        channel: futures::channel::oneshot::Sender<Out>
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
pub struct QueryCallback {
    command: Box<dyn FnOnce(&mut World) -> Option<QueryCallback> + 'static>
}

impl QueryCallback {
    pub fn fire_and_forget(
        query: impl (FnOnce(&mut World)) + 'static,
    ) -> Self {
        Self {
            command: Box::new(move |w| {
                query(w);
                None
            })
        }
    }

    pub fn once<Out: 'static>(
        query: impl (FnOnce(&mut World) -> Out) + 'static,
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

    pub fn repeat<Out: 'static>(
        mut query: impl (FnMut(&mut World) -> Option<Out>) + 'static,
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
                        Some(QueryCallback::repeat(query, channel))
                    }
                }

            })
        }
    }
}


/// Queue for deferred queries applied on the [`World`].
#[derive(Default)]
pub struct AsyncQueryQueue {
    pub readonly: RefCell<Vec<ParallelQueryCallback>>,
    pub queries: RefCell<Vec<QueryCallback>>,
}

/// Resource containing a reference to an async executor.
#[derive(Debug, Default)]
pub struct AsyncExecutor(pub(crate) LocalPool);

impl std::fmt::Debug for AsyncQueryQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncExecutor")
            .field("readonly", &self.readonly.borrow().len())
            .field("queries", &self.queries.borrow().len())
            .finish()
    }
}

/// Queue for deferred queries applied on the [`World`].
#[derive(Default)]
pub struct QueryQueue(pub(crate) Rc<AsyncQueryQueue>);

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
    let executor = w.non_send_resource::<QueryQueue>().0.clone();
    // always dropped
    let mut readonly: Vec<_> = {
        let mut lock = executor.readonly.borrow_mut();
        mem::take(lock.as_mut())
    };

    if !readonly.is_empty() {
        let pool = ComputeTaskPool::get();
        readonly.par_splat_map_mut(pool, None, |chunks| for item in chunks {
            if let Some(f) = item.command.take() { f(w) }
        });
    }
    
    {
        let mut lock = executor.queries.borrow_mut();
        let inner: Vec<_> = mem::take(lock.as_mut());
        *lock = inner.into_iter().filter_map(|query| (query.command)(w)).collect();
    }
}

/// Run [`AsyncExecutor`]
pub fn run_async_executor(
    queue: NonSend<QueryQueue>,
    mut executor: NonSendMut<AsyncExecutor>
) {
    world_scope(&queue.0, executor.0.spawner(), || {
        executor.0.run_until_stalled();
    })
}

/// Push inactive [`AsyncSystems`] to the executor.
pub(crate) fn push_async_systems(
    executor: NonSend<QueryQueue>,
    exec: NonSend<AsyncExecutor>,
    mut query: Query<(Entity, Option<&Signals>, &mut AsyncSystems)>
) {
    let dummy = DUMMY_SIGNALS.deref();
    let spawner = exec.0.spawner();
    for (entity, signals, mut systems) in query.iter_mut() {
        let signals = signals.unwrap_or(dummy);
        for system in systems.systems.iter_mut(){
            if !system.marker.other_alive() {
                let alive = system.marker.clone_child();

                let Some(fut) = (system.function)(entity, &executor.0, signals) else {continue};
                let _ = spawner.spawn_local(async move {
                    futures::select_biased! {
                        _ = alive.fuse() => (),
                        _ = fut.fuse() => (),
                    };
                });
            }
        }
    }
}
