use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::Deref;
use triomphe::Arc;
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::component::Component;
use bevy_ecs::{entity::Entity, world::World};
use bevy_ecs::system::{In, Resource, StaticSystemParam, SystemId, SystemParam};
use std::future::Future;
use futures::channel::oneshot::channel;
use crate::signals::Signals;
use crate::AsyncEntityParam;

use super::{AsyncQueryQueue, AsyncFailure, BoxedQueryCallback, BoxedReadonlyCallback, AsyncResult};

/// Async version of [`SystemParam`].
#[derive(Debug)]
pub struct AsyncSystemParam<'t, P: SystemParam>{
    pub(crate) executor: Cow<'t, Arc<AsyncQueryQueue>>,
    pub(crate) p: PhantomData<P>
}

/// Safety: Safe since `P` is a marker.
unsafe impl<P: SystemParam> Send for AsyncSystemParam<'_, P> {}
/// Safety: Safe since `P` is a marker.
unsafe impl<P: SystemParam> Sync for AsyncSystemParam<'_, P> {}

impl<'t, P: SystemParam> AsyncEntityParam<'t> for AsyncSystemParam<'t, P> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &'t Arc<AsyncQueryQueue>,
        _: ()
    ) -> Self {
        AsyncSystemParam {
            executor: Cow::Borrowed(executor),
            p: PhantomData
        }
    }
    
}

type SysParamFn<Q, T> = dyn Fn(StaticSystemParam<Q>) -> T + Send + Sync + 'static;

#[derive(Debug, Resource)]
struct ResSysParamId<P: SystemParam, T>(SystemId<Box<SysParamFn<P, T>>, T>);

impl<Q: SystemParam + 'static> AsyncSystemParam<'_, Q> {
    pub fn run<T: Send + Sync + 'static>(&self,
        f: impl (Fn(StaticSystemParam<Q>) -> T) + Send + Sync + 'static
    ) -> impl Future<Output = AsyncResult<T>> + Send + Sync + 'static{
        let (sender, receiver) = channel::<T>();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                let id = match world.get_resource::<ResSysParamId<Q, T>>(){
                    Some(res) => res.0,
                    None => {
                        let id = world.register_system(
                            |input: In<Box<SysParamFn<Q, T>>>, query: StaticSystemParam<Q>| -> T{
                                (input.0)(query)
                            }
                        );
                        world.insert_resource(ResSysParamId(id));
                        id
                    },
                };
                world.run_system_with_input(id, Box::new(f)).unwrap()
            },
            sender,
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.map_err(|_|AsyncFailure::ChannelClosed)
        }
    }
}

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
#[derive(Debug)]
pub struct AsyncComponent<'t, C: Component>{
    pub(crate) entity: Entity,
    pub(crate) executor: Cow<'t, Arc<AsyncQueryQueue>>,
    pub(crate) p: PhantomData<C>
}

/// Safety: Safe since `C` is a marker.
unsafe impl<C: Component> Send for AsyncComponent<'_, C> {}
/// Safety: Safe since `C` is a marker.
unsafe impl<C: Component> Sync for AsyncComponent<'_, C> {}

impl<'t, C: Component> AsyncEntityParam<'t> for AsyncComponent<'t, C> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        entity: Entity,
        executor: &'t Arc<AsyncQueryQueue>,
        _: ()
    ) -> Self {
        Self {
            entity,
            executor: Cow::Borrowed(executor),
            p: PhantomData
        }
    }
}

