//! Asynchronous accessors to the `World`.
mod as_asset;
pub mod deref;
mod impls;

pub(crate) mod async_asset;
pub(crate) mod async_query;
pub(crate) mod async_values;
pub(crate) mod async_world;
pub(crate) mod child_query;
pub(crate) mod dyn_access;
pub(crate) mod get_entity;
pub(crate) mod query;
pub use as_asset::{AssetOf, GetHandle};
pub use async_asset::AsyncAsset;
pub use async_query::{AsyncEntityQuery, AsyncQuery, AsyncQuerySingle};
pub use async_values::{AsyncComponent, AsyncNonSend, AsyncResource};
pub use async_world::{AsyncEntity, AsyncWorld};
use bevy::ecs::entity::Entity;
#[cfg(feature = "derive")]
pub use bevy_defer_derive::{AsyncComponent, AsyncNonSend, AsyncResource};
pub use child_query::{AsyncRelatedQuery, RelatedQueryState};
// pub use dyn_access::DynAccess;
pub use get_entity::{FilterChild, GetParent, IndexedChild, NamedChild, VirtualEntity};
#[deprecated = "Use AsyncEntity or AsyncEntity<Entity>."]
pub type AsyncEntityMut = AsyncEntity<Entity>;
