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
mod executor;
mod commands;
mod accessors;
mod ext;
mod locals;
mod queue;
pub mod reactors;
pub mod cancellation;
pub mod tween;
pub mod channels;
pub mod picking;
use bevy_ecs::{schedule::{IntoSystemConfigs, ScheduleLabel, SystemSet}, system::{Command, Commands}, world::World};
use bevy_log::error;
use bevy_reflect::std_traits::ReflectDefault;
pub use executor::{AsyncExecutor, QueryQueue};
use queue::AsyncQueryQueue;
use reactors::Reactors;
pub use accessors::{AsyncAccess, Captures};

pub use crate::executor::{world, in_async_context, spawn, spawn_scoped};

pub mod access {
    //! Asynchronous accessors to the `World`.
    pub use crate::async_world::{AsyncWorld, AsyncWorldMut, AsyncEntityMut, AsyncChild};
    pub use crate::async_query::{AsyncQuery, AsyncEntityQuery};
    pub use crate::async_values::{AsyncComponent, AsyncResource, AsyncNonSend, AsyncSystemParam};
    pub use crate::async_event::EventStream;
    pub use crate::async_asset::AsyncAsset;
    pub use crate::ext::AsyncScene;
}

pub mod extensions {
    //! Traits for adding extension methods on asynchronous accessors to the `World` through `deref`.
    pub use crate::async_values::{AsyncComponentDeref, AsyncResourceDeref, AsyncNonSendDeref, AsyncSystemParamDeref};
    pub use crate::async_query::{AsyncQueryDeref, AsyncEntityQueryDeref};
    pub use crate::async_asset::AsyncAssetDeref;
}

pub mod systems {
    //! Systems in `bevy_defer`.
    pub use crate::executor::run_async_executor;
    pub use crate::async_systems::push_async_systems;
    pub use crate::queue::{run_async_queries, run_fixed_queue, run_time_series};
    pub use crate::async_event::react_to_event;
    pub use crate::reactors::react_to_state;
}

use std::future::Future;
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

use signals::{Signal, SignalId, Signals};
use queue::run_fixed_queue;

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
            .init_resource::<Reactors>()
            .register_type::<async_systems::AsyncSystems>()
            .register_type_data::<async_systems::AsyncSystems, ReflectDefault>()
            .register_type::<Signals>()
            .register_type_data::<Signals, ReflectDefault>()
            .add_systems(First, systems::run_time_series)
            .add_systems(First, systems::push_async_systems)
            .add_systems(FixedUpdate, run_fixed_queue);
    }
}

/// An `bevy_defer` plugin that can run the executor through user configuration.
/// 
/// This plugin is not unique, if you need different locals in different schedules,
/// add multiple of this components.
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
        if !app.is_plugin_added::<CoreAsyncPlugin>() {
            app.add_plugins(CoreAsyncPlugin);
        }
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

    fn is_unique(&self) -> bool {
        false
    }
}

/// Extension for [`World`] and [`App`].
pub trait AsyncExtension {
    /// Spawn a task to be run on the [`AsyncExecutor`].
    fn spawn_task(&mut self, f: impl Future<Output = AsyncResult> + 'static) -> &mut Self;

    /// Obtain a named signal.
    fn typed_signal<T: SignalId>(&mut self) -> Signal<T::Data>;

    /// Obtain a named signal.
    fn named_signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Signal<T::Data>;
}

impl AsyncExtension for World {
    fn spawn_task(&mut self, f: impl Future<Output = AsyncResult>  + 'static) -> &mut Self {
        let _ = self.non_send_resource::<AsyncExecutor>().spawn(async move {
            match f.await {
                Ok(()) => (),
                Err(err) => error!("Async Failure: {err}.")
            }
        });
        self
    }

    fn typed_signal<T: SignalId>(&mut self) -> Signal<T::Data> {
        self.get_resource_or_insert_with::<Reactors>(Default::default).get_typed::<T>()
    }
    
    fn named_signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Signal<T::Data> {
        self.get_resource_or_insert_with::<Reactors>(Default::default).get_named::<T>(name)
    }
}

impl AsyncExtension for App {
    fn spawn_task(&mut self, f: impl Future<Output = AsyncResult> + 'static) -> &mut Self {
        let _ = self.world.non_send_resource::<AsyncExecutor>().spawn(async move {
            match f.await {
                Ok(()) => (),
                Err(err) => error!("Async Failure: {err}.")
            }
        });
        self
    }

    fn typed_signal<T: SignalId>(&mut self) -> Signal<T::Data> {
        self.world.get_resource_or_insert_with::<Reactors>(Default::default).get_typed::<T>()
    }

    fn named_signal<T: SignalId>(&mut self, name: impl Borrow<str> + Into<String>) -> Signal<T::Data> {
        self.world.get_resource_or_insert_with::<Reactors>(Default::default).get_named::<T>(name)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AsyncFailure {
    #[error("async channel closed")]
    ChannelClosed,
    #[error("entity not found")]
    EntityNotFound,
    #[error("too many entities")]
    TooManyEntities,
    #[error("child index missing")]
    ChildNotFound,
    #[error("component not found")]
    ComponentNotFound,
    #[error("resource not found")]
    ResourceNotFound,
    #[error("asset not found")]
    AssetNotFound,
    #[error("event not registered")]
    EventNotRegistered,
    #[error("signal not found")]
    SignalNotFound,
    #[error("schedule not found")]
    ScheduleNotFound,
    #[error("system param error")]
    SystemParamError,
    #[error("AsyncWorldParam not found")]
    WorldParamNotFound,
    #[error("SystemId not found")]
    SystemIdNotFound,
    /// Return `Err(ManuallyKilled)` to terminate a `system_future!` future.
    #[error("manually killed a `system_future!` future")]
    ManuallyKilled,
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
            #[derive(Debug, Clone, Copy, Component, Resource, Event, Asset, TypePath)]
            pub struct Int(i32);
    
            #[derive(Debug, Clone, Copy, Component, Resource, Event, Asset, TypePath)]
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

