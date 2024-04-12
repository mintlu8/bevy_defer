
//! Asynchronous accessors to the `World`.
pub mod traits;
pub mod deref;

pub(crate) mod async_world;
pub(crate) mod async_values;
pub(crate) mod async_query;
pub(crate) mod async_asset;
pub(crate) mod async_event;
pub use async_world::{AsyncWorld, AsyncWorldMut, AsyncEntityMut, AsyncChild};
pub use async_values::{AsyncComponent, AsyncResource, AsyncNonSend, AsyncSystemParam};
pub use async_query::{AsyncQuery, AsyncQuerySingle, AsyncEntityQuery};
pub use async_asset::AsyncAsset;
pub use async_event::EventStream;
pub use crate::ext::AsyncScene;