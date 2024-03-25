#![doc=include_str!("../README.md")]
#![allow(clippy::type_complexity)]
use std::{borrow::Borrow, marker::PhantomData, pin::Pin, sync::Arc};

use bevy_utils::intern::Interned;
use bevy_app::{App, First, FixedUpdate, Plugin, PostUpdate, PreUpdate, Update};
mod async_world;
mod async_entity;
mod async_values;
pub mod async_systems;
mod async_query;
mod event;
pub mod signals;
mod object;
mod executor;
mod commands;
mod extension;
mod locals;
pub mod tween;
pub mod channels;
pub mod ui;
use bevy_ecs::{schedule::{IntoSystemConfigs, ScheduleLabel, SystemSet}, system::{Command, Commands}, world::World};
use bevy_log::error;
use bevy_reflect::std_traits::ReflectDefault;
pub use executor::{AsyncFailure, AsyncExecutor, QueryQueue, QueryCallback};
use executor::AsyncQueryQueue;

pub use crate::async_world::{world, in_async_context, spawn, spawn_scoped};
use crate::async_world::world_scope;

pub use locals::{with_sync_world, with_asset_server};

pub mod access {
    //! Asynchronous accessors to the `World`.
    pub use crate::async_world::{AsyncWorld, AsyncWorldMut, AsyncEntityMut};
    pub use crate::async_query::{AsyncQuery, AsyncEntityQuery};
    pub use crate::async_values::{AsyncComponent, AsyncResource, AsyncNonSend, AsyncSystemParam};
}

pub mod extensions {
    //! Traits for adding extension methods on asynchronous accessors to the `World`.
    pub use crate::async_values::{AsyncComponentDeref, AsyncResourceDeref, AsyncNonSendDeref};
    pub use crate::async_query::{AsyncQueryDeref, AsyncEntityQueryDeref};
}

pub mod systems {
    //! Systems in `bevy_defer`.
    pub use crate::executor::{run_async_executor, run_async_queries, push_async_systems};
    pub use crate::tween::run_fixed_queue;
}

use futures::{task::LocalSpawnExt, Future};
pub use object::{Object, AsObject};
pub use crate::channels::channel;
pub use crate::locals::LocalResourceScope;

pub(crate) static CHANNEL_CLOSED: &str = "channel closed unexpectedly";

#[doc(hidden)]
pub use bevy_ecs::entity::Entity;
#[doc(hidden)]
pub use bevy_ecs::system::{NonSend, Res, SystemParam};
#[doc(hidden)]
pub use scoped_tls::scoped_thread_local;

use signals::{NamedSignals, SignalData, SignalId, Signals};
use tween::{run_fixed_queue, FixedQueue};

/// Result type of `AsyncSystemFunction`.
pub type AsyncResult<T = ()> = Result<T, AsyncFailure>;

#[derive(Debug, Default, Clone, Copy)]

/// The core `bevy_defer` plugin that does not run its executors.
/// 
/// You should almost always use [`AsyncPlugin::empty`] or [`AsyncPlugin::default_settings`] instead.
pub struct CoreAsyncPlugin;

impl Plugin for CoreAsyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<QueryQueue>()
            .init_non_send_resource::<AsyncExecutor>()
            .init_non_send_resource::<FixedQueue>()
            .register_type::<async_systems::AsyncSystems>()
            .register_type_data::<async_systems::AsyncSystems, ReflectDefault>()
            .register_type::<Signals>()
            .register_type_data::<Signals, ReflectDefault>()
            .add_systems(First, systems::push_async_systems)
            .add_systems(FixedUpdate, run_fixed_queue);
    }
}

/// An `bevy_defer` plugin that can run the executor through user configuration.
#[derive(Debug)]
pub struct AsyncPlugin<S: LocalResourceScope=()> {
    schedules: Vec<(Interned<dyn ScheduleLabel>, Option<Interned<dyn SystemSet>>)>,
    p: PhantomData<S>
}

impl AsyncPlugin {
    /// Equivalent to `CoreAsyncPlugin`.
    pub fn empty() -> Self {
        AsyncPlugin { schedules: Vec::new(), p: PhantomData }
    }

    /// Run on `PreUpdate`, `Update` and `PostUpdate`.
    pub fn default_settings() -> Self {
        AsyncPlugin { schedules: vec![
            (Interned(Box::leak(Box::new(PreUpdate))), None),
            (Interned(Box::leak(Box::new(Update))), None),
            (Interned(Box::leak(Box::new(PostUpdate))), None),
        ], p: PhantomData }
    }
}

impl<S: LocalResourceScope> AsyncPlugin<S> {

