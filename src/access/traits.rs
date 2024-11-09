//! Access traits for `bevy_defer`.

use crate::access::{
    AsyncAsset, AsyncComponent, AsyncEntityQuery, AsyncNonSend, AsyncQuery, AsyncQuerySingle,
    AsyncResource, AsyncWorld,
};
use crate::tween::{AsSeconds, Lerp, Playback};
use crate::OwnedQueryState;
use crate::{
    cancellation::TaskCancellation,
    executor::{with_world_mut, with_world_ref},
    sync::oneshot::{ChannelOut, InterpolateOut, MaybeChannelOut},
    AccessError, AccessResult,
};
use bevy::asset::{Asset, Assets, Handle};
use bevy::ecs::{
    component::Component,
    entity::Entity,
    query::{QueryData, QueryFilter, WorldQuery},
    system::Resource,
    world::World,
};
use futures::future::{ready, Either};
use std::any::type_name;
use std::{borrow::BorrowMut, cell::OnceCell};

/// Obtain readonly access from a readonly `&World`.
pub trait AsyncReadonlyAccess: AsyncAccess {
    /// Obtain reference from a read only world.
    fn from_ref_world<'t>(world: &'t World, cx: &Self::Cx) -> AccessResult<Self::Ref<'t>>;
}

/// Async access that derefs to a concrete type.
pub trait AsyncAccessRef:
    for<'t> AsyncAccess<RefMut<'t> = &'t mut Self::Generic>
    + for<'t> AsyncReadonlyAccess<Ref<'t> = &'t Self::Generic>
{
    type Generic: 'static;
}

/// Allows the `take` method
pub trait AsyncTake: AsyncAccessRef {
    /// Remove and obtain from the world.
    fn take(world: &mut World, cx: &Self::Cx) -> AccessResult<Self::Generic>;
}

/// Allows the `on_load` method family.
///
/// Currently `Resource`, `NonSend` and `Asset` only.
pub trait AsyncLoad: AsyncAccess {}

/// Provides functionalities for async accessors.
pub trait AsyncAccess {
    /// Static information, usually `Entity` or `Handle`.
    type Cx: 'static;
    /// Optional borrow guard for mutable access.
    type RefMutCx<'t>;
    /// Reference for immutable access.
    type Ref<'t>;
    /// Reference for mutable access.
    type RefMut<'t>;

    /// Obtain `Cx`.
    fn as_cx(&self) -> Self::Cx;

