use bevy_ecs::query::{QueryEntityError, QuerySingleError};
use bevy_log::error;

/// Standard errors for the async runtime.
///
/// This type is designed to be match friendly but not necessarily carry all the debugging information.
/// It might me more correct to either match or unwrap this error instead of propagating it.
///
/// # Error Logging
/// 
/// Consider the `instrument` macro from the `tracing` crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AccessError {
    #[error("async channel closed")]
    ChannelClosed,
    #[error("entity not found")]
    EntityNotFound,
    #[error("too many entities")]
    TooManyEntities,
    #[error("child index missing")]
    ChildNotFound,
    #[error("component not found")]
    ComponentNotFound,
    #[error("resource not found")]
    ResourceNotFound,
    #[error("asset not found")]
    AssetNotFound,
    #[error("event not registered")]
    EventNotRegistered,
    #[error("signal not found")]
    SignalNotFound,
    #[error("schedule not found")]
    ScheduleNotFound,
    #[error("system param error")]
    SystemParamError,
    #[error("AsyncWorldParam not found")]
    WorldParamNotFound,
    #[error("SystemId not found")]
    SystemIdNotFound,
    #[error("Task spawned by `spawn_scoped` has panicked")]
    TaskPanicked,
    #[error("name not found")]
    NameNotFound,
    #[error("not in state")]
    NotInState,
    #[error("io error")]
    IO,
    #[error("custom error")]
    Custom,
    #[error("this error should not happen")]
    ShouldNotHappen,
}

impl From<QuerySingleError> for AccessError {
    fn from(value: QuerySingleError) -> Self {
        match value {
            QuerySingleError::NoEntities(_) => AccessError::EntityNotFound,
            QuerySingleError::MultipleEntities(_) => AccessError::TooManyEntities,
        }
    }
}

impl From<QueryEntityError> for AccessError {
    fn from(_: QueryEntityError) -> Self {
        AccessError::EntityNotFound
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
