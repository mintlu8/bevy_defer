use crate::async_systems::AsyncEntityParam;
use crate::async_systems::AsyncWorldParam;
use crate::executor::with_world_mut;
use crate::reactors::Reactors;
use crate::signals::Signals;
use crate::{AccessError, AsyncResult};
use bevy_ecs::component::Component;
use bevy_ecs::system::{In, Resource, StaticSystemParam, SystemId, SystemParam};
use bevy_ecs::{entity::Entity, world::World};
use std::marker::PhantomData;

/// Async version of [`SystemParam`].
#[derive(Debug)]
pub struct AsyncSystemParam<P: SystemParam>(pub(crate) PhantomData<P>);

impl<P: SystemParam> Copy for AsyncSystemParam<P> {}

impl<P: SystemParam> Clone for AsyncSystemParam<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P: SystemParam> AsyncWorldParam for AsyncSystemParam<P> {
    fn from_async_context(_: &Reactors) -> Option<Self> {
        Some(AsyncSystemParam(PhantomData))
    }
}

type SysParamFn<Q, T> = dyn Fn(StaticSystemParam<Q>) -> T + Send + Sync + 'static;

#[derive(Debug, Resource)]
struct ResSysParamId<P: SystemParam, T>(SystemId<Box<SysParamFn<P, T>>, T>);

impl<Q: SystemParam + 'static> AsyncSystemParam<Q> {

    /// Run a function on the [`SystemParam`] and obtain the result.
    pub fn run<T: Send + Sync + 'static>(
        &self,
        f: impl (Fn(StaticSystemParam<Q>) -> T) + Send + Sync + 'static,
    ) -> AsyncResult<T> {
        with_world_mut(move |world: &mut World| {
            let id = match world.get_resource::<ResSysParamId<Q, T>>() {
                Some(res) => res.0,
                None => {
                    let id = world.register_system(
                        |input: In<Box<SysParamFn<Q, T>>>, query: StaticSystemParam<Q>| -> T {
                            (input.0)(query)
                        },
                    );
                    world.insert_resource(ResSysParamId(id));
                    id
                }
            };
            world
                .run_system_with_input(id, Box::new(f))
                .map_err(|_| AccessError::SystemParamError)
        })
    }
}

/// An `AsyncSystemParam` that gets or sets a component on the current `Entity`.
#[derive(Debug)]
pub struct AsyncComponent<C: Component> {
    pub(crate) entity: Entity,
    pub(crate) p: PhantomData<C>,
}

impl<C: Component> Copy for AsyncComponent<C> {}

impl<C: Component> Clone for AsyncComponent<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Component> AsyncEntityParam for AsyncComponent<C> {
    type Signal = ();

    fn fetch_signal(_: &Signals) -> Option<Self::Signal> {
        Some(())
    }

    fn from_async_context(
        entity: Entity,
        _: &Reactors,
        _: (),
        _: &[Entity],
    ) -> Option<Self> {
        Some(Self {
            entity,
            p: PhantomData,
        })
    }
}

#[allow(unused)]
pub use bevy_ecs::system::NonSend;

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug)]
pub struct AsyncNonSend<R: 'static> (
    pub(crate) PhantomData<R>,
);

impl<R: 'static> Copy for AsyncNonSend<R> {}

impl<R: 'static> Clone for AsyncNonSend<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: 'static> AsyncWorldParam for AsyncNonSend<R> {
    fn from_async_context(_: &Reactors) -> Option<Self> {
        Some(Self(PhantomData))
    }
}

/// An `AsyncSystemParam` that gets or sets a resource on the `World`.
#[derive(Debug)]
pub struct AsyncResource<R: Resource>(
    pub(crate) PhantomData<R>,
);

impl<R: Resource> Copy for AsyncResource<R> {}

impl<R: Resource> Clone for AsyncResource<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Resource> AsyncWorldParam for AsyncResource<R> {
    fn from_async_context(_: &Reactors) -> Option<Self> {
        Some(Self(PhantomData))
    }
}
