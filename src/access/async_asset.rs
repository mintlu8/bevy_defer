use std::rc::Rc;
use bevy_asset::{Asset, AssetPath, AssetServer, Assets, Handle, LoadState};
use bevy_ecs::world::World;
use futures::future::{Either, ready};
use crate::sync::oneshot::{ChannelOut, MaybeChannelOut};
use crate::{AsyncFailure, AsyncResult};
use crate::{access::AsyncWorldMut, channel, queue::AsyncQueryQueue};
use crate::locals::with_asset_server;

/// Async version of [`Handle`].
#[derive(Debug, Clone)]
pub struct AsyncAsset<A: Asset>{
    pub(crate) queue: Rc<AsyncQueryQueue>,
    pub(crate) handle: Handle<A>,
}

impl AsyncWorldMut {
    
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
    pub fn asset<A: Asset>(
        &self, 
        handle: Handle<A>, 
    ) -> AsyncAsset<A> {
        AsyncAsset {
            queue: self.queue.clone(),
            handle,
        }
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
        AsyncAsset {
            queue: self.queue.clone(),
            handle: self.with_asset_server(|s| s.load::<A>(path)),
        }
    }

    /// Add an asset and obtain its handle.
    pub fn add_asset<A: Asset + 'static>(
        &self,
        item: A,
    ) -> ChannelOut<AsyncResult<Handle<A>>>{
        self.run(|w| 
            Ok(w.get_resource_mut::<Assets<A>>()
                .ok_or(AsyncFailure::ResourceNotFound)?
                .add(item))
        )
    }

}

impl<A: Asset> AsyncAsset<A> {
    /// Obtain the underlying [`Handle`].
    pub fn handle(&self) -> &Handle<A> {
        &self.handle
    }

    /// Obtain the underlying [`Handle`].
    pub fn into_handle(self) -> Handle<A> {
        self.handle
    }

    /// Repeat until the asset is loaded, returns false if loading failed.
    pub fn loaded (
        &self, 
    ) -> MaybeChannelOut<bool> {
        match with_asset_server(|server| {
            server.load_state(&self.handle)
        }) {
            LoadState::Loaded => return Either::Right(ready(true)),
            LoadState::Failed => return Either::Right(ready(false)),
            _ => (),
        };
        let (sender, receiver) = channel();
        let handle = self.handle.id();
        self.queue.repeat(
            move |world: &mut World| {
                match world.resource::<AssetServer>().load_state(handle){
                    LoadState::Loaded => Some(true),
                    LoadState::Failed => Some(false),
                    _ => None,
                }
            },
            sender
        );
        Either::Left(receiver.into_out())
    }
}
