use std::cell::RefCell;
use std::rc::Rc;
use std::ops::Deref;
use bevy_asset::AssetServer;
use bevy_ecs::system::{CommandQueue, Commands, NonSend, Res, StaticSystemParam};
use futures::executor::{LocalPool, LocalSpawner};
use crate::queue::AsyncQueryQueue;
use crate::signals::NamedSignals;
use crate::{world_scope, LocalResourceScope};

/// Resource containing a reference to an async executor.
#[derive(Debug, Default)]
pub struct AsyncExecutor(pub(crate) RefCell<LocalPool>);

impl AsyncExecutor {
    pub fn spawner(&self) -> LocalSpawner {
        self.0.borrow().spawner()
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
