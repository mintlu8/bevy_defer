//! Asynchronous accessors to the `World`.
mod as_asset;
pub mod deref;
pub mod traits;

pub(crate) mod async_asset;
pub(crate) mod async_event;
pub(crate) mod async_query;
pub(crate) mod async_values;
pub(crate) mod async_world;
pub use as_asset::{AsAssetId, AssetOf};
pub use async_asset::AsyncAsset;
pub use async_event::EventStream;
pub use async_query::{AsyncEntityQuery, AsyncQuery, AsyncQuerySingle};
pub use async_values::{AsyncComponent, AsyncNonSend, AsyncResource};
pub use async_world::{AsyncChild, AsyncEntityMut, AsyncWorld};

#[cfg(feature = "derive")]
pub use bevy_defer_derive::{AsyncComponent, AsyncNonSend, AsyncResource};
