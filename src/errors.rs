use crate::InspectEntity;
use bevy::ecs::entity::Entity;
use bevy::log::error;
use std::any::type_name;

#[cfg(feature = "full_types")]
fn fmt(s: &str) -> &str {
    s
}

#[cfg(not(feature = "full_types"))]
fn fmt(s: &str) -> String {
    pretty_type_name::pretty_type_name_str(s)
}

/// Standard errors for the async runtime.
///
/// # Error Logging
///
/// Consider the `instrument` macro from the `tracing` crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AccessError {
    #[error("async channel closed")]
    ChannelClosed,
    #[error("entity {} not found", InspectEntity(*.0))]
    EntityNotFound(Entity),
    #[error("single entity not found in query {}", fmt(query))]
    NoEntityFound { query: &'static str },
    #[error("too many entities")]
    TooManyEntities { query: &'static str },
    #[error("child index {index} missing")]
    ChildNotFound { index: usize },
    #[error("component <{}> not found", fmt(name))]
    ComponentNotFound { name: &'static str },
    #[error("resource <{}> not found", fmt(name))]
    ResourceNotFound { name: &'static str },
    #[error("asset <{}> not found", fmt(name))]
    AssetNotFound { name: &'static str },
    #[error("event <{}> not registered", fmt(name))]
    EventNotRegistered { name: &'static str },
    #[error("signal <{}> not found", fmt(name))]
    SignalNotFound { name: &'static str },
    #[error("schedule not found")]
    ScheduleNotFound,
    #[error("system param error")]
    SystemParamError,
    #[error("AsyncWorldParam not found")]
    WorldParamNotFound,
    #[error("SystemId not found")]
    SystemIdNotFound,
    #[error("Task spawned has panicked")]
    TaskPanicked,
    #[error("name not found")]
    NameNotFound,
    #[error("not in state")]
    NotInState,
    #[error("io error")]
    IO,
    #[error("custom error: {0}")]
    Custom(&'static str),
    #[error("this error should not happen")]
    ShouldNotHappen,
}

impl AccessError {
    pub fn component<T>() -> Self {
        AccessError::ComponentNotFound {
            name: type_name::<T>(),
        }
    }

    pub fn resource<T>() -> Self {
        AccessError::ResourceNotFound {
            name: type_name::<T>(),
        }
    }

    pub fn asset<T>() -> Self {
        AccessError::AssetNotFound {
            name: type_name::<T>(),
        }
    }
}

/// Try run a potentially async block of arguments with result type [`AccessError`],
/// always discard the result and does not log the error.
#[macro_export]
macro_rules! attempt {
    ($($tt:tt)*) => {
        let _: $crate::AccessResult<()> = async {
            let _ = {$($tt)*};
            Ok(())
        }.await;
    };
}
