use std::{future::Future, ops::{Deref, DerefMut}};
use bevy_reflect::Reflect;
use triomphe::Arc;
use std::fmt::Debug;
use std::pin::Pin;
use bevy_ecs::{component::Component, entity::Entity};

use crate::KeepAlive;

use super::{async_param::AsyncSystemParam, AsyncExecutor, AsyncFailure, Signals};

/// Macro for simplifying construction of an async system.
#[macro_export]
macro_rules! async_system {
    (|$($field: ident :$ty: ty),* $(,)?| $body: expr) => {
        $crate::AsyncSystem::new(move |$($field :$ty),*| async move {
            let _ = $body;
            Ok(())
        })
    };
}

/// Core trait for an async system function.
pub trait AsyncSystemFunction<M>: Send + Sync + 'static {
    fn as_future(
        &self,
        entity: Entity,
        executor: &Arc<AsyncExecutor>,
        signals: &Signals,
    ) -> impl Future<Output = Result<(), AsyncFailure>> + Send + Sync + 'static;
}


macro_rules! impl_async_system_fn {
    () => {};
    ($head: ident $(,$tail: ident)*) => {
        impl_async_system_fn!($($tail),*);

        const _: () = {

            impl<F, Fut: Future<Output = Result<(), AsyncFailure>> + Send + Sync + 'static, $head $(,$tail)*>
                    AsyncSystemFunction<($head, $($tail,)*)> for F where $head: AsyncSystemParam, $($tail: AsyncSystemParam,)*
                        F: Fn($head $(,$tail)*) -> Fut + Send + Sync + 'static,
                         {
                fn as_future(
                    &self,
                    entity: Entity,
                    executor: &Arc<AsyncExecutor>,
                    signals: &Signals,
                ) -> impl Future<Output = Result<(), AsyncFailure>> + Send + Sync + 'static {
                    self(
                        $head::from_async_context(entity, executor, signals),
                        $($tail::from_async_context(entity, executor, signals)),*
                    )
                }
            }
        };
    };
}

impl_async_system_fn!(
    T0, T1, T2, T3, T4,
    T5, T6, T7, T8, T9,
    T10, T11, T12, T13, T14
);


/// An async system function.
pub struct AsyncSystem {
    pub(crate) function: Box<dyn Fn(
        Entity,
        &Arc<AsyncExecutor>,
        &Signals,
    ) -> Pin<Box<dyn Future<Output = Result<(), AsyncFailure>> + Send + Sync + 'static>> + Send + Sync> ,
    pub(crate) marker: KeepAlive,
}

impl AsyncSystem {
    pub fn new<F, M>(f: F) -> Self where F: AsyncSystemFunction<M>  {
        AsyncSystem {
            function: Box::new(move |entity, executor, signals| {
                Box::pin(f.as_future(entity, executor, signals))
            }),
            marker: KeepAlive::new()
        }
    }
}

impl Debug for AsyncSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncSystem").finish()
    }
}


/// A component containing an entity's `AsyncSystem`s.
#[derive(Debug, Component, Reflect)]
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


impl AsyncSystems {
    pub fn new<F, M>(f: F) -> Self where F: AsyncSystemFunction<M>  {
        AsyncSystems {
            systems: vec![AsyncSystem {
                function: Box::new(move |entity, executor, signals| {
                    Box::pin(f.as_future(entity, executor, signals))
                }),
                marker: KeepAlive::new()
            }]
        }
    }

    pub fn from_single(sys: AsyncSystem) -> Self  {
        AsyncSystems {
            systems: vec![sys]
        }
    }


    pub fn from_systems(iter: impl IntoIterator<Item = AsyncSystem>) -> Self  {
        AsyncSystems {
            systems: iter.into_iter().collect()
        }
    }

    pub fn and<F, M>(mut self, f: F) -> Self where F: AsyncSystemFunction<M>  {
        self.systems.push(
            AsyncSystem {
                function: Box::new(move |entity, executor, signals| {
                    Box::pin(f.as_future(entity, executor, signals))
                }),
                marker: KeepAlive::new()
            }
        );
        self
    }
}
