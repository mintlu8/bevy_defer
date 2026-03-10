//! Asynchronous accessors to the `World`.
#[cfg(feature = "bevy_asset")]
mod as_asset;
pub mod deref;
mod impls;

#[cfg(feature = "bevy_asset")]
pub(crate) mod async_asset;
pub(crate) mod async_query;
pub(crate) mod async_values;
pub(crate) mod async_world;
pub(crate) mod child_query;
pub(crate) mod dyn_access;
#[cfg(feature = "bevy_asset")]
pub use as_asset::{AssetOf, GetHandle};
#[cfg(feature = "bevy_asset")]
pub use async_asset::AsyncAsset;
pub use async_query::{AsyncEntityQuery, AsyncQuery, AsyncQuerySingle};
pub use async_values::{AsyncComponent, AsyncNonSend, AsyncResource};
#[allow(deprecated)]
pub use async_world::AsyncChild;
pub use async_world::{AsyncEntityMut, AsyncWorld};
#[cfg(feature = "derive")]
pub use bevy_defer_derive::{AsyncComponent, AsyncNonSend, AsyncResource};
pub use child_query::{AsyncRelatedQuery, RelatedQueryState};
pub use dyn_access::DynAccess;
