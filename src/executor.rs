use std::{mem, ops::Deref, pin::Pin, task::Context};
use std::future::Future;
use bevy_ecs::system::ResMut;
use bevy_ecs::{entity::Entity, system::{Query, Res, Resource}, world::World};
use bevy_log::trace;
use bevy_tasks::{ComputeTaskPool, ParallelSliceMut};
use async_oneshot::Sender;
use parking_lot::Mutex;
use triomphe::Arc;
use crate::{world_scope, AsyncSystems, Signals, DUMMY_SIGNALS};

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
}

/// A shared storage that cleans up associated futures
/// when their associated entity is destroyed.
#[derive(Debug, Clone, Default)]
pub struct KeepAlive(Arc<()>);

impl KeepAlive {
    pub fn new() -> Self {
        KeepAlive::default()
    }
    pub fn other_alive(&self) -> bool {
        Arc::count(&self.0) > 1
    }
}


/// A future representing a running async system.
pub struct SystemFuture{
    pub(crate) future: Pin<Box<dyn Future<Output = Result<(), AsyncFailure>> + Send + 'static>>,
    pub(crate) alive: KeepAlive,
}

/// A parallelizable query on a `World`.
pub struct BoxedReadonlyCallback {
    command: Option<Box<dyn FnOnce(&World) + Send + 'static>>
}

impl BoxedReadonlyCallback {
    pub fn new<Out: Send + Sync + 'static>(
        query: impl (FnOnce(&World) -> Out) + Send + 'static,
        mut channel: Sender<Out>
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

/// A boxed function that return its data through a channel.
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
        mut channel: Sender<Out>
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
        mut channel: Sender<Out>
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

/// A simple async executor for `bevy_rectray`.
#[derive(Default)]
pub struct AsyncExecutor {
    pub stream: Mutex<Vec<SystemFuture>>,
    pub spawn_queue: Mutex<Vec<SystemFuture>>,
    pub readonly: Mutex<Vec<BoxedReadonlyCallback>>,
    pub queries: Mutex<Vec<BoxedQueryCallback>>,
}

impl std::fmt::Debug for AsyncExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncExecutor")
            .field("stream", &self.stream.lock().len())
            .field("readonly", &self.readonly.lock().len())
            .field("queries", &self.queries.lock().len())
            .finish()
    }
}

/// Resource containing a reference to an async executor.
#[derive(Default, Resource)]
pub struct ResAsyncExecutor(pub(crate) Arc<AsyncExecutor>);

impl Deref for ResAsyncExecutor {
    type Target = AsyncExecutor;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[doc(hidden)]
pub fn run_async_queries(
    w: &mut World,
) {
    let executor = w.resource::<ResAsyncExecutor>().0.clone();
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



#[doc(hidden)]
pub fn exec_async_executor(
    executor: Res<ResAsyncExecutor>,
    main_loop: Option<ResMut<AsyncMainLoop>>,
) {
    world_scope(&executor.0, || {
        let waker = noop_waker::noop_waker();
        let mut ctx = Context::from_waker(&waker);
        {
            let mut queue = executor.spawn_queue.lock();
            let mut stream = executor.stream.lock();
            stream.extend(mem::take::<Vec<SystemFuture>>(queue.as_mut()));
        }
        let mut lock = executor.stream.lock();
        lock.retain_mut(|fut| {
            if !fut.alive.other_alive() {return false;}
            match fut.future.as_mut().poll(&mut ctx) {
                std::task::Poll::Ready(Ok(_)) => false,
                std::task::Poll::Ready(Err(fail)) => {
                    trace!("Future dropped: {fail}.");
                    false
                },
                std::task::Poll::Pending => true,
            }
        });
        if let Some(mut main_loop) = main_loop {
            let _ = main_loop.0.as_mut().poll(&mut ctx);
        }
    })
}

#[doc(hidden)]
pub fn push_async_systems(
    executor: Res<ResAsyncExecutor>,
    query: Query<(Entity, Option<&Signals>, &AsyncSystems)>
) {
    let mut stream = executor.stream.lock();
    let dummy = DUMMY_SIGNALS.deref();
    for (entity, signals, systems) in query.iter() {
        let signals = signals.unwrap_or(dummy);
        for system in systems.systems.iter(){
            if !system.marker.other_alive() {
                let fut = SystemFuture{
                    future: (system.function)(entity, &executor.0, signals),
                    alive: system.marker.clone()
                };
                stream.push(fut)
            }
        }
    }
}

#[derive(Resource)]
pub struct AsyncMainLoop(Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>);