    /// Obtain a borrow guard.
    fn from_mut_world<'t>(world: &'t mut World, cx: &Self::Cx) -> AccessResult<Self::RefMutCx<'t>>;
    /// Obtain a mutable reference from the borrow guard.
    fn from_mut_cx<'t>(
        cx: &'t mut Self::RefMutCx<'_>,
        cx: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>>;

    /// Remove and obtain the item from the world.
    fn take(&self) -> AccessResult<Self::Generic>
    where
        Self: AsyncTake,
    {
        let cx = self.as_cx();
        with_world_mut(move |w| <Self as AsyncTake>::take(w, &cx))
    }

    /// Remove and obtain the item from the world once loaded.
    fn take_on_load(&self) -> ChannelOut<AccessResult<Self::Generic>>
    where
        Self: AsyncTake + AsyncLoad,
    {
        let ctx = self.as_cx();
        AsyncWorld.watch(move |w| match <Self as AsyncTake>::take(w, &ctx) {
            Ok(result) => Some(Ok(result)),
            Err(err) if Self::should_continue(err) => None,
            Err(err) => Some(Err(err)),
        })
    }

    /// Remove the item if it exists.
    fn remove(&self)
    where
        Self: AsyncTake,
    {
        let cx = self.as_cx();
        with_world_mut(move |w| {
            let _ = <Self as AsyncTake>::take(w, &cx);
        });
    }

    /// Run a function on this item and obtain the result.
    fn set<T>(&self, f: impl FnOnce(Self::RefMut<'_>) -> T) -> AccessResult<T> {
        let cx = self.as_cx();
        with_world_mut(|w| {
            let mut mut_cx = Self::from_mut_world(w, &cx)?;
            let cx = Self::from_mut_cx(&mut mut_cx, &cx)?;
            Ok(f(cx))
        })
    }

    /// Run a function if the query is infallible.
    fn with<T>(&self, f: impl FnOnce(Self::RefMut<'_>) -> T) -> T
    where
        Self: InfallibleQuery,
    {
        let cx = self.as_cx();
        with_world_mut(|w| {
            let mut mut_cx = Self::from_mut_world(w, &cx).expect("Should be infallible");
            let cx = Self::from_mut_cx(&mut mut_cx, &cx).expect("Should be infallible");
            f(cx)
        })
    }

    /// Obtain an underlying world accessor from an item using [`WorldDeref`].
    ///
    /// An example is obtaining `AsyncAsset<T>` from `AsyncComponent<Handle<T>>`.
    fn chain(&self) -> AccessResult<<Self::Generic as WorldDeref>::Target>
    where
        Self: AsyncAccessRef,
        Self::Generic: WorldDeref,
    {
        let cx = self.as_cx();
        with_world_mut(|w| {
            let mut mut_cx = Self::from_mut_world(w, &cx)?;
            let cx = Self::from_mut_cx(&mut mut_cx, &cx)?;
            Ok(WorldDeref::deref_to(cx))
        })
    }

    /// Run a function on this item and obtain the result once loaded.
    fn set_on_load<T: 'static>(
        &self,
        mut f: impl FnMut(Self::RefMut<'_>) -> T + 'static,
    ) -> ChannelOut<AccessResult<T>>
    where
        Self: AsyncLoad,
    {
        let cx = self.as_cx();
        AsyncWorld.watch(move |w| match Self::from_mut_world(w, &cx) {
            Ok(mut mut_cx) => match Self::from_mut_cx(&mut mut_cx, &cx) {
                Ok(ref_mut) => Some(Ok(f(ref_mut))),
                Err(err) if Self::should_continue(err) => None,
                Err(err) => Some(Err(err)),
            },
            Err(err) if Self::should_continue(err) => None,
            Err(err) => Some(Err(err)),
        })
    }

    /// Run a function on this item until it returns `Some`.
    fn watch<T: 'static>(
        &self,
        mut f: impl FnMut(Self::RefMut<'_>) -> Option<T> + 'static,
    ) -> ChannelOut<AccessResult<T>> {
        let cx = self.as_cx();
        AsyncWorld.watch(move |w| match Self::from_mut_world(w, &cx) {
            Ok(mut mut_cx) => match Self::from_mut_cx(&mut mut_cx, &cx) {
                Ok(ref_mut) => f(ref_mut).map(Ok),
                Err(err) if Self::should_continue(err) => None,
                Err(err) => Some(Err(err)),
            },
            Err(err) if Self::should_continue(err) => None,
            Err(err) => Some(Err(err)),
        })
    }

    /// Continue `watch` and `on_load` if fetch context failed with these errors.
    #[allow(unused_variables)]
    fn should_continue(err: AccessError) -> bool {
        false
    }

    /// Check if item exists.
    fn exists(&self) -> bool
    where
        Self: AsyncReadonlyAccess,
    {
        let ctx = self.as_cx();
        with_world_ref(|w| Self::from_ref_world(w, &ctx).is_ok())
    }

    /// Wait until the item is loaded.
    fn on_load(&self) -> MaybeChannelOut<()>
    where
        Self: AsyncReadonlyAccess + AsyncLoad,
    {
        let ctx = self.as_cx();
        if with_world_ref(|w| Self::from_ref_world(w, &ctx).is_ok()) {
            return Either::Right(ready(()));
        }
        Either::Left(
            AsyncWorld
                .watch(move |world: &mut World| Self::from_mut_world(world, &ctx).ok().map(|_| ())),
        )
    }

    /// Run a function on a readonly reference to this item and obtain the result.
    fn get<T>(&self, f: impl FnOnce(Self::Ref<'_>) -> T) -> AccessResult<T>
    where
        Self: AsyncReadonlyAccess,
    {
        let ctx = self.as_cx();
        with_world_ref(|w| Ok(f(Self::from_ref_world(w, &ctx)?)))
    }

    /// Run a function on a readonly reference to this item and obtain the result,
    /// repeat until the item is loaded.
    fn get_on_load<T: 'static>(
        &self,
        f: impl FnOnce(Self::Ref<'_>) -> T + 'static,
    ) -> MaybeChannelOut<AccessResult<T>>
    where
        Self: AsyncReadonlyAccess + AsyncLoad,
    {
        let ctx = self.as_cx();
        let mut f = Some(f);
        // Wrap a FnOnce in a FnMut.
        let mut f = move |world: &World| {
            let item = Self::from_ref_world(world, &ctx)?;
            if let Some(f) = f.take() {
                Ok(f(item))
            } else {
                Err(AccessError::ShouldNotHappen)
            }
        };
        match with_world_ref(&mut f) {
            Ok(result) => return Either::Right(ready(Ok(result))),
            Err(err) if Self::should_continue(err) => (),
            Err(err) => return Either::Right(ready(Err(err))),
        };
        Either::Left(AsyncWorld.watch(move |w| match f(w) {
            Ok(result) => Some(Ok(result)),
            Err(err) if Self::should_continue(err) => None,
            Err(err) => Some(Err(err)),
        }))
    }

    /// Copy the item.
    fn copied(&self) -> AccessResult<Self::Generic>
    where
        Self: AsyncAccessRef,
        Self::Generic: Copy,
    {
        self.get(|x| *x)
    }

    /// Clone the item.
    fn cloned(&self) -> AccessResult<Self::Generic>
    where
        Self: AsyncAccessRef,
        Self::Generic: Clone,
    {
        self.get(Clone::clone)
    }

    /// Clone the item, repeat until the item is loaded.
    fn clone_on_load(&self) -> MaybeChannelOut<AccessResult<Self::Generic>>
    where
        Self: AsyncAccessRef + AsyncLoad,
        Self::Generic: Clone,
    {
        self.get_on_load(Clone::clone)
    }

    /// Interpolate to a new value from the previous value.
    fn interpolate_to<V: Lerp>(
        &self,
        to: V,
        mut get: impl FnMut(Self::Ref<'_>) -> V + Send + 'static,
        mut set: impl FnMut(Self::RefMut<'_>, V) + Send + 'static,
        mut curve: impl FnMut(f32) -> f32 + Send + 'static,
        duration: impl AsSeconds,
        cancel: impl Into<TaskCancellation>,
    ) -> InterpolateOut
    where
        Self: AsyncAccessRef,
    {
        let mut t = 0.0;
        let duration = duration.as_secs();
        let source = OnceCell::new();
        let cancel = cancel.into();
        let cx = self.as_cx();
        AsyncWorld
            .timed_routine(
                move |world, dt| {
                    let Ok(mut mut_cx) = Self::from_mut_world(world, &cx) else {
                        return None;
                    };
                    let Ok(item) = Self::from_mut_cx(&mut mut_cx, &cx) else {
                        return None;
                    };
                    let source = source.get_or_init(|| get(item)).clone();
                    t += dt.as_secs_f32();
                    if t > duration {
                        set(item.borrow_mut(), to.clone());
                        Some(Ok(()))
                    } else {
                        let fac = curve(t / duration);
                        set(item.borrow_mut(), V::lerp(source, to.clone(), fac));
                        None
                    }
                },
                cancel,
            )
            .into_interpolate_out()
    }

    /// Run an animation, maybe repeatedly, that can be cancelled.
    ///
    /// It is recommended to `spawn` the result instead of awaiting it directly
    /// if not [`Playback::Once`].
    ///
    /// ```
    /// # /*
    /// spawn(interpolate(.., Playback::Loop, &cancel));
    /// cancel.cancel();
    /// # */
    /// ```
    fn interpolate<V>(
        &self,
        mut span: impl FnMut(f32) -> V + 'static,
        mut write: impl FnMut(Self::RefMut<'_>, V) + 'static,
        mut curve: impl FnMut(f32) -> f32 + 'static,
        duration: impl AsSeconds,
        playback: Playback,
        cancel: impl Into<TaskCancellation>,
    ) -> InterpolateOut {
        let cx = self.as_cx();
        let duration = duration.as_secs();
        let mut t = 0.0;
        let cancel = cancel.into();
        AsyncWorld
            .timed_routine(
                move |world, dt| {
                    let Ok(mut mut_cx) = Self::from_mut_world(world, &cx) else {
                        return None;
                    };
                    let Ok(item) = Self::from_mut_cx(&mut mut_cx, &cx) else {
                        return None;
                    };
                    t += dt.as_secs_f32() / duration;
                    let fac = if t > 1.0 {
                        match playback {
                            Playback::Once => {
                                write(item, span(1.0));
                                return Some(Ok(()));
                            }
                            Playback::Loop => {
                                t = t.fract();
                                t
                            }
                            Playback::Bounce => {
                                t %= 2.0;
                                1.0 - (1.0 - t % 2.0).abs()
                            }
                        }
                    } else {
                        t
                    };
                    write(item, span(curve(fac)));
                    None
                },
                cancel,
            )
            .into_interpolate_out()
    }
}

impl<C: Component> AsyncAccess for AsyncComponent<C> {
    type Cx = Entity;
    type RefMutCx<'t> = &'t mut C;
    type Ref<'t> = &'t C;
    type RefMut<'t> = &'t mut C;

    fn as_cx(&self) -> Self::Cx {
        self.entity
    }

    fn from_mut_world<'t>(world: &'t mut World, cx: &Self::Cx) -> AccessResult<Self::RefMut<'t>> {
        world
            .get_mut::<C>(*cx)
            .ok_or(AccessError::ComponentNotFound {
                name: type_name::<C>(),
            })
            .map(|x| x.into_inner())
    }

    fn from_mut_cx<'t>(
        mut_cx: &'t mut Self::RefMutCx<'_>,
        _: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>> {
        Ok(mut_cx)
    }
}

impl<C: Component> AsyncReadonlyAccess for AsyncComponent<C> {
    fn from_ref_world<'t>(world: &'t World, cx: &Self::Cx) -> AccessResult<Self::Ref<'t>> {
        world
            .get_entity(*cx)
            .map_err(|_| AccessError::EntityNotFound(*cx))?
            .get::<C>()
            .ok_or(AccessError::ComponentNotFound {
                name: type_name::<C>(),
            })
    }
}

impl<C: Component> AsyncAccessRef for AsyncComponent<C> {
    type Generic = C;
}

impl<C: Component> AsyncTake for AsyncComponent<C> {
    fn take(world: &mut World, cx: &Self::Cx) -> AccessResult<Self::Generic> {
        world
            .get_entity_mut(*cx)
            .map_err(|_| AccessError::EntityNotFound(*cx))?
            .take::<C>()
            .ok_or(AccessError::ComponentNotFound {
                name: type_name::<C>(),
            })
    }
}

impl<R: Resource> AsyncAccess for AsyncResource<R> {
    type Cx = ();
    type RefMutCx<'t> = &'t mut R;
    type Ref<'t> = &'t R;
    type RefMut<'t> = &'t mut R;

    fn as_cx(&self) -> Self::Cx {}

    fn should_continue(err: AccessError) -> bool {
        err == AccessError::ResourceNotFound {
            name: type_name::<R>(),
        }
    }

    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Cx) -> AccessResult<Self::RefMut<'t>> {
        world
            .get_resource_mut::<R>()
            .ok_or(AccessError::ResourceNotFound {
                name: type_name::<R>(),
            })
            .map(|x| x.into_inner())
    }

    fn from_mut_cx<'t>(
        mut_cx: &'t mut Self::RefMutCx<'_>,
        _: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>> {
        Ok(mut_cx)
    }
}

impl<R: Resource> AsyncReadonlyAccess for AsyncResource<R> {
    fn from_ref_world<'t>(world: &'t World, _: &Self::Cx) -> AccessResult<Self::Ref<'t>> {
        world.get_resource().ok_or(AccessError::ResourceNotFound {
            name: type_name::<R>(),
        })
    }
}

