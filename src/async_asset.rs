use std::{ops::Deref, rc::Rc};
use bevy_asset::{Asset, Assets, Handle};
use bevy_ecs::world::World;
use futures::{Future, FutureExt};

use crate::{channel, executor::AsyncQueryQueue, AsyncFailure, AsyncResult, QueryCallback, CHANNEL_CLOSED};


/// Async version of [`Handle`].
#[derive(Debug, Clone)]
pub struct AsyncAsset<A: Asset>{
    pub(crate) queue: Rc<AsyncQueryQueue>,
    pub(crate) handle: Handle<A>,
}

impl<A: Asset> AsyncAsset<A> {
    /// Obtain the underlying [`Handle`].
    pub fn handle(&self) -> &Handle<A> {
        &self.handle
    }

    /// Run a function on an `Asset` and obtain the result,
    /// repeat until the asset is loaded.
    pub fn get<T: 'static> (
        &self, 
        mut f: impl FnMut(&A) -> T + Send + 'static
    ) -> impl Future<Output = AsyncResult<T>> {
        let (sender, receiver) = channel();
        let handle = self.handle.id();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let Some(assets) = world.get_resource::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.get(handle).map(|x|Ok(f(x)))
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Clone an `Asset`, repeat until the asset is loaded.
    pub fn cloned(
        &self, 
    ) -> impl Future<Output = AsyncResult<A>> where A: Clone {
        let (sender, receiver) = channel();
        let id = self.handle.id();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let Some(assets) = world.get_resource_mut::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.get(id).cloned().map(Ok)
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Remove an `Asset` and obtain it, repeat until the asset is loaded.
    pub fn take(
        &self, 
    ) -> impl Future<Output = AsyncResult<A>> {
        let (sender, receiver) = channel();
        let id = self.handle.id();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let Some(mut assets) = world.get_resource_mut::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.remove(id).map(Ok)
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
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