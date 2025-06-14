#![doc=include_str!("../README.md")]
#![allow(clippy::type_complexity)]
#![cfg_attr(docsrs, feature(doc_cfg))]
use bevy::app::{App, First, Plugin, PostUpdate, PreUpdate, Update};
use bevy::ecs::component::Component;
use bevy::ecs::event::Event;
use bevy::ecs::intern::Interned;
use bevy::ecs::query::{QueryData, QueryFilter};
use bevy::ecs::schedule::IntoScheduleConfigs as _;
use bevy::ecs::system::Command;
use bevy::prelude::EntityCommands;
use bevy::state::prelude::State;
use bevy::state::state::States;
use bevy::time::TimeSystem;
use std::fmt::Formatter;
use std::{any::type_name, pin::Pin};

pub mod access;
pub mod cancellation;
mod commands;
mod entity_commands;
mod errors;
mod event;
mod executor;
pub mod ext;
mod fetch;
mod inspect;
mod queue;
pub use inspect::{EntityInspectors, InspectEntity};
pub mod reactors;
pub mod signals;
mod spawn;
pub(crate) mod sync;
pub mod tween;
pub use access::async_asset::AssetSet;
pub use access::async_query::OwnedQueryState;
pub use access::traits::AsyncAccess;
pub use access::AsyncWorld;
pub use async_executor::Task;
use bevy::ecs::{
    schedule::{ScheduleLabel, SystemSet},
    system::Commands,
    world::World,
};
use bevy::reflect::std_traits::ReflectDefault;
pub use errors::AccessError;
pub use event::EventChannel;
pub use executor::{in_async_context, AsyncExecutor};
#[doc(hidden)]
pub use fetch::{fetch, fetch0, fetch1, fetch2, FetchEntity, FetchOne, FetchWorld};
pub use queue::QueryQueue;
use reactors::Reactors;
pub use spawn::ScopedTasks;
#[doc(hidden)]
#[cfg(feature = "spawn_macro")]
pub mod spawn_macro;

pub mod systems {
    pub use crate::event::react_to_event;
    pub use crate::executor::run_async_executor;
    pub use crate::queue::{run_fixed_queue, run_time_series, run_watch_queries};
    pub use crate::reactors::{react_to_component_change, react_to_state};

    #[cfg(feature = "bevy_animation")]
    pub use crate::ext::anim::react_to_animation;
    #[cfg(feature = "bevy_animation")]
    pub use crate::ext::anim::react_to_main_animation_change;
    #[cfg(feature = "bevy_scene")]
    pub use crate::ext::scene::react_to_scene_load;
}

pub use crate::sync::oneshot::channel;
use std::future::Future;

pub(crate) static CHANNEL_CLOSED: &str = "channel closed unexpectedly";

#[doc(hidden)]
pub use bevy::ecs::entity::Entity;
#[doc(hidden)]
pub use bevy::ecs::system::{NonSend, Res, SystemParam};
#[doc(hidden)]
pub use bevy::log::error;
#[doc(hidden)]
pub use ref_cast::RefCast;

use queue::run_fixed_queue;
use signals::Signals;

#[cfg(feature = "derive")]
pub use bevy_defer_derive::{async_access, async_dyn};

/// Result type of spawned tasks.
pub type AccessResult<T = ()> = Result<T, AccessError>;

pub type BoxedFuture = Pin<Box<dyn Future<Output = AccessResult>>>;

pub type BoxedSharedFuture = Pin<Box<dyn Future<Output = AccessResult> + Send + Sync>>;

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
            .init_resource::<EntityInspectors>()
            .register_type::<Signals>()
            .register_type_data::<Signals, ReflectDefault>()
            .init_schedule(BeforeAsyncExecutor)
            .add_systems(First, systems::run_time_series.after(TimeSystem))
            .add_systems(Update, run_fixed_queue)
            .add_systems(BeforeAsyncExecutor, systems::run_watch_queries);

        #[cfg(feature = "bevy_scene")]
        app.add_systems(BeforeAsyncExecutor, systems::react_to_scene_load);
        #[cfg(feature = "bevy_animation")]
        app.add_systems(BeforeAsyncExecutor, systems::react_to_animation);
        #[cfg(feature = "bevy_animation")]
        app.add_systems(BeforeAsyncExecutor, systems::react_to_main_animation_change);
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

    /// Spawn a `bevy_defer` compatible future, the future is constrained to a [`States`]
    /// and will be cancelled upon exiting the state.
    ///
    /// # Errors
    ///
    /// If not in the specified state.
    fn spawn_state_scoped<S: States>(
        &mut self,
        state: S,
        fut: impl Future<Output = AccessResult> + 'static,
    ) -> AccessResult;

    /// Initialize [`EventChannel<E>`].
    fn register_oneshot_event<E: Send + Sync + 'static>(&mut self) -> &mut Self;

    /// Registers a method that prints an entity in `bevy_defer`.
    ///
    /// This method will be used for printing [`AccessError`].
    /// [`InspectEntity`] can be used for custom debug printing
    ///  in `bevy_defer`'s scope.
    ///
    /// Only the first successful formatting function will be called according to their priorities.
    /// A [`Name`](bevy::core::Name) based method is automatically added at priority `0`.
    fn register_inspect_entity_by_component<C: Component>(
        &mut self,
        priority: i32,
        f: impl Fn(Entity, &C, &mut Formatter) + Send + Sync + 'static,
    ) -> &mut Self;

    /// Registers a method that prints an entity in `bevy_defer`.
    ///
    /// This method will be used for printing [`AccessError`].
    /// [`InspectEntity`] can be used for custom debug printing
    ///  in `bevy_defer`'s scope.
    ///
    /// Only the first successful formatting function will be called according to their priorities.
    /// A [`Name`](bevy::core::Name) based method is automatically added at priority `0`.
    fn register_inspect_entity_by_query<Q: QueryData + 'static, F: QueryFilter + 'static>(
        &mut self,
        priority: i32,
        f: impl Fn(Q::Item<'_>, &mut Formatter) + Send + Sync + 'static,
    ) -> &mut Self;
}

