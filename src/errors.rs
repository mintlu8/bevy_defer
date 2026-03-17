use crate::InspectEntity;
use bevy::ecs::entity::Entity;
use std::any::type_name;

#[cfg(feature = "full_types")]
fn fmt(s: &str) -> &str {
    s
}

#[cfg(not(feature = "full_types"))]
fn fmt(s: &str) -> String {
    pretty_type_name::pretty_type_name_str(s)
}

#[test]
fn split() {
    assert_eq!(
        split_tuple_2(std::any::type_name::<(u8, u8)>()),
        Some(("u8", "u8"))
    );
    assert_eq!(split_tuple_2("((u8, u8), u8)"), Some(("(u8, u8)", "u8")));
    assert_eq!(split_tuple_2("(u8, u8), u8"), None);
    assert_eq!(
        split_tuple_2("(A<B, C>, D<E, F, G>)"),
        Some(("A<B, C>", "D<E, F, G>"))
    );
}

fn split_tuple_2(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('(')?;
    let s = s.strip_suffix(')')?;
    let mut paren = 0usize;
    let mut brace = 0usize;
    let mut bracket = 0usize;
    let mut angle_bracket = 0usize;

    for (idx, c) in s.as_bytes().iter().copied().enumerate() {
        match c {
            b'(' => paren += 1,
            b')' => paren = paren.checked_sub(1)?,
            b'[' => bracket += 1,
            b']' => bracket = bracket.checked_sub(1)?,
            b'{' => brace += 1,
            b'}' => brace = brace.checked_sub(1)?,
            b'<' => angle_bracket += 1,
            b'>' => angle_bracket = angle_bracket.checked_sub(1)?,
            b',' => {
                if paren == 0 && brace == 0 && bracket == 0 && angle_bracket == 0 {
                    return Some((s.get(0..idx)?.trim(), s.get(idx + 1..)?.trim()));
                }
            }
            _ => (),
        }
    }
    None
}

#[cfg(feature = "full_types")]
fn fmt_from_to(s: &str) -> String {
    match split_tuple_2(s) {
        Some((from, to)) => {
            format!("from {} to {}", from, to,)
        }
        None => s.into(),
    }
}

#[cfg(not(feature = "full_types"))]
fn fmt_from_to(s: &str) -> String {
    match split_tuple_2(s) {
        Some((from, to)) => {
            format!(
                "from {} to {}",
                pretty_type_name::pretty_type_name_str(from),
                pretty_type_name::pretty_type_name_str(to),
            )
        }
        None => pretty_type_name::pretty_type_name_str(s),
    }
}

/// Standard errors for the async runtime.
///
/// # Design
///
/// This type is `Copy` and should be relatively cheap to include in hot paths.
///
/// # Error Logging
///
/// Consider the `instrument` macro from the `tracing` crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AccessError {
    #[error("entity {} not found", InspectEntity(*.0))]
    EntityNotFound(Entity),
    #[error("query condition {} not met for entity {}", fmt(query), InspectEntity(*entity))]
    QueryConditionNotMet { entity: Entity, query: &'static str },
    #[error("single entity not found in query {}", fmt(query))]
    NoEntityFound { query: &'static str },
    #[error("too many entities found in query {}", fmt(query))]
    TooManyEntities { query: &'static str },
    #[error("named child not found")]
    NamedChildNotFound,
    #[error("child index {index} missing")]
    ChildNotFound { index: usize },
    #[error("child of type {} missing", fmt(query))]
    TypedChildNotFound { query: &'static str },
    #[error("parent of type {} missing", fmt(query))]
    TypedParentNotFound { query: &'static str },
    #[error("component <{}> not found", fmt(name))]
    ComponentNotFound { name: &'static str },
    #[error("resource <{}> not found", fmt(name))]
    ResourceNotFound { name: &'static str },
    #[error("asset <{}> not found", fmt(name))]
    AssetNotFound { name: &'static str },
    #[error("event <{}> not registered", fmt(name))]
    EventNotRegistered { name: &'static str },
    /// # Note
    ///
    /// If downcasting from A to B, supply type name of `(From, To)` if possible.
    #[error("downcasting {} failed", fmt_from_to(name))]
    DowncastFailed { name: &'static str },
    #[error("schedule not found")]
    ScheduleNotFound,
    #[error("SystemId not found")]
    SystemIdNotFound,
    #[error("not in a state of type {}", fmt(ty))]
    NotInState { ty: &'static str },
    /// A custom message.
    #[error("custom error: {0}")]
    Custom(&'static str),
    /// A custom message with a type.
    #[error("typed error: {message} of type {}", fmt(ty))]
    TypedError {
        message: &'static &'static str,
        ty: &'static str,
    },
    /// Equivalent to `unreachable!`.
    #[error("this error should not have happened")]
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
