#![doc=include_str!("../README.md")]
#![allow(clippy::type_complexity)]
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
pub use executor::*;
pub use async_world::*;
pub use async_systems::*;
pub use async_values::*;
pub use async_query::*;
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