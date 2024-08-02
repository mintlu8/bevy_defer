use std::future::Future;
use bevy_log::error;

use crate::{executor::SPAWNER, AccessResult, AsyncWorld};



impl AsyncWorld {
    /// Spawn a `bevy_defer` compatible future.
    ///
    /// The spawned future will not be dropped until finished.
    ///
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    pub fn spawn<T: 'static>(&self, fut: impl Future<Output = T> + 'static) {
        if !SPAWNER.is_set() {
            panic!("bevy_defer::spawn can only be used in a bevy_defer future.")
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
    pub fn spawn_scoped<T: 'static>(
        &self,
        fut: impl Future<Output = T> + 'static,
    ) -> impl Future<Output = T> {
        if !SPAWNER.is_set() {
            panic!("bevy_defer::spawn_scoped can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| s.spawn(fut))
    }

    /// Spawn a `bevy_defer` compatible future and logs errors.
    ///
    /// The spawned future will not be dropped until finished.
    ///
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    pub fn spawn_log<T: 'static>(
        &self,
        fut: impl Future<Output = AccessResult<T>> + 'static,
    ) {
        if !SPAWNER.is_set() {
            panic!("bevy_defer::spawn_log can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| {
            let task = s.spawn(fut).fallible();
            s.spawn(async move {
                match task.await {
                    Some(Err(e)) => error!("{e}"),
                    None => error!("Task panicked!"),
                    Some(_) => (),
                }
            }).detach();
        });
    }
}