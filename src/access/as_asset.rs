use std::borrow::Borrow;

use super::{AsyncAsset, AsyncComponent, AsyncNonSend, AsyncResource, AsyncWorld};
use crate::{AccessResult, FetchEntity, FetchWorld};
use bevy::{
    asset::{Asset, AssetId, Handle},
    prelude::{Component, Entity, Resource},
};

/// Types, usually a [`Component`], that contains a single [`Handle`].
pub trait GetHandle {
    type Asset: Asset;
    fn get_asset_id(&self) -> AssetId<Self::Asset>;
    fn get_handle(&self) -> Handle<Self::Asset>;
}

impl<C: Component + GetHandle> AsyncComponent<C> {
    /// Obtain the underlying [`Asset`] according to [`GetHandle`].
    pub fn asset(&self) -> AccessResult<AsyncAsset<C::Asset>> {
        Ok(AsyncAsset::Strong(self.get(|x| x.get_handle())?))
    }
}

impl<C: Resource + GetHandle> AsyncResource<C> {
    /// Obtain the underlying [`Asset`] according to [`GetHandle`].
    pub fn asset(&self) -> AccessResult<AsyncAsset<C::Asset>> {
        Ok(AsyncAsset::Strong(self.get(|x| x.get_handle())?))
    }
}

impl<C: GetHandle> AsyncNonSend<C> {
    /// Obtain the underlying [`Asset`] according to [`GetHandle`].
    pub fn asset(&self) -> AccessResult<AsyncAsset<C::Asset>> {
        Ok(AsyncAsset::Strong(self.get(|x| x.get_handle())?))
    }
}

/// When used in [`fetch!`](crate::fetch!), obtains the underlying [`AsyncAsset`].
pub struct AssetOf<T>(T);

impl<T: Component + GetHandle> FetchEntity for AssetOf<T> {
    type Out = AccessResult<AsyncAsset<T::Asset>>;

    fn fetch(entity: &impl Borrow<Entity>) -> Self::Out {
        AsyncWorld.entity(*entity.borrow()).component::<T>().asset()
    }
}

impl<T: Resource + GetHandle> FetchWorld for AssetOf<T> {
    type Out = AccessResult<AsyncAsset<T::Asset>>;

    fn fetch() -> Self::Out {
        AsyncWorld.resource::<T>().asset()
    }
}

#[cfg(feature = "bevy_render")]
impl GetHandle for bevy::prelude::Mesh2d {
    type Asset = bevy::prelude::Mesh;

    fn get_asset_id(&self) -> AssetId<Self::Asset> {
        self.0.id()
    }

    fn get_handle(&self) -> Handle<Self::Asset> {
        self.0.clone()
    }
}

#[cfg(feature = "bevy_render")]
impl GetHandle for bevy::prelude::Mesh3d {
    type Asset = bevy::prelude::Mesh;

    fn get_asset_id(&self) -> AssetId<Self::Asset> {
        self.0.id()
    }

    fn get_handle(&self) -> Handle<Self::Asset> {
        self.0.clone()
    }
}

#[cfg(feature = "bevy_sprite")]
impl<M: bevy::sprite_render::Material2d> GetHandle for bevy::prelude::MeshMaterial2d<M> {
    type Asset = M;

    fn get_asset_id(&self) -> AssetId<Self::Asset> {
        self.0.id()
    }

    fn get_handle(&self) -> Handle<Self::Asset> {
        self.0.clone()
    }
}

#[cfg(feature = "bevy_pbr")]
impl<M: bevy::pbr::Material> GetHandle for bevy::pbr::MeshMaterial3d<M> {
    type Asset = M;

    fn get_asset_id(&self) -> AssetId<Self::Asset> {
        self.0.id()
    }

    fn get_handle(&self) -> Handle<Self::Asset> {
        self.0.clone()
    }
}

#[cfg(feature = "bevy_sprite")]
impl GetHandle for bevy::sprite::Sprite {
    type Asset = bevy::image::Image;

    fn get_asset_id(&self) -> AssetId<Self::Asset> {
        self.image.id()
    }

    fn get_handle(&self) -> Handle<Self::Asset> {
        self.image.clone()
    }
}

#[cfg(feature = "bevy_sprite")]
impl GetHandle for bevy::image::TextureAtlas {
    type Asset = bevy::image::TextureAtlasLayout;

    fn get_asset_id(&self) -> AssetId<Self::Asset> {
        self.layout.id()
    }

    fn get_handle(&self) -> Handle<Self::Asset> {
        self.layout.clone()
    }
}

#[cfg(feature = "bevy_text")]
impl GetHandle for bevy::text::TextFont {
    type Asset = bevy::text::Font;

    fn get_asset_id(&self) -> AssetId<Self::Asset> {
        self.font.id()
    }

    fn get_handle(&self) -> Handle<Self::Asset> {
        self.font.clone()
    }
}
