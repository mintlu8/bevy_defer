#![doc=include_str!("../README.md")]
#![allow(clippy::type_complexity)]
use bevy_app::{App, First, Plugin, PostUpdate, PreUpdate, Update};
use bevy_ecs::component::Component;
use bevy_ecs::event::Event;
use bevy_ecs::intern::Interned;
use bevy_ecs::world::Command;
use bevy_state::state::States;
use bevy_time::TimeSystem;
use std::pin::Pin;

pub mod access;
pub mod async_systems;
pub mod cancellation;
mod commands;
mod entity_commands;
mod errors;
mod executor;
pub mod ext;
mod queue;
pub mod reactors;
pub mod signals;
pub mod sync;
pub mod tween;
#[allow(deprecated)]
pub use crate::executor::world;
#[allow(deprecated)]
pub use crate::executor::{in_async_context, spawn, spawn_scoped};
pub use access::async_event::EventBuffer;
pub use access::async_query::OwnedQueryState;
pub use access::traits::AsyncAccess;
pub use access::AsyncWorld;
use bevy_ecs::{
    schedule::{IntoSystemConfigs, ScheduleLabel, SystemSet},
    system::Commands,
    world::World,
};
use bevy_reflect::std_traits::ReflectDefault;
pub use errors::{AccessError, CustomError, MessageError};
pub use executor::AsyncExecutor;
pub use queue::QueryQueue;
use reactors::Reactors;

pub mod systems {
    //! Systems in `bevy_defer`.
    //!
    //! Systems named `react_to_*` must be added manually.
    pub use crate::access::async_event::react_to_event;
    pub use crate::async_systems::push_async_systems;
    pub use crate::executor::run_async_executor;
    pub use crate::queue::{run_fixed_queue, run_time_series, run_watch_queries};
    pub use crate::reactors::{react_to_component_change, react_to_state};

    #[cfg(feature = "bevy_animation")]
    pub use crate::ext::anim::react_to_animation;
    #[cfg(feature = "bevy_ui")]
    pub use crate::ext::picking::react_to_ui;
    #[cfg(feature = "bevy_scene")]
    pub use crate::ext::scene::react_to_scene_load;
}

pub use crate::sync::oneshot::channel;
use std::future::Future;

pub(crate) static CHANNEL_CLOSED: &str = "channel closed unexpectedly";

#[doc(hidden)]
pub use bevy_ecs::entity::Entity;
#[doc(hidden)]
pub use bevy_ecs::system::{NonSend, Res, SystemParam};
#[doc(hidden)]
pub use bevy_log::error;
#[doc(hidden)]
pub use ref_cast::RefCast;

use queue::run_fixed_queue;
use signals::{Signal, SignalId, Signals};

#[cfg(feature = "derive")]
pub use bevy_defer_derive::async_access;

/// Deprecated access error.
#[deprecated = "Use AccessError instead."]
pub type AsyncFailure = AccessError;

/// Deprecated access result.
#[deprecated = "Use AccessResult instead."]
pub type AsyncResult<T = ()> = Result<T, AccessError>;

/// Result type of spawned tasks.
pub type AccessResult<T = ()> = Result<T, AccessError>;

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
            .init_schedule(BeforeAsyncExecutor)
            .add_systems(First, systems::run_time_series.after(TimeSystem))
            .add_systems(First, systems::push_async_systems)
            .add_systems(Update, run_fixed_queue)
            .add_systems(BeforeAsyncExecutor, systems::run_watch_queries);

        #[cfg(feature = "bevy_scene")]
        app.add_systems(BeforeAsyncExecutor, systems::react_to_scene_load);
        #[cfg(feature = "bevy_ui")]
        app.add_systems(BeforeAsyncExecutor, systems::react_to_ui);
        #[cfg(feature = "bevy_animation")]
        app.add_systems(BeforeAsyncExecutor, systems::react_to_animation);
    }
}

/// A schedule that runs before [`run_async_executor`](systems::run_async_executor).
///
/// By default this runs `watch` queries and reactors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct BeforeAsyncExecutor;