impl AsyncExtension for World {
    fn spawn_task(&mut self, f: impl Future<Output = AccessResult> + 'static) -> &mut Self {
        self.non_send_resource::<AsyncExecutor>().spawn(f);
        self
    }

    fn spawn_state_scoped<S: States>(
        &mut self,
        state: S,
        fut: impl Future<Output = AccessResult> + 'static,
    ) -> AccessResult {
        match self.get_resource::<State<S>>() {
            Some(s) if s.get() == &state => (),
            _ => return Err(AccessError::NotInState),
        };
        let task = self.non_send_resource::<AsyncExecutor>().spawn_task(fut);
        if let Some(mut res) = self.get_resource_mut::<ScopedTasks<S>>() {
            res.tasks.entry(state).or_default().push(task);
        } else {
            error!(
                "Cannot spawn state scoped futures without `react_to_state::<{}>`.",
                type_name::<S>()
            )
        }
        Ok(())
    }

    fn register_inspect_entity_by_component<C: Component>(
        &mut self,
        priority: i32,
        f: impl Fn(Entity, &C, &mut Formatter) + Send + Sync + 'static,
    ) -> &mut Self {
        self.resource_mut::<EntityInspectors>().push(priority, f);
        self
    }

    fn register_inspect_entity_by_query<Q: QueryData + 'static, F: QueryFilter + 'static>(
        &mut self,
        priority: i32,
        f: impl Fn(Q::Item<'_>, &mut Formatter) + Send + Sync + 'static,
    ) -> &mut Self {
        self.resource_mut::<EntityInspectors>()
            .push_query::<Q, F>(priority, f);
        self
    }

    fn register_oneshot_event<E: Send + Sync + 'static>(&mut self) -> &mut Self {
        self.init_resource::<EventChannel<E>>();
        self
    }
}

impl AsyncExtension for App {
    fn spawn_task(&mut self, f: impl Future<Output = AccessResult> + 'static) -> &mut Self {
        self.world().non_send_resource::<AsyncExecutor>().spawn(f);
        self
    }

    fn spawn_state_scoped<S: States>(
        &mut self,
        state: S,
        fut: impl Future<Output = AccessResult> + 'static,
    ) -> AccessResult {
        self.world_mut().spawn_state_scoped(state, fut)
    }

    fn register_oneshot_event<E: Send + Sync + 'static>(&mut self) -> &mut Self {
        self.world_mut().register_oneshot_event::<E>();
        self
    }

    fn register_inspect_entity_by_component<C: Component>(
        &mut self,
        priority: i32,
        f: impl Fn(Entity, &C, &mut Formatter) + Send + Sync + 'static,
    ) -> &mut Self {
        self.world_mut()
            .register_inspect_entity_by_component(priority, f);
        self
    }

    fn register_inspect_entity_by_query<Q: QueryData + 'static, F: QueryFilter + 'static>(
        &mut self,
        priority: i32,
        f: impl Fn(Q::Item<'_>, &mut Formatter) + Send + Sync + 'static,
    ) -> &mut Self {
        self.world_mut()
            .register_inspect_entity_by_query::<Q, F>(priority, f);
        self
    }
}

/// Extension for [`App`] to add reactors.
pub trait AppReactorExtension {
    /// React to changes in an [`Event`] by duplicating events to [`EventChannel<E>`].
    ///
    /// Initializes the resource [`EventChannel<E>`].
    fn react_to_event<E: Event + Clone>(&mut self) -> &mut Self;

    /// React to changes in a [`States`].
    fn react_to_state<S: States>(&mut self) -> &mut Self;

    /// React to changes in a [`Component`].
    fn react_to_component_change<C: Component + Eq + Clone + Default>(&mut self) -> &mut Self;
}

