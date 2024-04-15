//! Per-entity repeatable async functions.

use std::{future::Future, num::NonZeroU32, ops::{Deref, DerefMut}, sync::Arc, task::{Poll, Waker}};
use bevy_hierarchy::Children;
use bevy_reflect::Reflect;
use futures::FutureExt;
use parking_lot::Mutex;
use ref_cast::RefCast;
use std::fmt::Debug;
use std::pin::Pin;
use bevy_ecs::{component::Component, entity::Entity, system::{Local, NonSend, Query}};
use crate::{access::AsyncWorldMut, signals::Signals, AsyncExecutor, AsyncResult, QueryQueue};
use super::AsyncFailure;
#[allow(unused)]
use crate::{access::AsyncComponent, signals::{Sender, Receiver}};


/// Construct a [`Future`] via [`AsyncSystem`] semantics. 
/// 
/// The future repeats forever, unless [`AsyncFailure::ManuallyKilled`] is returned.
/// 
/// This function uses [`AsyncWorldParam`] as parameters, as there is no active entity.
/// 
/// See [`async_system!`](crate::async_system) for syntax.
#[macro_export]
macro_rules! system_future {
    (|$($field: ident : $ty: ty),* $(,)?| $body: expr) => {
        {
            async move {
                use $crate::access::*;
                let __world = $crate::world();
                loop {
                    $(let $field = <$ty as $crate::async_systems::AsyncWorldParam>::from_async_context(&__world).ok_or($crate::AsyncFailure::WorldParamNotFound)?;)*
                    if let Err(AsyncFailure::ManuallyKilled) = async {
                        let _ = $body;
                        Result::<(), $crate::AsyncFailure>::Ok(())
                    }.await {
                        return Result::<(), $crate::AsyncFailure>::Ok(())
                    }
                }
            }
        }
    };
}

/// Construct an async system via the [`AsyncEntityParam`] abstraction.
/// 
/// # Syntax
/// 
/// * Expects an async closure with [`AsyncEntityParam`]s as parameters and returns `()`.
/// * `?` can be used to propagate [`AsyncFailure`]s.
/// * Most of this crate's [`AsyncEntityParam`]s, like [`Sender`], [`Receiver`] and [`AsyncComponent`] are automatically 
/// imported and may shadow external names. 
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
macro_rules! async_system {
    (|$($field: ident : $ty: ty),* $(,)?| $body: expr) => {
        {
            use $crate::access::*;
            use $crate::signals::{Sender, Receiver};
            $crate::async_systems::AsyncSystem::new(move |entity: $crate::Entity, executor: $crate::access::AsyncWorldMut, signals: &$crate::signals::Signals, children: &[$crate::Entity]| {
                $(let $field = <$ty as $crate::async_systems::AsyncEntityParam>::fetch_signal(signals)?;)*
                $(let $field = <$ty as $crate::async_systems::AsyncEntityParam>::from_async_context(entity, &executor, $field, children)?;)*
                Some(async move {
                    let _ = $body;
                    Ok(())
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
        if let Some(waker) = self.0.lock().take() {
            waker.wake()
        }
    }
}


/// A shared storage that cleans up associated futures
/// when their associated entity is destroyed.
#[derive(Debug, Default)]
pub(crate) struct ChildAlive{
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
            *self.inner.lock() = Some(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Pending
        }
    }
}

impl Drop for ChildAlive {
    fn drop(&mut self) {
        *self.inner.lock() = None
    }
}

type PinnedFut = Pin<Box<dyn Future<Output = Result<(), AsyncFailure>> + 'static>>;

/// An async system function.
pub struct AsyncSystem {
    pub(crate) function: Box<dyn FnMut(
        Entity,
        &AsyncWorldMut,
        &Signals,
        &[Entity],
    ) -> Option<PinnedFut> + Send + Sync> ,
    pub(crate) marker: ParentAlive,
    pub id: Option<NonZeroU32>,
}

impl AsyncSystem {
    pub fn new<F>(mut f: impl FnMut(Entity, AsyncWorldMut, &Signals, &[Entity]) -> Option<F> + Send + Sync + 'static) -> Self where F: Future<Output = AsyncResult> + 'static {
        AsyncSystem {
            function: Box::new(move |entity, executor, signals, children| {
                f(entity, executor.clone(), signals, children).map(|x| Box::pin(x) as PinnedFut)
            }),
            marker: ParentAlive::new(),
            id: None
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
            systems: iter.into_iter().collect()
        }
    }
}

impl AsyncSystems {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_single(sys: AsyncSystem) -> Self  {
        AsyncSystems {
            systems: vec![sys]
        }
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
    fn from_async_context(
        executor: &AsyncWorldMut,
    ) -> Option<Self>;
}

/// A parameter of an [`AsyncSystem`].
pub trait AsyncEntityParam: Sized {
    type Signal: Send + Sync + 'static;

    /// If not found, log what's missing and return None.
    fn fetch_signal(signals: &Signals) -> Option<Self::Signal>;

    /// Obtain `Self` from the async context.
    fn from_async_context(
        entity: Entity,
        executor: &AsyncWorldMut,
        signal: Self::Signal,
        children: &[Entity],
    ) -> Option<Self>;
}

impl<T> AsyncEntityParam for T where T: AsyncWorldParam {
    type Signal = ();

    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &AsyncWorldMut,
        _: Self::Signal,
        _: &[Entity],
    ) -> Option<Self> {
        T::from_async_context(executor)
    }
}

/// System that pushes inactive [`AsyncSystems`] onto the executor.
pub fn push_async_systems(
    dummy: Local<Signals>,
    queue: NonSend<QueryQueue>,
    exec: NonSend<AsyncExecutor>,
    mut query: Query<(Entity, Option<&Signals>, &mut AsyncSystems, Option<&Children>)>
) {
    for (entity, signals, mut systems, children) in query.iter_mut() {
        let signals = signals.unwrap_or(&dummy);
        for system in systems.systems.iter_mut(){
            system.spawn(entity, &queue, &exec, signals, children);
        }
    }
}

impl AsyncSystem {
    /// Spawn an [`AsyncSystem`] onto the executor.
    pub fn spawn(&mut self, entity: Entity, queue: &QueryQueue, executor: &AsyncExecutor, signals: &Signals, children: Option<&Children>) {
        if !self.marker.other_alive() {
            let alive = self.marker.clone_child();
            let children = children.map(|x| x.as_ref()).unwrap_or(&[]);
            let Some(fut) = (self.function)(entity, AsyncWorldMut::ref_cast(&queue.0), signals, children) else {return};
            executor.spawn(
                futures::future::select(alive, fut.map(|_| ()))
            );
        }
    }
}
