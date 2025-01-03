//! Per-entity repeatable async functions.
//!
//! [`AsyncSystem`] is a system-like async function on a specific entity.
//! The component [`AsyncSystems`] is a collection of `AsyncSystem`s that runs independently.
//!
//! Generally `AsyncSystems` is useful for providing reactivity with UI when combined with signals.
//!
//! # Example
//!
//! To create an [`AsyncSystem`], use the [`async_system!`](crate::async_system) macro:
//!
//! ```
//! # /*
//! // Scale up for a second when clicked.
//! let system = async_system!(|recv: Receiver<OnClick>, transform: AsyncComponent<Transform>|{
//!     let pos: Vec3 = recv.recv().await;
//!     transform.set(|transform| transform.scale = Vec3::splat(2.0)).await?;
//!     AsyncWorld.sleep(1.0).await;
//!     transform.set(|transform| transform.scale = Vec3::ONE).await?;
//! })
//! # */
//! ```
//!
//! The parameters must implement [`AsyncEntityParam`].
//!
//! # How an `AsyncSystem` executes
//!
//! Think of an `AsyncSystem` like a loop:
//!
//! * if this `Future` is not running at the start of this frame, run it.
//! * If the function finishes, rerun on the next frame.
//! * If the entity is dropped, the `Future` will be cancelled.
//!
//! So this is similar to
//!
//! ```
//! # /*
//! spawn(async {
//!     loop {
//!         futures::select! {
//!             _ = async_system => (),
//!             _ = cancel => break,
//!         }
//!     }
//! })
//! # */
//! ```
//!
//! If you want some state to persist, for example keeping a handle alive or using a
//! `AsyncEventReader`, you might want to implement the async system as a loop:
//!
//! ```
//! # /*
//! let system = async_system!(|recv: Receiver<OnClick>, mouse_wheel: AsyncEventReader<Input<MouseWheel>>|{
//!     loop {
//!         futures::select! {
//!             _ = recv.recv().fused() => ..,
//!             pos = mouse_wheel.poll().fused() => ..
//!         }
//!     }
//! })
//! # */
//! ```
//!
//!
#![allow(deprecated)]
use super::AccessError;
#[allow(unused)]
use crate::{
    access::AsyncComponent,
    signals::{Receiver, Sender},
};
use crate::{
    executor::REACTORS, reactors::Reactors, signals::Signals, AccessResult, AsyncExecutor,
};
use bevy::ecs::{
    component::Component,
    entity::Entity,
    system::{Local, NonSend, Query, Res},
};
use bevy::hierarchy::Children;
use bevy::reflect::Reflect;
use futures::FutureExt;
use std::pin::Pin;
use std::{fmt::Debug, sync::Mutex};
use std::{
    future::Future,
    num::NonZeroU32,
    ops::{Deref, DerefMut},
    sync::Arc,
    task::{Poll, Waker},
};

/// Construct an async system via the [`AsyncEntityParam`] abstraction.
///
/// # Syntax
///
/// * Expects an async closure with [`AsyncEntityParam`]s as parameters and returns `()`.
/// * `?` can be used to propagate [`AccessError`]s.
/// * Most of this crate's [`AsyncEntityParam`]s, like [`Sender`], [`Receiver`] and [`struct@AsyncComponent`] are automatically
///   imported and may shadow external names.
///
/// # Example
///
/// ```
/// # /*
/// // Set scale based on received position
/// let system = async_system!(|recv: Receiver<PositionChanged>, transform: AsyncComponent<Transform>|{
///     let pos: Vec3 = recv.recv().await;
///     transform.set(|transform| transform.scale = pos).await?;
/// })
/// # */
/// ```
#[macro_export]
#[deprecated = "Consider using `Observer`s for UI interaction."]
macro_rules! async_system {
    (|$($field: ident : $ty: ty),* $(,)?| $body: expr) => {
        {
            use $crate::access::*;
            use $crate::AsyncAccess;
            use $crate::signals::{Sender, Receiver};
            #[allow(deprecated)]
            $crate::async_systems::AsyncSystem::new(move |entity: $crate::Entity, reactors: &$crate::reactors::Reactors, signals: &$crate::signals::Signals, children: &[$crate::Entity]| {
                $(let $field = <$ty as $crate::async_systems::AsyncEntityParam>::fetch_signal(signals)?;)*
                $(let $field = <$ty as $crate::async_systems::AsyncEntityParam>::from_async_context(entity, &reactors, $field, children)?;)*
                Some(async move {
                    let _ = $body;
                    #[allow(unreachable_code)]
                    { Ok(()) }
                })
            })
        }
    };
}

/// A shared storage that cleans up associated futures
/// when their associated entity is destroyed.
#[derive(Debug, Default)]
pub(crate) struct ParentAlive(Arc<Mutex<Option<Waker>>>);

impl ParentAlive {
    pub fn new() -> Self {
        ParentAlive::default()
    }
    pub fn other_alive(&self) -> bool {
        Arc::strong_count(&self.0) > 1
    }
    pub fn clone_child(&self) -> ChildAlive {
        ChildAlive {
            inner: self.0.clone(),
            init: false,
        }
    }
}

impl Drop for ParentAlive {
    fn drop(&mut self) {
        if let Some(waker) = self.0.lock().unwrap().take() {
            waker.wake()
        }
    }
}

/// A shared storage that cleans up associated futures
/// when their associated entity is destroyed.
#[derive(Debug, Default)]
pub(crate) struct ChildAlive {
    inner: Arc<Mutex<Option<Waker>>>,
    init: bool,
}

