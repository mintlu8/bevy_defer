use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::Rc;
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::component::Component;
use bevy_ecs::{entity::Entity, world::World};
use bevy_ecs::system::{In, Resource, StaticSystemParam, SystemId, SystemParam};
use futures::future::{ready, Either};
use futures::FutureExt;
use ref_cast::RefCast;
use std::future::Future;
use crate::async_world::AsyncWorldMut;
use crate::channels::channel;
use crate::signals::Signals;
use crate::{async_systems::AsyncEntityParam, CHANNEL_CLOSED};

use super::{AsyncQueryQueue, AsyncFailure, AsyncResult};

/// Async version of [`SystemParam`].
#[derive(Debug, Clone)]
pub struct AsyncSystemParam<P: SystemParam>{
    pub(crate) queue: Rc<AsyncQueryQueue>,
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
            queue: executor.queue.clone(),
            p: PhantomData
        })
    }
    
}

type SysParamFn<Q, T> = dyn Fn(StaticSystemParam<Q>) -> T + Send + Sync + 'static;

#[derive(Debug, Resource)]
struct ResSysParamId<P: SystemParam, T>(SystemId<Box<SysParamFn<P, T>>, T>);

impl<Q: SystemParam + 'static> AsyncSystemParam<Q> {
    /// Obtain the underlying [`AsyncWorldMut`]
    pub fn world(&self) -> AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue).clone()
    }

    /// Run a function on the [`SystemParam`] and obtain the result.
    pub fn run<T: Send + Sync + 'static>(&self,
        f: impl (Fn(StaticSystemParam<Q>) -> T) + Send + Sync + 'static
    ) -> impl Future<Output = AsyncResult<T>> + 'static{
        let (sender, receiver) = channel();
        self.queue.once(
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
#[derive(Debug, Clone)]
pub struct AsyncComponent<C: Component>{
    pub(crate) entity: Entity,
    pub(crate) queue: Rc<AsyncQueryQueue>,
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
            queue: executor.queue.clone(),
            p: PhantomData
        })
    }
}

impl<C: Component> AsyncComponent<C> {

    /// Obtain the underlying [`AsyncWorldMut`]
    pub fn world(&self) -> AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue).clone()
    }
    
    /// Wait until a [`Component`] exists.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(4)).await.id();
    /// world().entity(entity).component::<Int>()
    ///     .exists().await;
    /// # });
    /// ```
    pub fn exists(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.repeat(
            move |world: &mut World| {
                world
                    .get_entity(entity)?
                    .get::<C>()
                    .map(|_|())
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a function on the [`Component`] and obtain the result.
    /// 
    /// Guaranteed to complete immediately with `World` access.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(4)).await.id();
    /// world().entity(entity).component::<Int>()
    ///     .get(|x| x.0).await?;
    /// # });
    /// ```
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
        let f = match self.world().with_world_ref(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        let (sender, receiver) = channel();
        self.queue.once(|w|f(w), sender);
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }

    /// Run a function on the mutable [`Component`] and obtain the result.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(4)).await.id();
    /// world().entity(entity).component::<Int>()
    ///     .set(|x| x.0 = 16).await?;
    /// # });
    /// ```
    pub fn set<Out: 'static>(&self, f: impl FnOnce(&mut C) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.once(
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
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    /// Run a repeatable function on the [`Component`] and obtain the result once [`Some`] is returned.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// # let entity = world().spawn_bundle(Int(4)).await.id();
    /// world().entity(entity).component::<Int>()
    ///     .watch(|x| (x.0 == 4).then_some(())).await?;
    /// # });
    pub fn watch<Out: 'static>(&self, mut f: impl FnMut(&C) -> Option<Out> + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        let entity = self.entity;
        self.queue.repeat(
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
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}

#[allow(unused)]
pub use bevy_ecs::system::NonSend;

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug, Clone)]
pub struct AsyncNonSend<R: 'static>{
    pub(crate) queue: Rc<AsyncQueryQueue>,
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
            queue: executor.queue.clone(),
            p: PhantomData
        })
    }
}

