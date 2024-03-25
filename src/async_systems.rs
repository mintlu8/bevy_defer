//! Per-entity repeatable async functions.

use std::{future::Future, ops::{Deref, DerefMut}, sync::Arc, task::{Poll, Waker}};
use bevy_reflect::Reflect;
use parking_lot::Mutex;
use std::fmt::Debug;
use std::pin::Pin;
use bevy_ecs::{component::Component, entity::Entity};
use crate::{async_world::AsyncWorldMut, signals::Signals, AsyncResult};
use super::AsyncFailure;
#[allow(unused)]
use crate::{access::AsyncComponent, signals::{Sender, Receiver}};

/// Macro for constructing an async system via the [`AsyncEntityParam`] abstraction.
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
/// // Set scale based on received position
/// let system = async_system!(|recv: Receiver<PositionChanged>, transform: AsyncComponent<Transform>|{
///     let pos: Vec3 = recv.recv().await;
///     transform.set(|transform| transform.scale = pos).await?;
/// })
/// ```
#[macro_export]
macro_rules! async_system {
    (|$($field: ident : $ty: ty),* $(,)?| $body: expr) => {
        {
            use $crate::access::{AsyncWorldMut, AsyncEntityMut, AsyncComponent, AsyncResource};
            use $crate::access::{AsyncSystemParam, AsyncQuery, AsyncEntityQuery};
            use $crate::signals::{Sender, Receiver};
            $crate::async_systems::AsyncSystem::new(move |entity: $crate::Entity, executor: $crate::access::AsyncWorldMut, signals: &$crate::signals::Signals| {
                $(let $field = <$ty as $crate::async_systems::AsyncEntityParam>::fetch_signal(signals)?;)*
                Some(async move {
                    $(let $field = <$ty as $crate::async_systems::AsyncEntityParam>::from_async_context(entity, &executor, $field);)*
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
    ) -> Option<PinnedFut> + Send + Sync> ,
    pub(crate) marker: ParentAlive,
}

impl AsyncSystem {
    pub fn new<F>(mut f: impl FnMut(Entity, AsyncWorldMut, &Signals) -> Option<F> + Send + Sync + 'static) -> Self where F: Future<Output = AsyncResult> + 'static {
        AsyncSystem {
            function: Box::new(move |entity, executor, signals| {
                f(entity, executor.clone(), signals).map(|x| Box::pin(x) as PinnedFut)
            }),
            marker: ParentAlive::new()
        }
    }
}

impl Debug for AsyncSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncSystem").finish()
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
    ) -> Self;
}
