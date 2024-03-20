use std::{future::Future, ops::{Deref, DerefMut}, rc::Rc};
use bevy_reflect::Reflect;
use std::fmt::Debug;
use std::pin::Pin;
use bevy_ecs::{component::Component, entity::Entity};
use crate::{signals::Signals, AsyncResult};
use crate::ParentAlive;
use super::{AsyncQueryQueue, AsyncFailure};
#[allow(unused)]
use crate::{AsyncComponent, signals::{Sender, Receiver}};

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
            use $crate::{AsyncWorldMut, AsyncEntityMut, AsyncComponent, AsyncResource};
            use $crate::{AsyncSystemParam, AsyncQuery, AsyncEntityQuery};
            use $crate::signals::{Sender, Receiver};
            $crate::AsyncSystem::new(move |entity: $crate::Entity, executor: ::std::rc::Rc<$crate::AsyncQueryQueue>, signals: &$crate::signals::Signals| {
                $(let $field = <$ty as $crate::AsyncEntityParam>::fetch_signal(signals)?;)*
                Some(async move {
                    $(let $field = <$ty as $crate::AsyncEntityParam>::from_async_context(entity, &executor, $field);)*
                    let _ = $body;
                    Ok(())
                })
            })
        }
        
    };
}

type PinnedFut = Pin<Box<dyn Future<Output = Result<(), AsyncFailure>> + 'static>>;

/// An async system function.
pub struct AsyncSystem {
    pub(crate) function: Box<dyn FnMut(
        Entity,
        &Rc<AsyncQueryQueue>,
        &Signals,
    ) -> Option<PinnedFut> + Send + Sync> ,
    pub(crate) marker: ParentAlive,
}

impl AsyncSystem {
    pub fn new<F>(mut f: impl FnMut(Entity, Rc<AsyncQueryQueue>, &Signals) -> Option<F> + Send + Sync + 'static) -> Self where F: Future<Output = AsyncResult> + 'static {
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
}

/// A parameter of an [`AsyncSystem`].
pub trait AsyncEntityParam: Sized {
    type Signal: Send + Sync + 'static;

    /// If not found, log what's missing and return None.
    fn fetch_signal(signals: &Signals) -> Option<Self::Signal>;

    /// Obtain `Self` from the async context.
    fn from_async_context(
        entity: Entity,
        executor: &Rc<AsyncQueryQueue>,
        signal: Self::Signal,
    ) -> Self;
}