impl<R: 'static> AsyncNonSend<R> {

    /// Obtain the underlying [`AsyncWorldMut`]
    pub fn world(&self) -> AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue).clone()
    }

    /// Wait until a [`NonSend`] exists.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// world().non_send_resource::<Int>().exists().await;
    /// # });
    pub fn exists(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        self.queue.repeat(
            move |world: &mut World| {
                world
                    .get_non_send_resource::<R>()
                    .map(|_|())
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a function on the [`NonSend`] and obtain the result.
    /// 
    /// Guaranteed to complete immediately with `World` access.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// world().non_send_resource::<Int>().get(|x| x.0).await?;
    /// # });
    pub fn get<Out: 'static>(&self, f: impl FnOnce(&R) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let f = move |world: &World| {
            world.get_non_send_resource::<R>()
                .map(f)
                .ok_or(AsyncFailure::ResourceNotFound)
        }; 
        let f = match self.world().with_world_ref(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        let (sender, receiver) = channel();
        self.queue.once(|w|f(w), sender);
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }

    /// Run a function on the mutable [`NonSend`] and obtain the result.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// world().non_send_resource::<Int>().set(|x| x.0 = 16).await?;
    /// # });
    pub fn set<Out: 'static>(&self, f: impl FnOnce(&mut R) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        self.queue.once(
            move |world: &mut World| {
                world.get_non_send_resource_mut::<R>()
                    .map(|mut x| f(x.as_mut()))
                    .ok_or(AsyncFailure::ResourceNotFound)
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a repeatable function on the [`NonSend`] and obtain the result once [`Some`] is returned.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// world().non_send_resource::<Int>()
    ///     .watch(|x| (x.0 == 4).then_some(())).await;
    /// # });
    pub fn watch<Out: 'static>(&self, mut f: impl FnMut(&R) -> Option<Out> + 'static)
            -> impl Future<Output = Out> {
        let (sender, receiver) = channel();
        self.queue.repeat(
            move |world: &mut World| {
                world.get_non_send_resource::<R>().and_then(&mut f)
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }
}


/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug, Clone)]
pub struct AsyncResource<R: Resource>{
    pub(crate) queue: Rc<AsyncQueryQueue>,
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
            queue: executor.queue.clone(),
            p: PhantomData
        })
    }
}

impl<R: Resource> AsyncResource<R> {

    /// Obtain the underlying [`AsyncWorldMut`]
    pub fn world(&self) -> AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue).clone()
    }

    /// Wait until a [`Resource`] exists.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// world().resource::<Int>().exists().await;
    /// # });
    pub fn exists(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        self.queue.repeat(
            move |world: &mut World| {
                world
                    .get_resource::<R>()
                    .map(|_|())
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a function on the [`Resource`] and obtain the result.
    /// 
    /// Guaranteed to complete immediately with `World` access.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// world().resource::<Int>().get(|x| x.0).await?;
    /// # });
    pub fn get<Out: 'static>(&self, f: impl FnOnce(&R) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let f = move |world: &World| {
            world.get_resource::<R>()
                .map(f)
                .ok_or(AsyncFailure::ResourceNotFound)
        };
        let f = match self.world().with_world_ref(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        let (sender, receiver) = channel();
        self.queue.once(|w|f(w), sender);
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }

    /// Run a function on the mutable [`Resource`] and obtain the result.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// world().resource::<Int>().set(|x| x.0 = 16).await?;
    /// # });
    pub fn set<Out: 'static>(&self, f: impl FnOnce(&mut R) -> Out + 'static)
            -> impl Future<Output = AsyncResult<Out>> {
        let (sender, receiver) = channel();
        self.queue.once(
            move |world: &mut World| {
                world.get_resource_mut::<R>()
                    .map(|mut x| f(x.as_mut()))
                    .ok_or(AsyncFailure::ResourceNotFound)
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Run a repeatable function on the [`Resource`] and obtain the result once [`Some`] is returned.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// world().resource::<Int>().watch(|x| (x.0 == 4).then_some(())).await;
    /// # });
    pub fn watch<Out: 'static>(&self, mut f: impl FnMut(&R) -> Option<Out> + 'static)
            -> impl Future<Output = Out> {
        let (sender, receiver) = channel();
        self.queue.repeat(
            move |world: &mut World| {
                world.get_resource_ref::<R>().and_then(&mut f)
            },
            sender
        );
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

/// Add method to [`AsyncSystemParam`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncSystemParamDeref: SystemParam + Sized {
    type Target;
    fn async_deref(this: &AsyncSystemParam<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncSystemParam<C> where C: AsyncSystemParamDeref{
    type Target = <C as AsyncSystemParamDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncSystemParamDeref::async_deref(self)
    }
}