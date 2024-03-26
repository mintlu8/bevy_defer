use std::cell::RefCell;
use std::rc::Rc;
use std::{mem, ops::Deref};
use bevy_asset::AssetServer;
use bevy_ecs::system::{NonSend, Res, StaticSystemParam};
use bevy_ecs::world::World;
use bevy_log::debug;
use futures::executor::{LocalPool, LocalSpawner};
use crate::channels::Sender;
use crate::{world_scope, LocalResourceScope};

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
    #[error("child index missing")]
    ChildNotFound,
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
                    debug!("Error: one-shot channel closed.")
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
                            debug!("Error: one-shot channel closed.")
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
    pub queries: RefCell<Vec<QueryCallback>>,
}

/// Resource containing a reference to an async executor.
#[derive(Debug, Default)]
pub struct AsyncExecutor(pub(crate) RefCell<LocalPool>);

impl AsyncExecutor {
    pub fn spawner(&self) -> LocalSpawner {
        self.0.borrow().spawner()
    }
}

impl std::fmt::Debug for AsyncQueryQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncExecutor")
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

/// System that tries to resolve queries sent to the queue.
pub fn run_async_queries(
    w: &mut World,
) {
    let executor = w.non_send_resource::<QueryQueue>().0.clone();
    let mut lock = executor.queries.borrow_mut();    
    let inner: Vec<_> = mem::take(lock.as_mut());
    *lock = inner.into_iter().filter_map(|query| (query.command)(w)).collect();
}

/// System for running [`AsyncExecutor`].
pub fn run_async_executor<R: LocalResourceScope>(
    queue: NonSend<QueryQueue>,
    scoped: StaticSystemParam<R::Resource>,
    // Since nobody needs mutable access to `AssetServer` this is enabled by default.
    asset_server: Option<Res<AssetServer>>,
    executor: NonSend<AsyncExecutor>
) {
    AssetServer::maybe_scoped(asset_server.as_ref(), ||{
        R::scoped(&*scoped, ||world_scope(&queue.0, executor.spawner(), || {
            executor.0.borrow_mut().run_until_stalled();
        }))
    })
    
}