impl<R: Resource> AsyncAccessRef for AsyncResource<R> {
    type Generic = R;
}

impl<R: Resource> AsyncTake for AsyncResource<R> {
    fn take(world: &mut World, _: &Self::Cx) -> AccessResult<Self::Generic> {
        world
            .remove_resource()
            .ok_or(AccessError::ResourceNotFound {
                name: type_name::<R>(),
            })
    }
}

impl<R: Resource> AsyncLoad for AsyncResource<R> {}

impl<R: 'static> AsyncAccess for AsyncNonSend<R> {
    type Cx = ();
    type RefMutCx<'t> = &'t mut R;
    type Ref<'t> = &'t R;
    type RefMut<'t> = &'t mut R;

    fn should_continue(err: AccessError) -> bool {
        err == AccessError::ResourceNotFound {
            name: type_name::<R>(),
        }
    }

    fn as_cx(&self) -> Self::Cx {}

    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Cx) -> AccessResult<Self::RefMut<'t>> {
        world
            .get_non_send_resource_mut::<R>()
            .ok_or(AccessError::ResourceNotFound {
                name: type_name::<R>(),
            })
            .map(|x| x.into_inner())
    }

    fn from_mut_cx<'t>(
        mut_cx: &'t mut Self::RefMutCx<'_>,
        _: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>> {
        Ok(mut_cx)
    }
}

