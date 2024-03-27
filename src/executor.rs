use std::cell::RefCell;
use std::rc::Rc;
use std::{mem, ops::Deref};
use bevy_asset::AssetServer;
use bevy_ecs::system::{CommandQueue, Commands, NonSend, Res, StaticSystemParam};
use bevy_ecs::world::World;
use futures::executor::{LocalPool, LocalSpawner};
use crate::channels::Sender;
use crate::signals::NamedSignals;
use crate::{world_scope, LocalResourceScope};

/// A deferred query on a `World`.
struct QueryCallback {
    command: Box<dyn FnOnce(&mut World) -> Option<QueryCallback> + 'static>
}

impl QueryCallback {
    fn fire_and_forget(
        query: impl (FnOnce(&mut World)) + 'static,
    ) -> Self {
        Self {
            command: Box::new(move |w| {
                query(w);
                None
            })
        }
    }

    fn once<Out: 'static>(
        query: impl (FnOnce(&mut World) -> Out) + 'static,
        channel: Sender<Out>
    ) -> Self {
        Self {
            command: Box::new(move |w| {
                if channel.is_closed() { return None } 
                let result = query(w);
                let _ = channel.send(result);
                None
            })
        }
    }

    fn repeat<Out: 'static>(
        mut query: impl (FnMut(&mut World) -> Option<Out>) + 'static,
        channel: Sender<Out>
    ) -> Self {
        Self {
            command: Box::new(move |w| {
                if channel.is_closed() { return None } 
                match query(w) {
                    Some(result) => {
                        let _ = channel.send(result);
                        None
                    },
                    None => Some(QueryCallback::repeat(query, channel))
                }
            })
        }
    }
}


/// Queue for deferred `!Send` queries applied on the [`World`].
#[derive(Default)]
pub struct AsyncQueryQueue {
    queries: RefCell<Vec<QueryCallback>>,
}

impl AsyncQueryQueue {
    
    /// Spawn a `!Send` command that runs once.
    /// 
    /// Use `AsyncWorldMut::add_command` if possible since the bevy `CommandQueue` is more optimized.
    pub fn fire_and_forget(
        &self,
        query: impl (FnOnce(&mut World)) + 'static,
    ) {
        self.queries.borrow_mut().push(
            QueryCallback::fire_and_forget(query)
        )
    }


    /// Spawn a `!Send` command that runs once and returns a result through a channel.
    /// 
    /// If receiver is dropped, the command will be cancelled.
    pub fn once<Out: 'static>(
        &self,
        query: impl (FnOnce(&mut World) -> Out) + 'static,
        channel: Sender<Out>
    ) {
        self.queries.borrow_mut().push(
            QueryCallback::once(query, channel)
        )
    }


    /// Spawn a `!Send` command and wait until it returns `Some`.
    /// 
    /// If receiver is dropped, the command will be cancelled.
    pub fn repeat<Out: 'static> (
        &self,
        query: impl (FnMut(&mut World) -> Option<Out>) + 'static,
        channel: Sender<Out>
    ) {
        self.queries.borrow_mut().push(
            QueryCallback::repeat(query, channel)
        )
    }
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
            .finish_non_exhaustive()
    }
}

/// Queue for deferred queries applied on the [`World`].
#[derive(Debug, Default)]
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

scoped_tls::scoped_thread_local! {pub(crate) static COMMAND_QUEUE: RefCell<CommandQueue>}

/// System for running [`AsyncExecutor`].
pub fn run_async_executor<R: LocalResourceScope>(
    mut commands: Commands,
    queue: NonSend<QueryQueue>,
    scoped: StaticSystemParam<R::Resource>,
    // Since nobody needs mutable access to `AssetServer` this is enabled by default.
    asset_server: Option<Res<AssetServer>>,
    named_signal: Res<NamedSignals>,
    executor: NonSend<AsyncExecutor>
) {
    let mut cmd_queue = RefCell::new(CommandQueue::default());
    COMMAND_QUEUE.set(&cmd_queue, || {
        AssetServer::maybe_scoped(asset_server.as_ref(), ||{
            NamedSignals::scoped(&named_signal, || {
                R::scoped(&*scoped, ||world_scope(&queue.0, executor.spawner(), || {
                    executor.0.borrow_mut().run_until_stalled();
                }))
            })
        })
    });
    commands.append(cmd_queue.get_mut())
}
