#![doc=include_str!("../README.md")]
#![allow(clippy::type_complexity)]
use std::{borrow::Borrow, marker::PhantomData, pin::Pin};

use bevy_utils::intern::Interned;
use bevy_app::{App, First, FixedUpdate, Plugin, PostUpdate, PreUpdate, Update};
mod async_world;
mod async_entity;
mod async_values;
mod async_asset;
pub mod async_systems;
mod async_query;
mod async_event;
pub mod signals;
//mod object;
mod executor;
mod commands;
mod extension;
mod locals;
mod fixed_queue;
pub use fixed_queue::FixedQueue;
pub mod cancellation;
pub mod tween;
pub mod channels;
pub mod picking;
use bevy_ecs::{schedule::{IntoSystemConfigs, ScheduleLabel, SystemSet}, system::{Command, Commands}, world::World};
use bevy_log::error;
use bevy_reflect::std_traits::ReflectDefault;
pub use executor::{AsyncExecutor, QueryQueue};
use executor::AsyncQueryQueue;

pub use crate::async_world::{world, in_async_context, spawn, spawn_scoped};
use crate::async_world::world_scope;

pub mod access {
    //! Asynchronous accessors to the `World`.
    pub use crate::async_world::{AsyncWorld, AsyncWorldMut, AsyncEntityMut};
    pub use crate::async_query::{AsyncQuery, AsyncEntityQuery};
    pub use crate::async_values::{AsyncComponent, AsyncResource, AsyncNonSend, AsyncSystemParam};
    pub use crate::async_event::AsyncEventReader;
    pub use crate::async_asset::AsyncAsset;
    pub use crate::extension::AsyncScene;
}

pub mod extensions {
    //! Traits for adding extension methods on asynchronous accessors to the `World` through `deref`.
    pub use crate::async_values::{AsyncComponentDeref, AsyncResourceDeref, AsyncNonSendDeref, AsyncSystemParamDeref};
    pub use crate::async_query::{AsyncQueryDeref, AsyncEntityQueryDeref};
    pub use crate::async_event::AsyncEventReaderDeref;
    pub use crate::async_asset::AsyncAssetDeref;
}

pub mod systems {
    //! Systems in `bevy_defer`.
    pub use crate::executor::{run_async_executor, run_async_queries};
    pub use crate::async_systems::push_async_systems;
    pub use crate::fixed_queue::run_fixed_queue;
}

use futures::{task::LocalSpawnExt, Future};
//pub use object::{Object, AsObject};
pub use crate::channels::channel;
pub use crate::locals::LocalResourceScope;

pub(crate) static CHANNEL_CLOSED: &str = "channel closed unexpectedly";

#[doc(hidden)]
pub use bevy_ecs::entity::Entity;
#[doc(hidden)]
pub use bevy_ecs::system::{NonSend, Res, SystemParam};
#[doc(hidden)]
pub use scoped_tls::scoped_thread_local;

use signals::{NamedSignals, Signal, SignalId, Signals};
use fixed_queue::run_fixed_queue;

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
            .init_resource::<NamedSignals>()
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
    pub fn with_world_access(self) -> AsyncPlugin<(S, World)> {
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
                app.add_systems(*schedule, 
                    (run_async_queries.before(run_async_executor::<S>), run_async_executor::<S>).in_set(*set)
                );
            } else {
                app.add_systems(*schedule, 
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
    fn signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Signal<T::Data>;
}

impl AsyncExtension for World {
    fn spawn_task(&mut self, f: impl Future<Output = AsyncResult>  + 'static) -> &mut Self {
        let _ = self.non_send_resource::<AsyncExecutor>().spawner().spawn_local(async move {
            match f.await {
                Ok(()) => (),
                Err(err) => error!("Async Failure: {err}.")
            }
        });
        self
    }

    fn signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Signal<T::Data> {
        self.get_resource_or_insert_with::<NamedSignals>(Default::default).get::<T>(name)
    }
}

impl AsyncExtension for App {
    fn spawn_task(&mut self, f: impl Future<Output = AsyncResult> + 'static) -> &mut Self {
        let _ = self.world.non_send_resource::<AsyncExecutor>().spawner().spawn_local(async move {
            match f.await {
                Ok(()) => (),
                Err(err) => error!("Async Failure: {err}.")
            }
        });
        self
    }

    fn signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Signal<T::Data> {
        self.world.get_resource_or_insert_with::<NamedSignals>(Default::default).get::<T>(name)
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

#[doc(hidden)]
#[allow(unused)]
#[macro_export]
macro_rules! test_spawn {
    ($expr: expr) => {
        {
            use ::bevy::prelude::*;
            use ::bevy_defer::*;
            use ::bevy_defer::access::*;
            #[derive(Debug, Component, Resource, Event, Asset, TypePath)]
            pub struct Int(i32);
    
            #[derive(Debug, Component, Resource, Event, Asset, TypePath)]
            pub struct Str(&'static str);

            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, States)]
            pub enum MyState{
                A, B, C
            };
    
            let mut app = ::bevy::app::App::new();
            app.add_plugins(MinimalPlugins);
            app.add_plugins(AssetPlugin::default());
            app.init_asset::<Image>();
            app.add_plugins(bevy_defer::AsyncPlugin::default_settings());
            app.world.spawn(Int(4));
            app.world.spawn(Str("Ferris"));
            app.insert_resource(Int(4));
            app.insert_resource(Str("Ferris"));
            app.insert_non_send_resource(Int(4));
            app.insert_non_send_resource(Str("Ferris"));
            app.insert_state(MyState::A);
            app.spawn_task(async move {
                $expr;
                world().quit().await;
                Ok(())
            });
            app.run();
        }
    };
}