impl<R: 'static> AsyncReadonlyAccess for AsyncNonSend<R> {
    fn from_ref_world<'t>(world: &'t World, _: &Self::Cx) -> AccessResult<Self::Ref<'t>> {
        world
            .get_non_send_resource()
            .ok_or(AccessError::ResourceNotFound {
                name: type_name::<R>(),
            })
    }
}

impl<R: 'static> AsyncAccessRef for AsyncNonSend<R> {
    type Generic = R;
}

impl<R: 'static> AsyncLoad for AsyncNonSend<R> {}

impl<R: 'static> AsyncTake for AsyncNonSend<R> {
    fn take(world: &mut World, _: &Self::Cx) -> AccessResult<Self::Generic> {
        world
            .remove_non_send_resource()
            .ok_or(AccessError::ResourceNotFound {
                name: type_name::<R>(),
            })
    }
}

impl<A: Asset> AsyncAccess for AsyncAsset<A> {
    type Cx = Handle<A>;
    type RefMutCx<'t> = &'t mut A;
    type Ref<'t> = &'t A;
    type RefMut<'t> = &'t mut A;

    fn as_cx(&self) -> Self::Cx {
        self.0.clone_weak()
    }

    fn should_continue(err: AccessError) -> bool {
        err == AccessError::AssetNotFound {
            name: type_name::<A>(),
        }
    }

