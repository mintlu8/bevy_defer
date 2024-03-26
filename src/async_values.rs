use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::component::Component;
use bevy_ecs::{entity::Entity, world::World};
use bevy_ecs::system::{In, Resource, StaticSystemParam, SystemId, SystemParam};
use futures::future::{ready, Either};
use futures::FutureExt;
use std::future::Future;
use crate::async_world::AsyncWorldMut;
use crate::channels::channel;
use crate::locals::with_sync_world;
use crate::signals::Signals;
use crate::{async_systems::AsyncEntityParam, CHANNEL_CLOSED};

use super::{AsyncQueryQueue, AsyncFailure, QueryCallback, AsyncResult};

/// Async version of [`SystemParam`].
#[derive(Debug, Clone)]
pub struct AsyncSystemParam<P: SystemParam>{
    pub(crate) executor: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<P>
}

impl<P: SystemParam> AsyncEntityParam for AsyncSystemParam<P> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &AsyncWorldMut,
        _: (),
        _: &[Entity]
    ) -> Option<Self> {
        Some(AsyncSystemParam {
            executor: executor.queue.clone(),
            p: PhantomData
        })
    }
    
}

type SysParamFn<Q, T> = dyn Fn(StaticSystemParam<Q>) -> T + Send + Sync + 'static;

#[derive(Debug, Resource)]
struct ResSysParamId<P: SystemParam, T>(SystemId<Box<SysParamFn<P, T>>, T>);

impl<Q: SystemParam + 'static> AsyncSystemParam<Q> {
    /// Run a function on the [`SystemParam`] and obtain the result.
    pub fn run<T: Send + Sync + 'static>(&self,
        f: impl (Fn(StaticSystemParam<Q>) -> T) + Send + Sync + 'static
    ) -> impl Future<Output = AsyncResult<T>> + 'static{
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
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
                world.run_system_with_input(id, Box::new(f))
                    .map_err(|_| AsyncFailure::SystemParamError)
            },
            sender,
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
#[derive(Debug, Clone)]
pub struct AsyncComponent<C: Component>{
    pub(crate) entity: Entity,
    pub(crate) executor: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<C>
}

impl<C: Component> AsyncEntityParam for AsyncComponent<C> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        entity: Entity,
        executor: &AsyncWorldMut,
        _: (),
        _: &[Entity]
    ) -> Option<Self> {
        Some(Self {
            entity,
            executor: executor.queue.clone(),
            p: PhantomData
        })
    }
}

impl<C: Component> AsyncComponent<C> {

    /// Wait until a [`Component`] exists.
    pub fn exists(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                world
                    .get_entity(entity)?
                    .get::<C>()
                    .map(|_|())
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a function on the [`Component`] and obtain the result.
    /// 
    /// Guaranteed to complete immediately with `World` access.
    pub fn get<Out: 'static>(&self, f: impl FnOnce(&C) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let entity = self.entity;
        let f = move |world: &World| {
            Ok(f(world
                .get_entity(entity)
                .ok_or(AsyncFailure::EntityNotFound)?
                .get::<C>()
                .ok_or(AsyncFailure::ComponentNotFound)?))
        }; 
        let f = match with_sync_world(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        let (sender, receiver) = channel();
        let query = QueryCallback::once(|w|f(w), sender);
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }

    /// Run a function on the mutable [`Component`] and obtain the result.
    pub fn set<Out: 'static>(&self, f: impl FnOnce(&mut C) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        let query = QueryCallback::once(
            move |world: &mut World| {
                Ok(f(world
                    .get_entity_mut(entity)
                    .ok_or(AsyncFailure::EntityNotFound)?
                    .get_mut::<C>()
                    .ok_or(AsyncFailure::ComponentNotFound)?
                    .as_mut()))
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    /// Run a repeatable function on the [`Component`] and obtain the result once [`Some`] is returned.
    pub fn watch<Out: 'static>(&self, mut f: impl FnMut(&C) -> Option<Out> + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                (||{
                    let component = world
                        .get_entity(entity)
                        .ok_or(AsyncFailure::EntityNotFound)?
                        .get_ref::<C>()
                        .ok_or(AsyncFailure::ComponentNotFound)?;
                    if component.is_changed() {
                        Ok(f(&component))
                    } else {
                        Ok(None)
                    }
                })().transpose()
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}

#[allow(unused)]
pub use bevy_ecs::system::NonSend;

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug, Clone)]
pub struct AsyncNonSend<R: 'static>{
    pub(crate) executor: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<R>
}

