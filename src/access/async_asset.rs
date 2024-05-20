use crate::access::AsyncWorld;
use crate::executor::{with_world_mut, ASSET_SERVER};
use crate::sync::oneshot::MaybeChannelOut;
use crate::{AccessError, AccessResult};
use bevy_asset::meta::Settings;
use bevy_asset::{Asset, AssetPath, AssetServer, Assets, Handle, LoadState};
use bevy_ecs::world::World;
use futures::future::{ready, Either};

/// Async version of [`Handle`].
#[derive(Debug)]
pub struct AsyncAsset<A: Asset>(pub(crate) Handle<A>);

impl<A: Asset> Clone for AsyncAsset<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl AsyncWorld {
    /// Obtain an [`AsyncAsset`] from a [`Handle`].
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// let square = world().load_asset::<Image>("square.png");
    /// world().asset(square.into_handle());
    /// # });
    /// ```
    pub fn asset<A: Asset>(&self, handle: Handle<A>) -> AsyncAsset<A> {
        AsyncAsset(handle)
    }

    /// Load an asset from an [`AssetPath`], equivalent to `AssetServer::load`.
    /// Does not wait for `Asset` to be loaded.
    ///
    /// # Panics
    ///
    /// If `AssetServer` does not exist in the world.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// let square = world().load_asset::<Image>("square.png");
    /// # });
    /// ```
    pub fn load_asset<A: Asset>(
        &self,
        path: impl Into<AssetPath<'static>> + Send + 'static,
    ) -> AsyncAsset<A> {
        if !ASSET_SERVER.is_set() {
            panic!("AssetServer does not exist.")
        }
        AsyncAsset(ASSET_SERVER.with(|s| s.load::<A>(path)))
    }

    /// Begins loading an Asset of type `A` stored at path.
    /// The given settings function will override the asset's AssetLoader settings.
    pub fn load_asset_with_settings<A: Asset, S: Settings>(
        &self,
        path: impl Into<AssetPath<'static>> + Send + 'static,
        f: impl Fn(&mut S) + Send + Sync + 'static,
    ) -> AsyncAsset<A> {
        if !ASSET_SERVER.is_set() {
            panic!("AssetServer does not exist.")
        }
        AsyncAsset(ASSET_SERVER.with(|s| s.load_with_settings::<A, S>(path, f)))
    }

    /// Add an asset and obtain its handle.
    pub fn add_asset<A: Asset + 'static>(&self, item: A) -> AccessResult<Handle<A>> {
        with_world_mut(|w| {
            Ok(w.get_resource_mut::<Assets<A>>()
                .ok_or(AccessError::ResourceNotFound)?
                .add(item))
        })
    }
}

impl<A: Asset> AsyncAsset<A> {
    /// Obtain the underlying [`Handle`].
    pub fn handle(&self) -> &Handle<A> {
        &self.0
    }

    /// Obtain the underlying [`Handle`].
    pub fn into_handle(self) -> Handle<A> {
        self.0
    }

    /// Repeat until the asset is loaded, returns false if loading failed.
    pub fn loaded(&self) -> MaybeChannelOut<bool> {
        if !ASSET_SERVER.is_set() {
            panic!("AssetServer does not exist.")
        }
        match ASSET_SERVER.with(|server| server.load_state(&self.0)) {
            LoadState::Loaded => return Either::Right(ready(true)),
            LoadState::Failed => return Either::Right(ready(false)),
            _ => (),
        };
        let handle = self.0.id();
        AsyncWorld.watch_left(move |world: &mut World| {
            match world.resource::<AssetServer>().load_state(handle) {
                LoadState::Loaded => Some(true),
                LoadState::Failed => Some(false),
                _ => None,
            }
        })
    }
}