/// Runs the [`BeforeAsyncExecutor`] schedule.
///
/// By default this runs `watch` queries and reactors.
pub fn run_before_async_executor(world: &mut World) {
    world.run_schedule(BeforeAsyncExecutor)
}

/// An `bevy_defer` plugin that can run the executor through user configuration.
///
/// This plugin is not unique and can be used repeatedly to add runs.
#[derive(Debug)]
pub struct AsyncPlugin {
    schedules: Vec<(Interned<dyn ScheduleLabel>, Option<Interned<dyn SystemSet>>)>,
}

impl AsyncPlugin {
    /// Equivalent to [`CoreAsyncPlugin`].
    ///
    /// Use [`AsyncPlugin::run_in`] and [`AsyncPlugin::run_in_set`] to add runs.
    pub fn empty() -> Self {
        AsyncPlugin {
            schedules: Vec::new(),
        }
    }

    /// Run in [`Update`] once.
    ///
    /// This is usually enough, be sure to order your
    /// systems against [`run_async_executor`](systems::run_async_executor) correctly if needed.
    pub fn default_settings() -> Self {
        AsyncPlugin {
            schedules: vec![(Interned(Box::leak(Box::new(Update))), None)],
        }
    }

    /// Run in [`PreUpdate`], [`Update`] and [`PostUpdate`].
    pub fn busy_schedule() -> Self {
        AsyncPlugin {
            schedules: vec![
                (Interned(Box::leak(Box::new(PreUpdate))), None),
                (Interned(Box::leak(Box::new(Update))), None),
                (Interned(Box::leak(Box::new(PostUpdate))), None),
            ],
        }
    }
}

impl AsyncPlugin {
    /// Run the executor in a specific `Schedule`.
    pub fn run_in(mut self, schedule: impl ScheduleLabel) -> Self {
        self.schedules
            .push((Interned(Box::leak(Box::new(schedule))), None));
        self
    }

    /// Run the executor in a specific `Schedule` and `SystemSet`.
    pub fn run_in_set(mut self, schedule: impl ScheduleLabel, set: impl SystemSet) -> Self {
        self.schedules.push((
            Interned(Box::leak(Box::new(schedule))),
            Some(Interned(Box::leak(Box::new(set)))),
        ));
        self
    }
}

