use crate::observer;
use crate::queue::QueryQueue;
use crate::reactors::Reactors;
use async_executor::{LocalExecutor, Task};
use bevy::asset::AssetServer;
use bevy::ecs::world::World;
use bevy::log::error;
use bevy::prelude::Mut;
use std::fmt::Display;
use std::future::Future;
use std::rc::Rc;

scoped_tls_hkt::scoped_thread_local!(pub(crate) static mut WORLD: World);

pub(crate) const USED_OUTSIDE: &str =
    "Should not be called outside of a `bevy_defer` future or inside a access function.";

pub(crate) fn with_world_ref<T>(f: impl FnOnce(&World) -> T) -> T {
    if !WORLD.is_set() {
        panic!("{}", USED_OUTSIDE)
    }
    WORLD.with(|w| f(w))
}

pub(crate) fn with_world_mut<T>(f: impl FnOnce(&mut World) -> T) -> T {
    if !WORLD.is_set() {
        panic!("{}", USED_OUTSIDE)
    }
    WORLD.with(f)
}

scoped_tls_hkt::scoped_thread_local!(pub(crate) static ASSET_SERVER: AssetServer);
scoped_tls_hkt::scoped_thread_local!(pub(crate) static QUERY_QUEUE: QueryQueue);
scoped_tls_hkt::scoped_thread_local!(pub(crate) static SPAWNER: LocalExecutor<'static>);
scoped_tls_hkt::scoped_thread_local!(pub(crate) static REACTORS: Reactors);

/// Returns `true` if in async context, for diagnostics purpose only.
pub fn in_async_context() -> bool {
    QUERY_QUEUE.is_set()
}

/// `!Send` resource containing a reference to an async executor,
/// this resource can be cloned to spawn futures.
#[derive(Debug, Default, Clone)]
pub struct AsyncExecutor(pub(crate) Rc<async_executor::LocalExecutor<'static>>);

impl AsyncExecutor {
    /// Spawns a future, does not wait for it to complete.
    pub fn spawn_any<T: 'static>(&self, future: impl Future<Output = T> + 'static) {
        self.0.spawn(future).detach();
    }

    /// Spawns a future and returns a [`Task`].
    #[deprecated = "Use `spawn_task`."]
    pub fn spawn_scoped<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Task<T> {
        self.0.spawn(future)
    }

    /// Spawns a future and returns a [`Task`].
    pub fn spawn_task<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> Task<T> {
        self.0.spawn(future)
    }

    /// Spawns a future, logs errors but does not wait for it to complete.
    pub fn spawn<T: 'static, E: Display>(
        &self,
        future: impl Future<Output = Result<T, E>> + 'static,
    ) {
        self.0
            .spawn(async {
                if let Err(e) = future.await {
                    error!("{e}")
                }
            })
            .detach();
    }
}

/// System for running [`AsyncExecutor`].
pub fn run_async_executor(world: &mut World) {
    let reactors = world.resource::<Reactors>().clone();
    let queue = world.non_send_resource::<QueryQueue>().clone();
    let executor = world.non_send_resource::<AsyncExecutor>().clone();
    let assets = world.get_resource::<AssetServer>().cloned();

    let mut f = || {
        SPAWNER.set(&executor.0.clone(), || {
            QUERY_QUEUE.set(&queue, || {
                REACTORS.set(&reactors, || {
                    WORLD.set(world, || while executor.0.try_tick() {});
                })
            })
        })
    };

    if let Some(assets) = assets {
        ASSET_SERVER.set(&assets, f)
    } else {
        f()
    }
}
