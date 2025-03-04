use async_executor::Task;
use bevy::ecs::prelude::Resource;
use bevy::log::error;
use bevy::state::prelude::{State, States};
use bevy::tasks::futures_lite::FutureExt;
use rustc_hash::FxHashMap;
use std::any::type_name;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};
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

    /// Spawn a `bevy_defer` compatible future.
    ///
    /// Unlike `spawn_task`, this ensures the async function segment before the first pending `.await` is ran
    /// immediately when this function is awaited. This can be useful if the future is generated
    /// inside a world access scope, but wants to context switch into a `bevy_defer` scope for
    /// immediate world access.
    ///
    /// The function is guaranteed to return immediately when awaited.
    /// The result might be a task that runs concurrently so it must be awaited again,
    /// or call `detach` to make it run independently.
    ///
    /// # Example
    ///
    /// ```
    /// # fn calculate_damage(_a: i32, _b: i32) -> i32 {0}
    /// # fn set_hp(_hp: i32) {}
    /// # async fn display_damage(_hp: i32) {}
    ///
    /// # bevy_defer::test_spawn!({
    /// # let attacker = 0;
    /// # let defender1 = 0;
    /// # let defender2 = 0;
    /// let task1 = AsyncWorld.spawn_polled(async move {
    ///     let damage = calculate_damage(attacker, defender1);
    ///     set_hp(damage);
    ///     display_damage(damage).await;
    /// }).await;
    /// let task2 = AsyncWorld.spawn_polled(async move {
    ///     let damage = calculate_damage(attacker, defender2);
    ///     set_hp(damage);
    ///     display_damage(damage).await;
    /// }).await;
    /// // Both damage will be calculated with no delay,
    /// // then we wait for `display_damage` to complete.
    /// futures::join! { task1, task2 }
    /// # });
    /// ```
    pub async fn spawn_polled<T: 'static>(
        &self,
        fut: impl Future<Output = T> + 'static,
    ) -> ReadyOrTask<T> {
        PollOnceThenSpawn(Box::pin(fut)).await
    }
}

#[doc(hidden)]
pub struct PollOnceThenSpawn<T: 'static>(Pin<Box<dyn Future<Output = T>>>);

#[doc(hidden)]
pub enum ReadyOrTask<T: 'static> {
    Ready(Option<T>),
    Spawned(Task<T>),
}

impl<T: 'static> ReadyOrTask<T> {
    pub fn detach(self) {
        match self {
            ReadyOrTask::Ready(_) => (),
            ReadyOrTask::Spawned(task) => task.detach(),
        }
    }
}

impl<T: 'static> Unpin for ReadyOrTask<T> {}

impl<T: 'static> Future for PollOnceThenSpawn<T> {
    type Output = ReadyOrTask<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<Self::Output> {
        Poll::Ready(match self.0.poll(cx) {
            Poll::Ready(result) => ReadyOrTask::Ready(Some(result)),
            Poll::Pending => {
                // Pending is a zst so this is free.
                let fut = mem::replace(&mut self.0, Box::pin(core::future::pending()));
                ReadyOrTask::Spawned(SPAWNER.with(|s| s.spawn(fut)))
            }
        })
    }
}

impl<T: 'static> Future for ReadyOrTask<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut *self {
            ReadyOrTask::Ready(result) => match result.take() {
                Some(result) => Poll::Ready(result),
                None => Poll::Pending,
            },
            ReadyOrTask::Spawned(task) => task.poll(cx),
        }
    }
}