    fn from_mut_world<'t>(
        world: &'t mut World,
        handle: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>> {
        world
            .get_resource_mut::<Assets<A>>()
            .ok_or(AccessError::ResourceNotFound {
                name: type_name::<Assets<A>>(),
            })?
            .into_inner()
            .get_mut(handle)
            .ok_or(AccessError::AssetNotFound {
                name: type_name::<A>(),
            })
    }

    fn from_mut_cx<'t>(
        mut_cx: &'t mut Self::RefMutCx<'_>,
        _: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>> {
        Ok(mut_cx)
    }
}

impl<A: Asset> AsyncReadonlyAccess for AsyncAsset<A> {
    fn from_ref_world<'t>(world: &'t World, handle: &Self::Cx) -> AccessResult<Self::Ref<'t>> {
        world
            .get_resource::<Assets<A>>()
            .ok_or(AccessError::ResourceNotFound {
                name: type_name::<Assets<A>>(),
            })?
            .get(handle)
            .ok_or(AccessError::AssetNotFound {
                name: type_name::<A>(),
            })
    }
}

impl<A: Asset> AsyncAccessRef for AsyncAsset<A> {
    type Generic = A;
}

impl<A: Asset> AsyncTake for AsyncAsset<A> {
    fn take(world: &mut World, handle: &Self::Cx) -> AccessResult<Self::Generic> {
        world
            .get_resource_mut::<Assets<A>>()
            .ok_or(AccessError::ResourceNotFound {
                name: type_name::<Assets<A>>(),
            })?
            .remove(handle)
            .ok_or(AccessError::AssetNotFound {
                name: type_name::<A>(),
            })
    }
}