impl<R: 'static> AsyncEntityParam for AsyncNonSend<R> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &AsyncWorldMut,
        _: (),
        _: &[Entity]
    ) -> Option<Self> {
        Some(Self {
            executor: executor.queue.clone(),
            p: PhantomData
        })
    }
}

impl<R: 'static> AsyncNonSend<R> {

    /// Wait until a [`NonSend`] exists.
    pub fn exists(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                world
                    .get_non_send_resource::<R>()
                    .map(|_|())
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a function on the [`NonSend`] and obtain the result.
    /// 
    /// Guaranteed to complete immediately with `World` access.
    pub fn get<Out: 'static>(&self, f: impl FnOnce(&R) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let f = move |world: &World| {
            world.get_non_send_resource::<R>()
                .map(f)
                .ok_or(AsyncFailure::ResourceNotFound)
        }; 
        let f = match with_sync_world(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        let (sender, receiver) = channel();
        let query = QueryCallback::once(|w|f(w), sender);
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }

    /// Run a function on the mutable [`NonSend`] and obtain the result.
    pub fn set<Out: 'static>(&self, f: impl FnOnce(&mut R) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.get_non_send_resource_mut::<R>()
                    .map(|mut x| f(x.as_mut()))
                    .ok_or(AsyncFailure::ResourceNotFound)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a repeatable function on the [`NonSend`] and obtain the result once [`Some`] is returned.
    pub fn watch<Out: 'static>(&self, mut f: impl FnMut(&R) -> Option<Out> + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let Some(res) = world.get_non_send_resource::<R>() 
                    else {return Some(Err(AsyncFailure::ResourceNotFound))};
                Ok(f(res)).transpose()
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}


/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug, Clone)]
pub struct AsyncResource<R: Resource>{
    pub(crate) executor: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<R>
}

impl<R: Resource> AsyncEntityParam for AsyncResource<R> {
    type Signal = ();
    
    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        _: Entity,
        executor: &AsyncWorldMut,
        _: (),
        _: &[Entity]
    ) -> Option<Self> {
        Some(Self {
            executor: executor.queue.clone(),
            p: PhantomData
        })
    }
}

impl<R: Resource> AsyncResource<R> {

    /// Wait until a [`Resource`] exists.
    pub fn exists(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                world
                    .get_resource::<R>()
                    .map(|_|())
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a function on the [`Resource`] and obtain the result.
    /// 
    /// Guaranteed to complete immediately with `World` access.
    pub fn get<Out: 'static>(&self, f: impl FnOnce(&R) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let f = move |world: &World| {
            world.get_resource::<R>()
                .map(f)
                .ok_or(AsyncFailure::ResourceNotFound)
        };
        let f = match with_sync_world(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        let (sender, receiver) = channel();
        let query = QueryCallback::once(|w|f(w), sender);
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }

    /// Run a function on the mutable [`Resource`] and obtain the result.
    pub fn set<Out: 'static>(&self, f: impl FnOnce(&mut R) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.get_resource_mut::<R>()
                    .map(|mut x| f(x.as_mut()))
                    .ok_or(AsyncFailure::ResourceNotFound)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a repeatable function on the [`Resource`] and obtain the result once [`Some`] is returned.
    pub fn watch<Out: 'static>(&self, mut f: impl FnMut(&R) -> Option<Out> + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let Some(res) = world.get_resource_ref::<R>() 
                    else {return Some(Err(AsyncFailure::ResourceNotFound))};
                if res.is_changed() {
                    Ok(f(&res)).transpose()
                } else {
                    None
                }
            },
            sender
        );
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}

/// Add method to [`AsyncComponent`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncComponentDeref: Component + Sized {
    type Target;
    fn async_deref(this: &AsyncComponent<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncComponent<C> where C: AsyncComponentDeref{
    type Target = <C as AsyncComponentDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncComponentDeref::async_deref(self)
    }
}

/// Add method to [`AsyncResource`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncResourceDeref: Resource + Sized {
    type Target;
    fn async_deref(this: &AsyncResource<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncResource<C> where C: AsyncResourceDeref{
    type Target = <C as AsyncResourceDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncResourceDeref::async_deref(self)
    }
}

/// Add method to [`AsyncNonSend`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncNonSendDeref: Resource + Sized {
    type Target;
    fn async_deref(this: &AsyncNonSend<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncNonSend<C> where C: AsyncNonSendDeref{
    type Target = <C as AsyncNonSendDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncNonSendDeref::async_deref(self)
    }
}