impl Plugin for AsyncPlugin {
    fn build(&self, app: &mut App) {
        use crate::systems::*;
        if !app.is_plugin_added::<CoreAsyncPlugin>() {
            app.add_plugins(CoreAsyncPlugin);
        }
        for (schedule, set) in &self.schedules {
            if let Some(set) = set {
                app.add_systems(
                    *schedule,
                    (
                        run_before_async_executor.before(run_async_executor),
                        run_async_executor,
                    )
                        .in_set(*set),
                );
            } else {
                app.add_systems(
                    *schedule,
                    (
                        run_before_async_executor.before(run_async_executor),
                        run_async_executor,
                    ),
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
    fn spawn_task(&mut self, f: impl Future<Output = AccessResult> + 'static) -> &mut Self;

    /// Obtain a named signal.
    fn typed_signal<T: SignalId>(&mut self) -> Signal<T::Data>;

    /// Obtain a named signal.
    fn named_signal<T: SignalId>(&mut self, name: &str) -> Signal<T::Data>;
}

impl AsyncExtension for World {
    fn spawn_task(&mut self, f: impl Future<Output = AccessResult> + 'static) -> &mut Self {
        self.non_send_resource::<AsyncExecutor>().spawn(async move {
            match f.await {
                Ok(()) => (),
                Err(err) => error!("Async Failure: {err}."),
            }
        });
        self
    }

    fn typed_signal<T: SignalId>(&mut self) -> Signal<T::Data> {
        self.get_resource_or_insert_with::<Reactors>(Default::default)
            .get_typed::<T>()
    }

    fn named_signal<T: SignalId>(&mut self, name: &str) -> Signal<T::Data> {
        self.get_resource_or_insert_with::<Reactors>(Default::default)
            .get_named::<T>(name)
    }
}

impl AsyncExtension for App {
    fn spawn_task(&mut self, f: impl Future<Output = AccessResult> + 'static) -> &mut Self {
        self.world()
            .non_send_resource::<AsyncExecutor>()
            .spawn(async move {
                match f.await {
                    Ok(()) => (),
                    Err(err) => error!("Async Failure: {err}."),
                }
            });
        self
    }

    fn typed_signal<T: SignalId>(&mut self) -> Signal<T::Data> {
        self.world_mut()
            .get_resource_or_insert_with::<Reactors>(Default::default)
            .get_typed::<T>()
    }

    fn named_signal<T: SignalId>(&mut self, name: &str) -> Signal<T::Data> {
        self.world_mut()
            .get_resource_or_insert_with::<Reactors>(Default::default)
            .get_named::<T>(name)
    }
}

/// Extension for [`App`] to add reactors.
pub trait AppReactorExtension {
    /// React to changes in a [`Event`].
    fn react_to_event<E: Event + Clone>(&mut self) -> &mut Self;

    /// React to changes in a [`States`].
    fn react_to_state<S: States + Default>(&mut self) -> &mut Self;

    /// React to changes in a [`Component`].
    fn react_to_component_change<C: Component + Eq + Clone + Default>(&mut self) -> &mut Self;
}

impl AppReactorExtension for App {
    fn react_to_event<E: Event + Clone>(&mut self) -> &mut Self {
        self.add_systems(BeforeAsyncExecutor, systems::react_to_event::<E>);
        self
    }

    fn react_to_state<S: States + Default>(&mut self) -> &mut Self {
        self.add_systems(BeforeAsyncExecutor, systems::react_to_state::<S>);
        self
    }

    fn react_to_component_change<C: Component + Eq + Clone + Default>(&mut self) -> &mut Self {
        self.add_systems(BeforeAsyncExecutor, systems::react_to_component_change::<C>);
        self
    }
}

/// Extension for [`Commands`].
pub trait AsyncCommandsExtension {
    /// Spawn a task to be run on the [`AsyncExecutor`].
    fn spawn_task<F: Future<Output = AccessResult> + 'static>(
        &mut self,
        f: impl FnOnce() -> F + Send + 'static,
    ) -> &mut Self;
}

impl AsyncCommandsExtension for Commands<'_, '_> {
    fn spawn_task<F: Future<Output = AccessResult> + 'static>(
        &mut self,
        f: impl (FnOnce() -> F) + Send + 'static,
    ) -> &mut Self {
        self.add(SpawnFn::new(f));
        self
    }
}

/// [`Command`] for spawning a task.
pub struct SpawnFn(
    Box<dyn (FnOnce() -> Pin<Box<dyn Future<Output = AccessResult>>>) + Send + 'static>,
);

impl SpawnFn {
    fn new<F: Future<Output = AccessResult> + 'static>(
        f: impl (FnOnce() -> F) + Send + 'static,
    ) -> Self {
        Self(Box::new(move || Box::pin(f())))
    }
}

impl Command for SpawnFn {
    fn apply(self, world: &mut World) {
        world.spawn_task(self.0());
    }
}

#[doc(hidden)]
#[allow(unused)]
#[macro_export]
macro_rules! test_spawn {
    ($expr: expr) => {{
        use ::bevy::prelude::*;
        use ::bevy_defer::access::*;
        use ::bevy_defer::*;
        #[derive(Debug, Clone, Copy, Component, Resource, Event, Asset, TypePath)]
        pub struct Int(i32);

        #[derive(Debug, Clone, Copy, Component, Resource, Event, Asset, TypePath)]
        pub struct Str(&'static str);

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, States)]
        pub enum MyState {
            A,
            B,
            C,
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
            AsyncWorld.quit();
            Ok(())
        });
        app.run();
    }};
}