    /// Push a `Resource` or `NonSend` to thread local storage.
    pub fn with<A: LocalResourceScope>(self) -> AsyncPlugin<(S, A)> {
        AsyncPlugin { schedules: self.schedules, p: PhantomData}
    }

    /// Push `&World` to thread local storage, this blocks all write access
    /// during execution but allows `get` to resolve immediately.
    pub fn with_world_access<A: LocalResourceScope>(self) -> AsyncPlugin<(S, World)> {
        AsyncPlugin { schedules: self.schedules, p: PhantomData}
    }

    /// Run the executor in a specific `Schedule`.
    pub fn run_in<A: LocalResourceScope>(mut self, schedule: impl ScheduleLabel) -> Self {
        self.schedules.push((Interned(Box::leak(Box::new(schedule))), None));
        self
    }

    /// Run the executor in a specific `Schedule` and `SystemSet`.
    pub fn run_in_set<A: LocalResourceScope>(mut self, schedule: impl ScheduleLabel, set: impl SystemSet) -> Self {
        self.schedules.push(
            (
                Interned(Box::leak(Box::new(schedule))), 
                Some(Interned(Box::leak(Box::new(set)))),
            )
        );
        self
    }
}

/// Safety: Safe since S is a marker.
unsafe impl<S: LocalResourceScope> Send for AsyncPlugin<S> {}
/// Safety: Safe since S is a marker.
unsafe impl<S: LocalResourceScope> Sync for AsyncPlugin<S> {}

impl<S: LocalResourceScope> Plugin for AsyncPlugin<S> {
    fn build(&self, app: &mut App) {
        use crate::systems::*;
        app.add_plugins(CoreAsyncPlugin);
        for (schedule, set) in &self.schedules {
            if let Some(set) = set {
                app.add_systems(schedule.clone(), 
                    (run_async_queries.before(run_async_executor::<S>), run_async_executor::<S>).in_set(set.clone())
                );
            } else {
                app.add_systems(schedule.clone(), 
                    (run_async_queries.before(run_async_executor::<S>), run_async_executor::<S>)
                );
            }
        }
    }   
}

/// Extension for [`World`] and [`App`].
pub trait AsyncExtension {
    /// Spawn a task to be run on the [`AsyncExecutor`].
    fn spawn_task(&mut self, f: impl Future<Output = AsyncResult> + 'static) -> &mut Self;

    /// Obtain a named signal.
    fn signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>>;
}

impl AsyncExtension for World {
    fn spawn_task(&mut self, f: impl Future<Output = AsyncResult>  + 'static) -> &mut Self {
        let _ = self.non_send_resource::<AsyncExecutor>().0.spawner().spawn_local(async move {
            match f.await {
                Ok(()) => (),
                Err(err) => error!("Async Failure: {err}.")
            }
        });
        self
    }

    fn signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>> {
        self.get_resource_or_insert_with::<NamedSignals<T>>(Default::default).get(name)
    }
}

impl AsyncExtension for App {
    fn spawn_task(&mut self, f: impl Future<Output = AsyncResult> + 'static) -> &mut Self {
        let _ = self.world.non_send_resource::<AsyncExecutor>().0.spawner().spawn_local(async move {
            match f.await {
                Ok(()) => (),
                Err(err) => error!("Async Failure: {err}.")
            }
        });
        self
    }

    fn signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Arc<SignalData<T::Data>> {
        self.world.get_resource_or_insert_with::<NamedSignals<T>>(Default::default).get(name)
    }
}

/// Extension for [`Commands`].
pub trait AsyncCommandsExtension {
    /// Spawn a task to be run on the [`AsyncExecutor`].
    fn spawn_task<F: Future<Output = AsyncResult> + 'static>(&mut self, f: impl FnOnce() -> F + Send + 'static) -> &mut Self;
}
impl AsyncCommandsExtension for Commands<'_, '_> {
    fn spawn_task<F: Future<Output = AsyncResult> + 'static>(&mut self, f: impl (FnOnce() -> F) + Send + 'static) -> &mut Self {
        self.add(SpawnFn::new(f));
        self
    }

}

/// [`Command`] for spawning a task.
pub struct SpawnFn(Box<dyn (FnOnce() -> Pin<Box<dyn Future<Output = AsyncResult>>>) + Send + 'static>);

impl SpawnFn {
    fn new<F: Future<Output = AsyncResult> + 'static>(f: impl (FnOnce() -> F) + Send + 'static) -> Self{
        Self(Box::new( move || Box::pin(f())))
    }
}

impl Command for SpawnFn {
    fn apply(self, world: &mut World) {
        world.spawn_task(self.0());
    }
}

