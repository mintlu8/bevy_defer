#![doc=include_str!("../README.md")]
#![allow(clippy::type_complexity)]
use std::pin::Pin;

use bevy_app::{App, Plugin, PreUpdate, Update, PostUpdate, First};
mod async_world;
mod async_entity;
mod async_values;
mod async_systems;
mod async_query;
pub mod signals;
mod signal_inner;
mod object;
mod executor;
mod commands;
use bevy_ecs::{system::{Command, Commands}, world::World};
pub use executor::*;
pub use async_world::*;
pub use async_systems::*;
pub use async_values::*;
pub use async_query::*;
use futures::{task::SpawnExt, Future};
pub use object::{Object, AsObject};
pub use futures::channel::oneshot::channel;

pub(crate) static CHANNEL_CLOSED: &str = "channel closed unexpectedly";

#[doc(hidden)]
pub use bevy_ecs::entity::Entity;

#[doc(hidden)]
pub use triomphe::Arc;

/// Result type of `AsyncSystemFunction`.
pub type AsyncResult<T = ()> = Result<T, AsyncFailure>;

#[derive(Debug, Default, Clone, Copy)]
/// An `bevy_defer` plugin does not run its executor by default.
/// 
/// Add [`run_async_executor!`] to your schedules to run the executor as you like.
pub struct CoreAsyncPlugin;

impl Plugin for CoreAsyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QueryQueue>()
            .init_non_send_resource::<AsyncExecutor>()
            .add_systems(First, push_async_systems);
    }
}

/// `SystemConfigs` for running the async executor once.
/// 
/// # Example
/// 
/// ```
/// app.add_systems(Update, run_async_executor!()
///     .after(some_other_system))
/// ```
#[macro_export]
macro_rules! run_async_executor {
    () => {
        bevy_ecs::system::IntoSystem::pipe(
            $crate::executor::run_async_queries,
            $crate::executor::exec_async_executor,
        )
    };
}

/// An `bevy_defer` plugin that runs [`AsyncExecutor`] on [`PreUpdate`], [`Update`] and [`PostUpdate`].
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultAsyncPlugin;

impl Plugin for DefaultAsyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QueryQueue>()
            .init_non_send_resource::<AsyncExecutor>()
            .add_systems(First, push_async_systems)
            .add_systems(PreUpdate, run_async_executor!())
            .add_systems(Update, run_async_executor!())
            .add_systems(PostUpdate, run_async_executor!())
        ;
    }
}

/// Extension for spawning tasks for [`World`], [`App`] and [`Commands`].
pub trait AsyncExtension {
    /// Spawn a task to be run on the [`AsyncExecutor`].
    fn spawn_task(&mut self, f: impl Future<Output = ()> + Send + Sync + 'static);
}

impl AsyncExtension for World {
    fn spawn_task(&mut self, f: impl Future<Output = ()> + Send + Sync + 'static) {
        let _ = self.non_send_resource::<AsyncExecutor>().0.spawner().spawn(f);
    }
}

impl AsyncExtension for App {
    fn spawn_task(&mut self, f: impl Future<Output = ()> + Send + Sync + 'static) {
        let _ = self.world.non_send_resource::<AsyncExecutor>().0.spawner().spawn(f);
    }
}

impl AsyncExtension for Commands<'_, '_> {
    fn spawn_task(&mut self, f: impl Future<Output = ()> + Send + Sync + 'static) {
        self.add(Spawn::new(f))
    }
}

/// [`Command`] for spawning a task.
pub struct Spawn(Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>);

impl Spawn {
    fn new(f: impl Future<Output = ()> + Send + Sync + 'static) -> Self{
        Spawn(Box::pin(f))
    }
}

impl Command for Spawn {
    fn apply(self, world: &mut World) {
        world.spawn_task(self.0)
    }
}