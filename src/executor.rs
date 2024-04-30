use std::rc::Rc;
use std::ops::Deref;
use async_executor::{LocalExecutor, Task};
use bevy_asset::AssetServer;
use bevy_ecs::world::World;
use std::future::Future;
use ref_cast::RefCast;
use crate::access::AsyncWorldMut;
use crate::queue::AsyncQueryQueue;
use crate::reactors::{ArcReactors, Reactors};

scoped_tls_hkt::scoped_thread_local!(static mut WORLD: World);

pub(crate) const USED_OUTSIDE: &str = "Should not be called outside of a `bevy_defer` future or inside a access function.";

pub(crate) fn with_world_ref<T>(f: impl FnOnce(&World) -> T) -> T{
    if !WORLD.is_set() { panic!("{}", USED_OUTSIDE) }
    WORLD.with(|w| f(w))
}

pub(crate) fn with_world_mut<T>(f: impl FnOnce(&mut World) -> T) -> T{
    if !WORLD.is_set() { panic!("{}", USED_OUTSIDE) }
    WORLD.with(f)
}

scoped_tls_hkt::scoped_thread_local!(pub(crate) static ASSET_SERVER: AssetServer);
scoped_tls_hkt::scoped_thread_local!(pub(crate) static ASYNC_WORLD: AsyncWorldMut);
scoped_tls_hkt::scoped_thread_local!(pub(crate) static SPAWNER: LocalExecutor<'static>);
scoped_tls_hkt::scoped_thread_local!(pub(crate) static REACTORS: Reactors);

pub(crate) fn world_scope<T>(executor: &Rc<AsyncQueryQueue>, pool: &LocalExecutor<'static>, f: impl FnOnce() -> T) -> T{
    ASYNC_WORLD.set(AsyncWorldMut::ref_cast(executor), ||{
        SPAWNER.set(pool, f)
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
    SPAWNER.with(|s| s.spawn(fut).detach() );
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

/// [`NonSend`] resource containing a reference to an async executor, 
/// this resource can be cloned to spawn futures.
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

/// A `!Send` Queue for deferred queries applied on the `World`.
#[derive(Debug, Default, Clone)]
pub struct QueryQueue(pub(crate) Rc<AsyncQueryQueue>);

impl Deref for QueryQueue {
    type Target = AsyncQueryQueue;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

/// System for running [`AsyncExecutor`].
pub fn run_async_executor (
    world: &mut World
) {
    let reactors = world.resource::<ArcReactors>().clone();
    let queue = world.non_send_resource::<QueryQueue>().clone();
    let executor = world.non_send_resource::<AsyncExecutor>().clone();
    let assets = world.get_resource::<AssetServer>().cloned();

    let mut f = || SPAWNER.set(&executor.0.clone(), || {
        ASYNC_WORLD.set(AsyncWorldMut::ref_cast(&queue.0), || { 
            REACTORS.set(&reactors, || {
                WORLD.set(world, || {
                    while executor.0.try_tick() {}
                    while executor.0.try_tick() {}
                });
            })
        })
    });

    if let Some(assets) = assets {
        ASSET_SERVER.set(&assets, f)
    } else {
        f()
    }

    
}
