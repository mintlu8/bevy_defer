use std::{any::Any, borrow::Cow, error::Error, fmt::Display};

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
/// # /*
/// returns_a_custom_error().await?;
/// # */
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
    #[error("io error")]
    IO,
    #[error("custom error")]
    Custom,
    #[error("this error should not happen")]
    ShouldNotHappen,
}

/// An alternative [`AccessError`] with a custom error message.
/// All types implementing [`Error`] can propagate to [`CustomError`] via `?`.
/// Use [`Error::source`] to specify the associated `AccessError`, otherwise [`AccessError::Custom`].
/// If propagated to `AccessError` via `?`, will log the error message via `bevy_log`.
#[derive(Debug)]
pub struct CustomError {
    pub error: AccessError,
    pub message: Option<Box<dyn Error>>,
}

impl Display for CustomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.error, &self.message) {
            (AccessError::Custom, Some(message)) => Display::fmt(&message, f),
            (err, None) => Display::fmt(&err, f),
            (err, Some(message)) => {
                write!(f, "{}: {}", err, message)
            }
        }
    }
}

impl<E: Error + 'static> From<E> for CustomError {
    fn from(error: E) -> Self {
        if let Some(error) = (&error as &dyn Any).downcast_ref::<AccessError>() {
            CustomError {
                error: *error,
                message: None,
            }
        } else if let Some(e) = error.source().and_then(|e| e.downcast_ref::<AccessError>()) {
            let e = *e;
            CustomError {
                error: e,
                message: Some(Box::new(e)),
            }
        } else {
            CustomError {
                error: AccessError::Custom,
                message: Some(Box::new(error)),
            }
        }
    }
}

impl From<CustomError> for AccessError {
    fn from(val: CustomError) -> Self {
        if val.message.is_some() {
            error!("{}", val);
        }
        val.error
    }
}

/// A [`String`] error.
#[derive(Debug, Clone)]
pub struct MessageError(pub Cow<'static, str>);

impl MessageError {
    pub fn new(d: impl ToString) -> MessageError {
        MessageError(Cow::Owned(d.to_string()))
    }

    pub fn new_static(d: &'static str) -> MessageError {
        MessageError(Cow::Borrowed(d))
    }
}

impl Display for MessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_ref())
    }
}

impl Error for MessageError {}

impl AccessError {
    /// Create a [`CustomError`] with a message.
    pub fn with_message(&self, e: impl Error) -> CustomError {
        CustomError {
            error: *self,
            message: Some(Box::new(MessageError(Cow::Owned(e.to_string())))),
        }
    }

    /// Create a [`CustomError`] with a static string message.
    pub fn with_message_static(&self, e: &'static str) -> CustomError {
        CustomError {
            error: *self,
            message: Some(Box::new(MessageError(Cow::Borrowed(e)))),
        }
    }
}

impl CustomError {
    /// Returns true if is a specific [`AccessError`].
    pub fn is(&self, error: AccessError) -> bool {
        self.error == error
    }

    /// Create a [`CustomError`] with a message.
    pub fn with_type(self, error: AccessError) -> CustomError {
        CustomError {
            error,
            message: self.message,
        }
    }

    /// Create a [`CustomError`] with a message.
    pub fn with_message(&self, e: impl Error) -> CustomError {
        CustomError {
            error: self.error,
            message: Some(Box::new(MessageError(Cow::Owned(e.to_string())))),
        }
    }
}

/// Construct a [`CustomError`] from a [`AccessError`] and [`format!`] syntax.
#[macro_export]
macro_rules! format_error {
    ($str: literal $(,)?) => {
        $crate::CustomError {
            error: $crate::AccessError::Custom,
            message: Some(Box::new($crate::MessageError(::std::borrow::Cow::Borrowed($str)))),
        }
    };

    ($str: literal $(,$expr: expr)* $(,)?) => {
        $crate::CustomError {
            error: $crate::AccessError::Custom,
            message: Some(Box::new($crate::MessageError::new(
                format!($str $(,$expr)*)
            ))),
        }
    };
    ($variant: expr, $str: literal $(,)?) => {
        #[allow(unused_imports)]
        {
            use $crate::AccessError::*;
            $crate::CustomError {
                error: $variant,
                message: Some(Box::new($crate::MessageError::new_static($str))),
            }
        }

    };

    ($variant: expr, $str: literal $(,$expr: expr)* $(,)?) => {
        #[allow(unused_imports)]
        {
            use $crate::AccessError::*;
            $crate::CustomError {
                error: $variant,
                message: Some(Box::new($crate::MessageError::new(
                    format!($str $(,$expr)*)
                ))),
            }
        }
    };


    ($variant: expr, $expr: expr $(,)?) => {
        #[allow(unused_imports)]
        {
            use $crate::AccessError::*;
            $crate::CustomError {
                error: $variant,
                message: Some(Box::new($expr)),
            }
        }
    };
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

#[cfg(test)]
mod test {
    use crate::{AccessError, CustomError};

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
        format_error!(
            EntityNotFound,
            std::io::Error::new(std::io::ErrorKind::NotFound, "No!")
        );
        format_error!(
            my_error,
            std::io::Error::new(std::io::ErrorKind::NotFound, "No!")
        );
    }

    fn custom() -> Result<(), CustomError> {
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No!"))?;
        Err(format_error!(EntityNotFound, "{} is missing.", 42))?;
        Ok(())
    }

    fn main() -> Result<(), AccessError> {
        custom()?;
        Ok(())
    }

    #[test]
    fn test2() {
        let err = (|| -> Result<(), CustomError> {
            Err(AccessError::ChildNotFound)?;
            Ok(())
        })()
        .unwrap_err();
        assert!(err.is(AccessError::ChildNotFound));
        assert!(err.message.is_none());
    }
}
