use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::Deref;
use triomphe::Arc;
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::component::Component;
use bevy_ecs::{entity::Entity, world::World};
use bevy_ecs::system::{In, Resource, StaticSystemParam, SystemId, SystemParam};
use std::future::Future;
use async_oneshot::oneshot;

use super::{AsyncExecutor, AsyncFailure, BoxedQueryCallback, BoxedReadonlyCallback, AsyncResult, Signals};

/// A parameter of an `AsyncSystem`.
pub trait AsyncSystemParam: Sized {
    fn from_async_context(
        entity: Entity,
        executor: &Arc<AsyncExecutor>,
        signals: &Signals,
    ) -> Self;
}


/// Async version of [`SystemParam`].
#[derive(Debug)]
pub struct AsyncSystemParams<'t, P: SystemParam>{
    pub(crate) executor: Cow<'t, Arc<AsyncExecutor>>,
    pub(crate) p: PhantomData<P>
}

unsafe impl<P: SystemParam> Send for AsyncSystemParams<'_, P> {}
unsafe impl<P: SystemParam> Sync for AsyncSystemParams<'_, P> {}

impl<P: SystemParam> AsyncSystemParam for AsyncSystemParams<'_, P> {
    fn from_async_context(
        _: Entity,
        executor: &Arc<AsyncExecutor>,
        _: &Signals,
    ) -> Self {
        AsyncSystemParams {
            executor: Cow::Owned(executor.clone()),
            p: PhantomData
        }
    }
}

type SysParamFn<Q, T> = dyn Fn(StaticSystemParam<Q>) -> T + Send + Sync + 'static;

#[derive(Debug, Resource)]
struct ResSysParamId<P: SystemParam, T>(SystemId<Box<SysParamFn<P, T>>, T>);

impl<Q: SystemParam + 'static> AsyncSystemParams<'_, Q> {
    pub fn run<T: Send + Sync + 'static>(&self,
        f: impl (Fn(StaticSystemParam<Q>) -> T) + Send + Sync + 'static
    ) -> impl Future<Output = AsyncResult<T>> + Send + Sync + 'static{
        let (sender, receiver) = oneshot::<T>();
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
    pub(crate) executor: Cow<'t, Arc<AsyncExecutor>>,
    pub(crate) p: PhantomData<C>
}

impl<C: Component> AsyncSystemParam for AsyncComponent<'_, C> {
    fn from_async_context(
        entity: Entity,
        executor: &Arc<AsyncExecutor>,
        _: &Signals,
    ) -> Self {
        Self {
            entity,
            executor: Cow::Owned(executor.clone()),
            p: PhantomData
        }
    }
}

impl<C: Component> AsyncComponent<'_, C> {

    pub fn get<Out: Send + Sync + 'static>(&self, f: impl FnOnce(&C) -> Out + Send + Sync + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = oneshot::<Option<Out>>();
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
        let (sender, receiver) = oneshot::<Out>();
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
        let (sender, receiver) = oneshot::<Option<Out>>();
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
    pub(crate) executor: Cow<'t, Arc<AsyncExecutor>>,
    pub(crate) p: PhantomData<R>
}

impl<R: Resource> AsyncSystemParam for AsyncResource<'_, R> {
    fn from_async_context(
        _: Entity,
        executor: &Arc<AsyncExecutor>,
        _: &Signals,
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
        let (sender, receiver) = oneshot::<Option<Out>>();
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
        let (sender, receiver) = oneshot::<Option<Out>>();
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

/// The standard way to add method to [`AsyncComponent`].
///
/// It is recommended to derive [`RefCast`](https://docs.rs/ref-cast/latest/ref_cast/) for this.
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


/// The standard way to add method to [`AsyncResource`].
///
/// It is recommended to derive [`RefCast`](https://docs.rs/ref-cast/latest/ref_cast/) for this.
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