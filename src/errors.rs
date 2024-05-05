use std::{borrow::Cow, error::Error, fmt::Display};

use bevy_log::error;

/// Standard errors for the async runtime.
///
/// This type is designed to be match friendly but not necessarily carry all the debugging information.
/// It might me more correct to either match or unwrap this error instead of propagating it.
/// 
/// ## Logging Pattern
/// 
/// For custom errors, convert to `AccessError` via `Into` and log the message
/// via `bevy_log` in the `into` implementation.
/// 
/// ```
/// returns_a_custom_error().await?;
/// ```
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
    #[error("name not found")]
    NameNotFound,
    #[error("custom error")]
    Custom,
    #[error("this error should not happen")]
    ShouldNotHappen,
}

/// An alternative [`AccessError`] with a custom error message. 
/// If propagated to `AccessError` via `?`, will log the error message via `bevy_log`.
#[derive(Debug, Clone)]
pub struct CustomError {
    pub error: AccessError,
    pub message: Cow<'static, str>,
}

impl Display for CustomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.error {
            AccessError::Custom => {
                f.write_str(&self.message)
            },
            err => {
                write!(f, "{}: {}", err, self.message)
            }
        }   
    }
}

impl Error for CustomError {}

impl From<AccessError> for CustomError {
    fn from(val: AccessError) -> Self {
        CustomError {
            error: val,
            message: Cow::Owned(String::new()),
        }
    }
}

impl From<CustomError> for AccessError {
    fn from(val: CustomError) -> Self {
        error!("{}", val);
        val.error
    }
}

impl AccessError {
    /// Create a [`CustomError`] with a message.
    pub fn with_message(&self, e: impl Display) -> CustomError {
        CustomError {
            error: *self,
            message: Cow::Owned(e.to_string()),
        }
    }

    /// Create a [`CustomError`] with a static string message.
    pub const fn with_message_static(&self, e: &'static str) -> CustomError {
        CustomError {
            error: *self,
            message: Cow::Borrowed(e),
        }
    }
}

/// Error that has a `ManuallyKilled` component.
#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("{0}")]
    AccessError(#[from] AccessError),
    /// Return `Err(ManuallyKilled)` to terminate a `system_future!` future.
    #[error("Manually killed.")]
    ManuallyKilled,
}

impl From<SystemError> for AccessError {
    fn from(value: SystemError) -> Self {
        match value {
            SystemError::AccessError(e) => e,
            SystemError::ManuallyKilled => AccessError::ShouldNotHappen,
        }
    }
}

/// Construct a [`CustomError`] from a [`AccessError`] and [`format!`] syntax.
#[macro_export]
macro_rules! format_error {
    ($str: literal $(,)?) => {
        $crate::CustomError {
            error: $crate::AccessError::Custom,
            message: ::std::borrow::Cow::Borrowed($str),
        }
    };

    ($str: literal $(,$expr: expr)* $(,)?) => {
        $crate::CustomError {
            error: $crate::AccessError::Custom,
            message: ::std::borrow::Cow::Owned(
                format!($str $(,$expr)*)
            ),
        }
    };
    ($variant: expr, $str: literal $(,)?) => {
        #[allow(unused_imports)]
        {
            use $crate::AccessError::*;
            $crate::CustomError {
                error: $variant,
                message: ::std::borrow::Cow::Borrowed($str),
            }
        }
        
    };

    ($variant: expr, $str: literal $(,$expr: expr)* $(,)?) => {
        #[allow(unused_imports)]
        {
            use $crate::AccessError::*;
            $crate::CustomError {
                error: $variant,
                message: ::std::borrow::Cow::Owned(
                    format!($str $(,$expr)*)
                ),
            }
        }
    };
}

#[cfg(test)]
mod test {
    use crate::AccessError;

    #[test]
    fn test() {
        format_error!("Hello");
        format_error!("Hello{}", "world");
        let world = 32;
        format_error!("Hello{}", world);
        format_error!(EntityNotFound, "Sword is missing.");
        let sword = "sword";
        format_error!(EntityNotFound, "{} is missing.", sword);
        let my_error = AccessError::EntityNotFound;
        format_error!(my_error, "{} is missing.", sword);
    }
}