impl AppReactorExtension for App {
    fn react_to_event<E: Event + Clone>(&mut self) -> &mut Self {
        self.register_oneshot_event::<E>();
        self.add_systems(BeforeAsyncExecutor, systems::react_to_event::<E>);
        self
    }

    fn react_to_state<S: States>(&mut self) -> &mut Self {
        self.add_systems(BeforeAsyncExecutor, systems::react_to_state::<S>);
        self.init_resource::<ScopedTasks<S>>();
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
    ///
    /// Unlike [`AsyncExtension::spawn_task`] this accepts a closure so
    /// that users can smuggle `!Send` futures across thread boundaries.
    ///
    /// ```rust
    /// # /*
    /// move || async move {
    ///     ...
    /// }
    /// # */
    /// ```
    fn spawn_task<F: Future<Output = AccessResult> + 'static>(
        &mut self,
        f: impl FnOnce() -> F + Send + 'static,
    ) -> &mut Self;

    /// Spawn a `bevy_defer` compatible future, the future is constrained to a [`States`]
    /// and will be cancelled upon exiting the state.
    ///
    /// # Errors
    ///
    /// If not in the specified state.
    fn spawn_state_scoped<S: States, F: Future<Output = AccessResult> + 'static>(
        &mut self,
        state: S,
        fut: impl FnOnce() -> F + Send + 'static,
    ) -> &mut Self;
}

impl AsyncCommandsExtension for Commands<'_, '_> {
    fn spawn_task<F: Future<Output = AccessResult> + 'static>(
        &mut self,
        f: impl (FnOnce() -> F) + Send + 'static,
    ) -> &mut Self {
        self.queue(SpawnFn::new(f));
        self
    }

    fn spawn_state_scoped<S: States, F: Future<Output = AccessResult> + 'static>(
        &mut self,
        state: S,
        f: impl FnOnce() -> F + Send + 'static,
    ) -> &mut Self {
        self.queue(StateScopedSpawnFn::new(state, f));
        self
    }
}

/// Extension for [`Commands`].
pub trait AsyncEntityCommandsExtension {
    /// Spawn a task to be run on the [`AsyncExecutor`].
    ///
    /// Unlike [`AsyncExtension::spawn_task`] this accepts a closure so
    /// that users can smuggle `!Send` futures across thread boundaries.
    ///
    /// ```rust
    /// # /*
    /// move || async move {
    ///     ...
    /// }
    /// # */
    /// ```
    fn spawn_task<F: Future<Output = AccessResult> + 'static>(
        &mut self,
        f: impl FnOnce(Entity) -> F + Send + 'static,
    ) -> &mut Self;

    /// Spawn a `bevy_defer` compatible future, the future is constrained to a [`States`]
    /// and will be cancelled upon exiting the state.
    ///
    /// # Errors
    ///
    /// If not in the specified state.
    fn spawn_state_scoped<S: States, F: Future<Output = AccessResult> + 'static>(
        &mut self,
        state: S,
        fut: impl FnOnce(Entity) -> F + Send + 'static,
    ) -> &mut Self;
}

impl AsyncEntityCommandsExtension for EntityCommands<'_> {
    fn spawn_task<F: Future<Output = AccessResult> + 'static>(
        &mut self,
        f: impl (FnOnce(Entity) -> F) + Send + 'static,
    ) -> &mut Self {
        let entity = self.id();
        self.commands().queue(SpawnFn::new(move || f(entity)));
        self
    }

    fn spawn_state_scoped<S: States, F: Future<Output = AccessResult> + 'static>(
        &mut self,
        state: S,
        f: impl FnOnce(Entity) -> F + Send + 'static,
    ) -> &mut Self {
        let entity = self.id();
        self.commands()
            .queue(StateScopedSpawnFn::new(state, move || f(entity)));
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

/// [`Command`] for spawning a task.
pub struct StateScopedSpawnFn<S: States> {
    future: Box<dyn (FnOnce() -> Pin<Box<dyn Future<Output = AccessResult>>>) + Send + 'static>,
    state: S,
}

impl<S: States> StateScopedSpawnFn<S> {
    fn new<F: Future<Output = AccessResult> + 'static>(
        state: S,
        f: impl (FnOnce() -> F) + Send + 'static,
    ) -> Self {
        Self {
            future: Box::new(move || Box::pin(f())),
            state,
        }
    }
}

impl<S: States> Command for StateScopedSpawnFn<S> {
    fn apply(self, world: &mut World) {
        let _ = world.spawn_state_scoped(self.state, (self.future)());
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
        use bevy::state::app::StatesPlugin;
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
        app.add_plugins(StatesPlugin);
        app.add_plugins(AssetPlugin::default());
        app.init_asset::<Image>();
        app.add_plugins(bevy_defer::AsyncPlugin::default_settings());
        app.world_mut().spawn(Int(4));
        app.world_mut().spawn(Str("Ferris"));
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