impl Future for ChildAlive {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if Arc::strong_count(&self.inner) <= 1 {
            Poll::Ready(())
        } else if !self.init {
            self.init = true;
            *self.inner.lock().unwrap() = Some(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Pending
        }
    }
}

impl Drop for ChildAlive {
    fn drop(&mut self) {
        *self.inner.lock().unwrap() = None
    }
}

type PinnedFut = Pin<Box<dyn Future<Output = Result<(), AccessError>> + 'static>>;

/// An async system function.
#[deprecated = "Consider using `Observer`s for UI interaction."]
pub struct AsyncSystem {
    pub(crate) function:
        Box<dyn FnMut(Entity, &Reactors, &Signals, &[Entity]) -> Option<PinnedFut> + Send + Sync>,
    pub(crate) marker: ParentAlive,
    pub id: Option<NonZeroU32>,
}

impl AsyncSystem {
    pub fn new<F>(
        mut f: impl FnMut(Entity, &Reactors, &Signals, &[Entity]) -> Option<F> + Send + Sync + 'static,
    ) -> Self
    where
        F: Future<Output = AccessResult> + 'static,
    {
        AsyncSystem {
            function: Box::new(move |entity, reactors, signals, children| {
                f(entity, reactors, signals, children).map(|x| Box::pin(x) as PinnedFut)
            }),
            marker: ParentAlive::new(),
            id: None,
        }
    }

    /// Mark the [`AsyncSystem`] with an `id` that can be later used to remove this system.
    pub fn with_id(mut self, id: NonZeroU32) -> Self {
        self.id = Some(id);
        self
    }
}

impl Debug for AsyncSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncSystem").field("id", &self.id).finish()
    }
}

/// A component containing an entity's `AsyncSystem`s.
#[derive(Debug, Component, Default, Reflect)]
#[deprecated = "Consider using `Observer`s for UI interaction."]
pub struct AsyncSystems {
    #[reflect(ignore)]
    pub systems: Vec<AsyncSystem>,
}

impl Deref for AsyncSystems {
    type Target = Vec<AsyncSystem>;

    fn deref(&self) -> &Self::Target {
        &self.systems
    }
}

impl DerefMut for AsyncSystems {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.systems
    }
}

impl FromIterator<AsyncSystem> for AsyncSystems {
    fn from_iter<T: IntoIterator<Item = AsyncSystem>>(iter: T) -> Self {
        AsyncSystems {
            systems: iter.into_iter().collect(),
        }
    }
}

impl AsyncSystems {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_single(sys: AsyncSystem) -> Self {
        AsyncSystems { systems: vec![sys] }
    }

    pub fn and(mut self, sys: AsyncSystem) -> Self {
        self.systems.push(sys);
        self
    }

    pub fn extend(mut self, systems: AsyncSystems) -> Self {
        self.systems.extend(systems.systems);
        self
    }

    /// Remove all [`AsyncSystem`]s with a specific id.
    ///
    /// This will cancel currently running futures.
    pub fn remove_by_id(&mut self, id: NonZeroU32) {
        self.retain(|x| x.id != Some(id))
    }
}

/// A parameter of an [`AsyncSystem`].
pub trait AsyncWorldParam: Sized {
    /// Obtain `Self` from the async context.
    fn from_async_context(queue: &Reactors) -> Option<Self>;

    /// Construct inside a `bevy_defer` future.
    fn build_in_async() -> Option<Self> {
        REACTORS.with(|reactors| <Self as AsyncWorldParam>::from_async_context(reactors))
    }
}

/// A parameter of an [`AsyncSystem`].
pub trait AsyncEntityParam: Sized {
    /// Associated signal, or `()`.
    type Signal: Send + Sync + 'static;

    /// If not found, log what's missing and return None.
    fn fetch_signal(signals: &Signals) -> Option<Self::Signal>;

    /// Obtain `Self` from the async context.
    fn from_async_context(
        entity: Entity,
        queue: &Reactors,
        signal: Self::Signal,
        children: &[Entity],
    ) -> Option<Self>;
}

impl<T> AsyncEntityParam for T
where
    T: AsyncWorldParam,
{
    type Signal = ();

    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        reactors: &Reactors,
        _: Self::Signal,
        _: &[Entity],
    ) -> Option<Self> {
        T::from_async_context(reactors)
    }
}

/// System that pushes inactive [`AsyncSystems`] onto the executor.
pub fn push_async_systems(
    dummy: Local<Signals>,
    exec: NonSend<AsyncExecutor>,
    reactors: Res<Reactors>,
    mut query: Query<(
        Entity,
        Option<&Signals>,
        &mut AsyncSystems,
        Option<&Children>,
    )>,
) {
    for (entity, signals, mut systems, children) in query.iter_mut() {
        let signals = signals.unwrap_or(&dummy);
        for system in systems.systems.iter_mut() {
            system.spawn(entity, &reactors, &exec, signals, children);
        }
    }
}

impl AsyncSystem {
    /// Spawn an [`AsyncSystem`] onto the executor.
    pub fn spawn(
        &mut self,
        entity: Entity,
        reactors: &Reactors,
        executor: &AsyncExecutor,
        signals: &Signals,
        children: Option<&Children>,
    ) {
        if !self.marker.other_alive() {
            let alive = self.marker.clone_child();
            let children = children.map(|x| x.as_ref()).unwrap_or(&[]);
            let Some(fut) = (self.function)(entity, reactors, signals, children) else {
                return;
            };
            executor.spawn_any(futures::future::select(alive, fut.map(|_| ())));
        }
    }
}