impl<A: Asset> AsyncLoad for AsyncAsset<A> {}

impl<D: QueryData + 'static, F: QueryFilter + 'static> AsyncAccess for AsyncQuery<D, F> {
    type Cx = ();
    type RefMutCx<'t> = Option<OwnedQueryState<'t, D, F>>;
    type Ref<'t> = OwnedQueryState<'t, D, F>;
    type RefMut<'t> = OwnedQueryState<'t, D, F>;

    fn as_cx(&self) -> Self::Cx {}

    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Cx) -> AccessResult<Self::RefMutCx<'t>> {
        Ok(Some(OwnedQueryState::new(world)))
    }

    fn from_mut_cx<'t>(
        mut_cx: &'t mut Self::RefMutCx<'_>,
        _: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>> {
        Ok(mut_cx.take().unwrap())
    }
}

impl<D: QueryData + 'static, F: QueryFilter + 'static> AsyncAccess for AsyncQuerySingle<D, F> {
    type Cx = ();
    type RefMutCx<'t> = OwnedQueryState<'t, D, F>;
    type Ref<'t> = <D::ReadOnly as WorldQuery>::Item<'t>;
    type RefMut<'t> = D::Item<'t>;

    fn as_cx(&self) -> Self::Cx {}

    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Cx) -> AccessResult<Self::RefMutCx<'t>> {
        Ok(OwnedQueryState::new(world))
    }

    fn from_mut_cx<'t>(
        cx: &'t mut Self::RefMutCx<'_>,
        _: &Self::Cx,
    ) -> AccessResult<Self::RefMut<'t>> {
        cx.single_mut()
    }
}

impl<D: QueryData + 'static, F: QueryFilter + 'static> AsyncAccess for AsyncEntityQuery<D, F> {
    type Cx = Entity;
    type RefMutCx<'t> = OwnedQueryState<'t, D, F>;
    type Ref<'t> = <D::ReadOnly as WorldQuery>::Item<'t>;
    type RefMut<'t> = D::Item<'t>;

    fn as_cx(&self) -> Self::Cx {
        self.entity
    }

    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Cx) -> AccessResult<Self::RefMutCx<'t>> {
        Ok(OwnedQueryState::new(world))
    }

    fn from_mut_cx<'t>(
        cx: &'t mut Self::RefMutCx<'_>,
        entity: &Entity,
    ) -> AccessResult<Self::RefMut<'t>> {
        cx.get_mut(*entity)
            .map_err(|_| AccessError::EntityNotFound(*entity))
    }
}

/// If implemented on a resource or a non-send resource, assume its always available
/// during `bevy_defer`'s runtime. This allows the `with` accessor to be used.
pub trait InfallibleQuery {}

impl<T: QueryData, F: QueryFilter> InfallibleQuery for AsyncQuery<T, F> {}

impl<T: Resource + InfallibleQuery> InfallibleQuery for AsyncResource<T> {}

impl<T: InfallibleQuery + 'static> InfallibleQuery for AsyncNonSend<T> {}

/// Signifies an item points to another item in the [`World`].
pub trait WorldDeref {
    type Target: 'static;

    /// Returns a world accessor like [`AsyncAsset`] or [`struct@AsyncComponent`].
    fn deref_to(&self) -> Self::Target;
}

impl<T: Asset> WorldDeref for Handle<T> {
    type Target = AsyncAsset<T>;

    fn deref_to(&self) -> Self::Target {
        AsyncAsset(self.clone_weak())
    }
}
