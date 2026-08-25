use crate::queue::QueryQueue;
use crate::reactors::Reactors;
use async_executor::{LocalExecutor, Task};
use bevy::ecs::world::World;
use bevy::log::error;
use std::fmt::Display;
use std::future::Future;
use std::rc::Rc;

scoped_tls_hkt::scoped_thread_local!(pub(crate) static mut WORLD: World);
scoped_tls_hkt::scoped_thread_local!(pub(crate) static WORLD_READ_ONLY: World);

pub(crate) const USED_OUTSIDE: &str =
    "Should not be called outside of a `bevy_defer` future or inside a access function.";

#[track_caller]
pub(crate) fn with_world_ref<T>(f: impl FnOnce(&World) -> T) -> T {
    if WORLD_READ_ONLY.is_set() {
        WORLD_READ_ONLY.with(f)
    } else if WORLD.is_set() {
        WORLD.with(|w| WORLD_READ_ONLY.set(w, || f(w)))
    } else {
        panic!("{}", USED_OUTSIDE)
    }
}

#[track_caller]
pub(crate) fn with_world_mut<T>(f: impl FnOnce(&mut World) -> T) -> T {
    if !WORLD.is_set() {
        panic!("{}", USED_OUTSIDE)
    }
    WORLD.with(f)
}

#[cfg(feature = "bevy_asset")]
scoped_tls_hkt::scoped_thread_local!(pub(crate) static ASSET_SERVER: ::bevy::asset::AssetServer);
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
pub fn run_async_executor<E: WorldExtract>(world: &mut World) {
    #[cfg(feature = "bevy_asset")]
    type AssetServer = ::bevy::asset::AssetServer;
    #[cfg(not(feature = "bevy_asset"))]
    type AssetServer = ();

    type Extract<E> = (AsyncExecutor, (Reactors, (QueryQueue, (AssetServer, E))));
    let executor = world.non_send::<AsyncExecutor>().clone();

    Extract::<E>::extract_from_world(world, |world| {
        WORLD.set(world, || while executor.0.try_tick() {})
    })
}

pub trait WorldExtract: 'static {
    fn extract_from_world(world: &mut World, f: impl FnOnce(&mut World));
}

impl WorldExtract for () {
    fn extract_from_world(world: &mut World, f: impl FnOnce(&mut World)) {
        f(world)
    }
}

impl<A: WorldExtract, B: WorldExtract> WorldExtract for (A, B) {
    fn extract_from_world(world: &mut World, f: impl FnOnce(&mut World)) {
        A::extract_from_world(world, |world| B::extract_from_world(world, f));
    }
}

impl WorldExtract for AsyncExecutor {
    fn extract_from_world(world: &mut World, f: impl FnOnce(&mut World)) {
        let executor = world.non_send::<AsyncExecutor>().clone();
        SPAWNER.set(&executor.0, || f(world))
    }
}

impl WorldExtract for QueryQueue {
    fn extract_from_world(world: &mut World, f: impl FnOnce(&mut World)) {
        let queue = world.non_send::<QueryQueue>().clone();
        QUERY_QUEUE.set(&queue, || f(world))
    }
}

impl WorldExtract for Reactors {
    fn extract_from_world(world: &mut World, f: impl FnOnce(&mut World)) {
        let reactors = world.resource::<Reactors>().clone();
        REACTORS.set(&reactors, || f(world))
    }
}

#[cfg(feature = "bevy_asset")]
impl WorldExtract for ::bevy::asset::AssetServer {
    fn extract_from_world(world: &mut World, f: impl FnOnce(&mut World)) {
        if let Some(asset_server) = world.get_resource::<::bevy::asset::AssetServer>() {
            let asset_server = asset_server.clone();
            ASSET_SERVER.set(&asset_server, || f(world))
        } else {
            f(world)
        }
    }
}

#[cfg(test)]
mod test {
    use bevy::ecs::{resource::Resource, world::World};

    use crate::WorldExtract;

    #[derive(Resource)]
    pub struct R;

    scoped_tls_hkt::scoped_thread_local!(static mut STATIC: R);

    impl WorldExtract for R {
        fn extract_from_world(world: &mut World, f: impl FnOnce(&mut World)) {
            world.resource_scope::<R, _>(|world, res| STATIC.set(res.into_inner(), || f(world)))
        }
    }
}
