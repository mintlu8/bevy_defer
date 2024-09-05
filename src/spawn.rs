use async_executor::Task;
use bevy_ecs::prelude::Resource;
use bevy_log::error;
use bevy_state::prelude::{State, States};
use rustc_hash::FxHashMap;
use std::any::type_name;
use std::{future::Future, marker::PhantomData};

use crate::{executor::SPAWNER, AccessError, AccessResult, AsyncWorld};

/// A list of tasks constrained by [`States`].
#[derive(Debug, Resource)]
pub struct ScopedTasks<T: States> {
    pub(crate) tasks: FxHashMap<T, Vec<Task<AccessResult<()>>>>,
    p: PhantomData<T>,
}

impl<T: States> Default for ScopedTasks<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: States> ScopedTasks<T> {
    pub fn new() -> Self {
        ScopedTasks {
            tasks: FxHashMap::default(),
            p: PhantomData,
        }
    }

    /// Drop tasks bound to scope.
    pub fn drain(&mut self, state: &T) {
        if let Some(v) = self.tasks.get_mut(state) {
            v.clear();
        }
    }
}

impl AsyncWorld {
    /// Spawn a `bevy_defer` compatible future.
    ///
    /// The spawned future will not be dropped until finished.
    ///
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    ///
    /// # Panic Handling
    ///
    /// Due to the internals of `AsyncExecutor` this function will fail silently on panic.
    /// Use `spawn_log` or `panic=abort` for better panic handling.
    pub fn spawn_any<T: 'static>(&self, fut: impl Future<Output = T> + 'static) {
        if !SPAWNER.is_set() {
            panic!("AsyncWorld::spawn_any can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| s.spawn(fut).detach());
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
    /// * If used outside a `bevy_defer` future.
    /// * If the task has panicked.
    pub fn spawn_task<T: 'static>(&self, fut: impl Future<Output = T> + 'static) -> Task<T> {
        if !SPAWNER.is_set() {
            panic!("AsyncWorld::spawn_scoped can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| s.spawn(fut))
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
    /// * If used outside a `bevy_defer` future.
    /// * If the task has panicked.
    #[deprecated = "Use `spawn_task`."]
    pub fn spawn_scoped<T: 'static>(
        &self,
        fut: impl Future<Output = T> + 'static,
    ) -> impl Future<Output = T> {
        if !SPAWNER.is_set() {
            panic!("AsyncWorld::spawn_scoped can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| s.spawn(fut))
    }

    /// Spawn a `bevy_defer` compatible future, the future is constrained to a [`States`]
    /// and will be cancelled upon exiting the state.
    ///
    /// # Errors
    ///
    /// If not in the specified state.
    ///
    /// # Panics
    ///
    /// * If used outside a `bevy_defer` future.
    pub fn spawn_state_scoped<S: States>(
        &self,
        state: S,
        fut: impl Future<Output = AccessResult> + 'static,
    ) -> AccessResult {
        if !SPAWNER.is_set() {
            panic!("AsyncWorld::spawn_state_scoped can only be used in a bevy_defer future.")
        }
        AsyncWorld.run(|world| match world.get_resource::<State<S>>() {
            Some(s) if s.get() == &state => Ok(()),
            _ => Err(AccessError::NotInState),
        })?;
        AsyncWorld.run(|world| {
            if let Some(mut res) = world.get_resource_mut::<ScopedTasks<S>>() {
                res.tasks
                    .entry(state)
                    .or_default()
                    .push(SPAWNER.with(|s| s.spawn(fut)));
            } else {
                error!(
                    "Cannot spawn state scoped futures without `react_to_state::<{}>`.",
                    type_name::<S>()
                )
            }
        });
        Ok(())
    }

    /// Spawn a `bevy_defer` compatible future and logs errors.
    ///
    /// The spawned future will not be dropped until finished.
    ///
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    ///
    /// # Panic Handling
    ///
    /// Due to the internals of `AsyncExecutor` we currently cannot report error messages of panics.
    /// Use `panic=abort` to avoid unwinding or choose an error based approach.
    pub fn spawn<T: 'static>(&self, fut: impl Future<Output = AccessResult<T>> + 'static) {
        if !SPAWNER.is_set() {
            panic!("AsyncWorld::spawn can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| {
            let task = s.spawn(fut).fallible();
            s.spawn(async move {
                match task.await {
                    Some(Err(e)) => error!("{e}"),
                    None => error!("Task panicked!"),
                    Some(_) => (),
                }
            })
            .detach();
        });
    }
}
