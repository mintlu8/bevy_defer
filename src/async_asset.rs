use std::{ops::Deref, rc::Rc};
use bevy_asset::{Asset, AssetPath, AssetServer, Assets, Handle, LoadState};
use bevy_ecs::world::World;
use futures::{future::Either, Future, FutureExt};
use std::future::ready;
use crate::{async_world::AsyncWorldMut, channel, queue::AsyncQueryQueue, locals::{with_asset_server, with_world_ref}, AsyncFailure, AsyncResult, CHANNEL_CLOSED};


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

    /// Clone an `Asset`, repeat until the asset is loaded.
    pub fn cloned(
        &self, 
    ) -> impl Future<Output = AsyncResult<A>> where A: Clone {
        self.get(Clone::clone)
    }

    /// Repeat until the asset is loaded, returns false if loading failed.
    pub fn loaded<T: 'static> (
        &self, 
    ) -> impl Future<Output = bool> {
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
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }

    /// Run a function on an `Asset` and obtain the result,
    /// repeat until the asset is loaded.
    pub fn get<T: 'static> (
        &self, 
        mut f: impl FnMut(&A) -> T + Send + 'static
    ) -> impl Future<Output = AsyncResult<T>> {
        if let Ok(Some(result)) = with_world_ref(|world| {
            world.get_resource::<Assets<A>>()
                .and_then(|x| x.get(&self.handle))
                .map(&mut f)
        }) {
            return Either::Right(ready(Ok(result)))
        };
        let (sender, receiver) = channel();
        let handle = self.handle.id();
        self.queue.repeat(
            move |world: &mut World| {
                let Some(assets) = world.get_resource::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.get(handle).map(|x|Ok(f(x)))
            },
            sender
        );
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }

    /// Remove an `Asset` and obtain it, repeat until the asset is loaded.
    pub fn take(
        &self, 
    ) -> impl Future<Output = AsyncResult<A>> {
        let (sender, receiver) = channel();
        let id = self.handle.id();
        self.queue.repeat(
            move |world: &mut World| {
                let Some(mut assets) = world.get_resource_mut::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.remove(id).map(Ok)
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}


/// Add method to [`AsyncAsset`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncAssetDeref: Asset + Sized {
    type Target;
    fn async_deref(this: &AsyncAsset<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncAsset<C> where C: AsyncAssetDeref{
    type Target = <C as AsyncAssetDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncAssetDeref::async_deref(self)
    }
}