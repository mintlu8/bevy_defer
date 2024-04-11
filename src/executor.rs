use std::cell::RefCell;
use std::rc::Rc;
use std::ops::Deref;
use async_executor::{LocalExecutor, Task};
use bevy_asset::AssetServer;
use bevy_ecs::system::{CommandQueue, Commands, NonSend, Res, StaticSystemParam};
use std::future::Future;
use ref_cast::RefCast;
use crate::async_world::AsyncWorldMut;
use crate::queue::AsyncQueryQueue;
use crate::reactors::Reactors;
use crate::LocalResourceScope;

scoped_tls::scoped_thread_local!(pub(crate) static ASYNC_WORLD: AsyncWorldMut);
scoped_tls::scoped_thread_local!(pub(crate) static SPAWNER: LocalExecutor<'static>);

pub(crate) fn world_scope<T>(executor: &Rc<AsyncQueryQueue>, pool: &LocalExecutor<'static>, f: impl FnOnce() -> T) -> T{
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
        panic!("bevy_defer::spawn_scoped can only be used in a bevy_defer future.")
    }
    SPAWNER.with(|s| s.spawn(fut))
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
        panic!("bevy_defer::spawn can only be used in a bevy_defer future.")
    }
    let _ = SPAWNER.with(|s| s.spawn(fut).detach() );
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

/// Resource containing a reference to an async executor.
#[derive(Debug, Default, Clone)]
pub struct AsyncExecutor(pub(crate) Rc<async_executor::LocalExecutor<'static>>);

impl AsyncExecutor {
    pub fn spawn<T: 'static>(&self, future: impl Future<Output = T> + 'static) {
        self.0.spawn(future).detach();
    }

    pub fn spawn_scoped<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Task<T> {
        self.0.spawn(future)
    }
}

/// Queue for deferred queries applied on the `World`.
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
    state_reactors: Res<Reactors>,
    executor: NonSend<AsyncExecutor>
) {
    let mut cmd_queue = RefCell::new(CommandQueue::default());
    COMMAND_QUEUE.set(&cmd_queue, || {
        AssetServer::maybe_scoped(asset_server.as_ref(), ||{
            Reactors::scoped(&state_reactors, || {
                R::scoped(&*scoped, || world_scope(&queue.0, &executor.0, || {
                    while executor.0.try_tick() {}
                }))
            })
        })
    });
    commands.append(cmd_queue.get_mut())
}