impl<C: Component> AsyncComponent<'_, C> {

    pub fn get<Out: Send + Sync + 'static>(&self, f: impl FnOnce(&C) -> Out + Send + Sync + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel::<Option<Out>>();
        let entity = self.entity;
        let query = BoxedReadonlyCallback::new(
            move |world: &World| {
                world.entity(entity)
                    .get::<C>()
                    .map(f)
            },
            sender
        );
        {
            let mut lock = self.executor.readonly.lock();
            lock.push(query);
        }
        async {
            match receiver.await {
                Ok(Some(out)) => Ok(out),
                Ok(None) => Err(AsyncFailure::ComponentNotFound),
                Err(_) => Err(AsyncFailure::ChannelClosed),
            }
        }
    }

    pub fn watch<Out: Send + Sync + 'static>(&self, f: impl Fn(&C) -> Out + Send + Sync + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel::<Out>();
        let entity = self.entity;
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                world.entity_mut(entity)
                    .get_ref::<C>()
                    .and_then(|r| if r.is_changed() {
                        Some(f(r.as_ref()))
                    } else {
                        None
                    })
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.map_err(|_| AsyncFailure::ChannelClosed)
        }
    }

    pub fn set<Out: Send + Sync + 'static>(&self, f: impl FnOnce(&mut C) -> Out + Send + Sync + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel::<Option<Out>>();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.entity_mut(entity)
                    .get_mut::<C>()
                    .map(|mut x| f(x.as_mut()))
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            match receiver.await {
                Ok(Some(out)) => Ok(out),
                Ok(None) => Err(AsyncFailure::ComponentNotFound),
                Err(_) => Err(AsyncFailure::ChannelClosed),
            }
        }
    }
}

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug)]
pub struct AsyncResource<'t, R: Resource>{
    pub(crate) executor: Cow<'t, Arc<AsyncQueryQueue>>,
    pub(crate) p: PhantomData<R>
}

/// Safety: Safe since `R` is a marker.
unsafe impl<R: Resource> Send for AsyncResource<'_, R> {}
/// Safety: Safe since `R` is a marker.
unsafe impl<R: Resource> Sync for AsyncResource<'_, R> {}

impl<'t, R: Resource> AsyncEntityParam<'t> for AsyncResource<'t, R> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &Arc<AsyncQueryQueue>,
        _: ()
    ) -> Self {
        Self {
            executor: Cow::Owned(executor.clone()),
            p: PhantomData
        }
    }
}

impl<R: Resource> AsyncResource<'_, R> {
    pub fn get<Out: Send + Sync + 'static>(&self, f: impl FnOnce(&R) -> Out + Send + Sync + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel::<Option<Out>>();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_resource::<R>().map(f)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            match receiver.await {
                Ok(Some(out)) => Ok(out),
                Ok(None) => Err(AsyncFailure::ResourceNotFound),
                Err(_) => Err(AsyncFailure::ChannelClosed),
            }
        }
    }

    pub fn set<Out: Send + Sync + 'static>(&self, f: impl FnOnce(&mut R) -> Out + Send + Sync + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel::<Option<Out>>();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_resource_mut::<R>()
                    .map(|mut x| f(x.as_mut()))
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            match receiver.await {
                Ok(Some(out)) => Ok(out),
                Ok(None) => Err(AsyncFailure::ComponentNotFound),
                Err(_) => Err(AsyncFailure::ChannelClosed),
            }
        }
    }
}

/// Add method to [`AsyncComponent`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncComponentDeref: Component + Sized {
    type Target<'t>;
    fn async_deref<'a, 'b>(this: &'b AsyncComponent<'a, Self>) -> &'b Self::Target<'a>;
}

impl<'t, C> Deref for AsyncComponent<'t, C> where C: AsyncComponentDeref{
    type Target = <C as AsyncComponentDeref>::Target<'t>;

    fn deref(&self) -> &Self::Target {
        AsyncComponentDeref::async_deref(self)
    }
}

/// Add method to [`AsyncResource`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncResourceDeref: Resource + Sized {
    type Target;
    fn async_deref<'t>(this: &'t AsyncResource<Self>) -> &'t Self::Target;
}

impl<C> Deref for AsyncResource<'_, C> where C: AsyncResourceDeref{
    type Target = <C as AsyncResourceDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncResourceDeref::async_deref(self)
    }
}