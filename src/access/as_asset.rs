use std::borrow::Borrow;

use super::{AsyncAsset, AsyncComponent, AsyncNonSend, AsyncResource, AsyncWorld};
use crate::{AccessResult, AsyncAccess, FetchEntity, FetchWorld};
use bevy::{
    asset::{Asset, AssetId, Handle},
    prelude::{Component, Entity, Resource},
};

/// Obtain an underlying [`Asset`] of a component.
pub trait AsAssetId {
    type Asset: Asset;
    fn as_asset_id(&self) -> AssetId<Self::Asset>;
}

impl<C: Component + AsAssetId> AsyncComponent<C> {
    /// Obtain the underlying [`Asset`] according to [`AsAssetId`].
    pub fn asset(&self) -> AccessResult<AsyncAsset<C::Asset>> {
        Ok(AsyncAsset(Handle::Weak(self.get(|x| x.as_asset_id())?)))
    }
}

impl<C: Resource + AsAssetId> AsyncResource<C> {
    /// Obtain the underlying [`Asset`] according to [`AsAssetId`].
    pub fn asset(&self) -> AccessResult<AsyncAsset<C::Asset>> {
        Ok(AsyncAsset(Handle::Weak(self.get(|x| x.as_asset_id())?)))
    }
}

impl<C: AsAssetId> AsyncNonSend<C> {
    /// Obtain the underlying [`Asset`] according to [`AsAssetId`].
    pub fn asset(&self) -> AccessResult<AsyncAsset<C::Asset>> {
        Ok(AsyncAsset(Handle::Weak(self.get(|x| x.as_asset_id())?)))
    }
}

/// When used in [`fetch!`](crate::fetch!), obtains the underlying [`AsyncAsset`].
pub struct AssetOf<T>(T);

impl<T: Component + AsAssetId> FetchEntity for AssetOf<T> {
    type Out = AccessResult<AsyncAsset<T::Asset>>;

    fn fetch(entity: &impl Borrow<Entity>) -> Self::Out {
        AsyncWorld.entity(*entity.borrow()).component::<T>().asset()
    }
}

impl<T: Resource + AsAssetId> FetchWorld for AssetOf<T> {
    type Out = AccessResult<AsyncAsset<T::Asset>>;

    fn fetch() -> Self::Out {
        AsyncWorld.resource::<T>().asset()
    }
}

#[cfg(feature = "bevy_render")]
impl AsAssetId for bevy::render::mesh::Mesh2d {
    type Asset = bevy::render::mesh::Mesh;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.0.id()
    }
}

#[cfg(feature = "bevy_render")]
impl AsAssetId for bevy::render::mesh::Mesh3d {
    type Asset = bevy::render::mesh::Mesh;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.0.id()
    }
}

#[cfg(feature = "bevy_sprite")]
impl<M: bevy::sprite::Material2d> AsAssetId for bevy::sprite::MeshMaterial2d<M> {
    type Asset = M;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.0.id()
    }
}

#[cfg(feature = "bevy_pbr")]
impl<M: bevy::pbr::Material> AsAssetId for bevy::pbr::MeshMaterial3d<M> {
    type Asset = M;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.0.id()
    }
}

#[cfg(feature = "bevy_sprite")]
impl AsAssetId for bevy::sprite::Sprite {
    type Asset = bevy::render::texture::Image;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.image.id()
    }
}

#[cfg(feature = "bevy_text")]
impl AsAssetId for bevy::text::TextFont {
    type Asset = bevy::text::Font;

    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.font.id()
    }
}
