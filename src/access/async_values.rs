use std::marker::PhantomData;
use std::rc::Rc;
use bevy_ecs::component::Component;
use bevy_ecs::{entity::Entity, world::World};
use bevy_ecs::system::{In, Resource, StaticSystemParam, SystemId, SystemParam};
use ref_cast::RefCast;
use crate::async_systems::AsyncWorldParam;
use crate::access::AsyncWorldMut;
use crate::channels::ChannelOut;
use crate::signals::Signals;
use crate::async_systems::AsyncEntityParam;
use crate::{AsyncQueryQueue, AsyncFailure, AsyncResult};

/// Async version of [`SystemParam`].
#[derive(Debug, Clone)]
pub struct AsyncSystemParam<P: SystemParam>{
    pub(crate) queue: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<P>
}

impl<P: SystemParam> AsyncWorldParam for AsyncSystemParam<P> {
    fn from_async_context(
        executor: &AsyncWorldMut
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
    ) -> ChannelOut<AsyncResult<T>> {
        self.world().run(move |world: &mut World| {
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
        })
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

#[allow(unused)]
pub use bevy_ecs::system::NonSend;

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug, Clone)]
pub struct AsyncNonSend<R: 'static>{
    pub(crate) queue: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<R>
}

impl<R: 'static> AsyncWorldParam for AsyncNonSend<R> {
    fn from_async_context(
        executor: &AsyncWorldMut,
    ) -> Option<Self> {
        Some(Self {
            queue: executor.queue.clone(),
            p: PhantomData
        })
    }
}

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug, Clone)]
pub struct AsyncResource<R: Resource>{
    pub(crate) queue: Rc<AsyncQueryQueue>,
    pub(crate) p: PhantomData<R>
}

impl<R: Resource> AsyncWorldParam for AsyncResource<R> {
    fn from_async_context(
        executor: &AsyncWorldMut,
    ) -> Option<Self> {
        Some(Self {
            queue: executor.queue.clone(),
            p: PhantomData
        })
    }
}
