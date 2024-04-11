use std::{borrow::BorrowMut, cell::OnceCell};

use bevy_asset::{Asset, Assets, Handle};
use bevy_ecs::{component::Component, entity::Entity, query::{QueryData, QueryFilter}, system::Resource, world::World};
use futures::{future::{ready, Either}, Future};
use ref_cast::RefCast;

use crate::{async_asset::AsyncAsset, async_query::{AsyncQuery, OwnedQueryState}, async_world::AsyncWorldMut, cancellation::TaskCancellation, channel, locals::with_world_ref, AsyncFailure, AsyncResult, CHANNEL_CLOSED};
use crate::tween::{AsSeconds, Lerp, Playback};
use crate::async_values::{AsyncComponent, AsyncNonSend, AsyncResource};

/// A trait that lets RPIT `'static` futures capture (non-existent) lifetimes.
/// 
/// See <https://github.com/rust-lang/rust/issues/82171>
/// 
/// This fixes the issue that `impl Future<Output = i32> + 'static` in trait can potentially capture additional lifetimes.
/// 
/// ```compile_fail
/// # use core::future::Future;
/// # use bevy_defer::Captures;
/// trait A {
///     fn get(&self) -> impl Future<Output = i32> + 'static;
/// }
///
/// fn get(a: &impl A) -> impl Future<Output = i32> + 'static {
///     a.get()
/// }
/// ```
/// 
/// The fix is this, note `get` doesn't actually capture lifetimes due to the static bound.
/// 
/// ```
/// # use core::future::Future;
/// # use bevy_defer::Captures;
/// trait A {
///     fn get(&self) -> impl Future<Output = i32> + 'static;
/// }
///
/// fn get(a: &impl A) -> impl Future<Output = i32> + 'static + Captures<&'_ ()> {
///     a.get()
/// }
/// ```
pub trait Captures<T> {}
impl<'a, T: ?Sized> Captures<&'a ()> for T {}

pub trait AsyncReadonlyAccess: AsyncAccess {
    fn from_ref_world<'t>(world: &'t World, cx: &Self::Ctx) -> AsyncResult<Self::Ref<'t>>;
}

pub trait AsyncAccessRef: 
        for<'t> AsyncAccess<RefMut<'t> = &'t mut Self::Generic> +  
        for<'t> AsyncReadonlyAccess<Ref<'t> = &'t Self::Generic> {
    type Generic: 'static;
}

pub trait AsyncTake: AsyncAccessRef{
    fn take<'t>(world: &'t mut World, cx: &Self::Ctx) -> AsyncResult<Self::Generic>;
}

pub trait AsyncAccess {
    type Ctx: 'static;
    type Ref<'t>;
    type RefMut<'t>;

    fn world(&self) -> &AsyncWorldMut;
    fn as_ctx(&self) -> Self::Ctx;
    fn from_mut_world<'t>(world: &'t mut World, cx: &Self::Ctx) -> AsyncResult<Self::RefMut<'t>>;

    fn take<'t>(&self) -> impl Future<Output = AsyncResult<Self::Generic>> + 'static where Self: AsyncTake {
        let ctx = self.as_ctx();
        self.world().run(move |w| Ok(<Self as AsyncTake>::take(w, &ctx)?))
    }

    fn take_on_load<'t>(&self) -> impl Future<Output = AsyncResult<Self::Generic>> + 'static where Self: AsyncTake {
        let ctx = self.as_ctx();
        self.world().watch(move |w| match <Self as AsyncTake>::take(w, &ctx) {
            Ok(result) => Some(Ok(result)),
            Err(err) if Self::should_continue(err) => None,
            Err(err) => Some(Err(err)),
        })
    }

    fn set<T: 'static>(&self, f: impl FnOnce(Self::RefMut<'_>) -> T + 'static) -> impl Future<Output = AsyncResult<T>> + 'static{
        let ctx = self.as_ctx();
        self.world().run(move |w| Ok(f(Self::from_mut_world(w, &ctx)?)))
    }

    fn watch<T: 'static>(&self, mut f: impl FnMut(Self::RefMut<'_>) -> Option<T> + 'static) -> impl Future<Output = AsyncResult<T>> + 'static{
        let ctx = self.as_ctx();
        self.world().watch(move |w| match Self::from_mut_world(w, &ctx) {
            Ok(result) => f(result).map(Ok),
            Err(err) if Self::should_continue(err) => None,
            Err(err) => Some(Err(err)),
        })
    }

    #[allow(unused_variables)]
    fn should_continue(err: AsyncFailure) -> bool {
        false
    }

    fn exists(&self) -> impl Future<Output = ()> where Self: AsyncReadonlyAccess {
        let ctx = self.as_ctx();
        if matches!(with_world_ref(|w| Self::from_ref_world(w, &ctx).is_ok()), Ok(true)) {
            return Either::Right(ready(()))
        };
        use futures::FutureExt;
        let (sender, receiver) = channel();
        self.world().queue.repeat(
            move |world: &mut World| {
                Self::from_mut_world(world, &ctx).ok().map(|_| ())
            },
            sender
        );
        Either::Left(receiver.map(|x| x.expect(CHANNEL_CLOSED)))
    }


    fn get<T: 'static>(&self, f: impl FnOnce(Self::Ref<'_>) -> T + 'static) -> impl Future<Output = AsyncResult<T>> + 'static where Self: AsyncReadonlyAccess{
        let ctx = self.as_ctx();
        let f = move |world: &World| {
            Ok(f(Self::from_ref_world(world, &ctx)?))
        }; 
        let f = match with_world_ref(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        Either::Left(self.world().run(|w| f(w)))
    }

    fn get_on_load<T: 'static>(&self, f: impl FnOnce(Self::Ref<'_>) -> T + 'static) -> impl Future<Output = AsyncResult<T>> + 'static where Self: AsyncReadonlyAccess{
        let ctx = self.as_ctx();
        let mut f = Some(f);
        // Wrap a FnOnce in a FnMut.
        let f = move |world: &World| {
            let item = Self::from_ref_world(world, &ctx)?;
            if let Some(f) = f.take() {
                Ok(f(item))
            } else {
                Err(AsyncFailure::ShouldNotHappen)
            }
        }; 
        let mut f = match with_world_ref(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        Either::Left(self.world().watch(move |w| match f(w) {
            Ok(result) => Some(Ok(result)),
            Err(err) if Self::should_continue(err) => None,
            Err(err) => Some(Err(err)),
        }))
    }

    fn cloned<'a>(&self) -> impl Future<Output = AsyncResult<Self::Generic>> where Self: AsyncAccessRef, Self::Generic: Clone {
        self.get(move |a| a.clone())
    }

    fn clone_on_load<'t>(&self) -> impl Future<Output = AsyncResult<Self::Generic>> + 'static where Self: AsyncAccessRef, Self::Generic: Clone {
        let ctx = self.as_ctx();
        self.world().watch(move |w| match Self::from_ref_world(w, &ctx) {
            Ok(result) => Some(Ok(result.clone())),
            Err(err) if Self::should_continue(err) => None,
            Err(err) => Some(Err(err)),
        })
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
    ) -> impl Future<Output = AsyncResult<()>> + 'static where Self: AsyncAccessRef {
        let world = self.world().clone();
        let mut t = 0.0;
        let duration = duration.as_secs();
        let source = OnceCell::new();
        let cancel = cancel.into();
        let ctx = self.as_ctx();
        async move {
            world.fixed_routine(move |world, dt| {
                let Ok(item) = Self::from_mut_world(world, &ctx) else { return None };
                let source = source.get_or_init(||get(item)).clone();
                t += dt.as_secs_f32();
                if t > duration {
                    set(item.borrow_mut(), to.clone());
                    Some(Ok(()))
                } else {
                    let fac = curve(t / duration);
                    set(item.borrow_mut(), V::lerp(source, to.clone(), fac));
                    None
                }
            }, cancel).await.unwrap_or(Ok(()))
        }
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
    ) -> impl Future<Output = AsyncResult<()>> + 'static {
        let world = self.world().clone();
        let ctx = self.as_ctx();
        let duration = duration.as_secs();
        let mut t = 0.0;
        let cancel = cancel.into();
        async move {
            world.fixed_routine(move |world, dt| {
                let Ok(item) = Self::from_mut_world(world, &ctx) else { return None };
                t += dt.as_secs_f32() / duration;
                let fac = if t > 1.0 {
                    match playback {
                        Playback::Once => {
                            write(item, span(1.0));
                            return Some(Ok(()))
                        },
                        Playback::Loop => {
                            t = t.fract();
                            t
                        },
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
            }, cancel).await.unwrap_or(Ok(()))
        }
    }

}

impl<C: Component> AsyncAccess for AsyncComponent<C> {
    type Ctx = Entity;
    type Ref<'t> = &'t C;
    type RefMut<'t> = &'t mut C;

    fn world(&self) -> &AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue)
    }

    fn as_ctx(&self) -> Self::Ctx {
        self.entity
    }

    fn from_mut_world<'t>(world: &'t mut World, cx: &Self::Ctx) -> AsyncResult<Self::RefMut<'t>> {
        world.get_mut::<C>(*cx)
            .ok_or(AsyncFailure::ComponentNotFound)
            .map(|x| x.into_inner())
    }
}

impl<C: Component> AsyncReadonlyAccess for AsyncComponent<C> {
    fn from_ref_world<'t>(world: &'t World, cx: &Self::Ctx) -> AsyncResult<Self::Ref<'t>> {
        world.get_entity(*cx)
            .ok_or(AsyncFailure::EntityNotFound)?
            .get::<C>()
            .ok_or(AsyncFailure::ComponentNotFound)
    }
}

impl<C: Component> AsyncAccessRef for AsyncComponent<C> {
    type Generic = C;
}

impl<C: Component> AsyncTake for AsyncComponent<C> {
    fn take<'t>(world: &'t mut World, cx: &Self::Ctx) -> AsyncResult<Self::Generic> {
        world.get_entity_mut(*cx).ok_or(AsyncFailure::EntityNotFound)?
            .take::<C>().ok_or(AsyncFailure::ComponentNotFound)
    }
}

impl<R: Resource> AsyncAccess for AsyncResource<R> {
    type Ctx = ();
    type Ref<'t> = &'t R;
    type RefMut<'t> = &'t mut R;

    fn world(&self) -> &AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue)
    }

    fn as_ctx(&self) -> Self::Ctx {
        ()
    }

    fn should_continue(err: AsyncFailure) -> bool {
        err == AsyncFailure::ResourceNotFound
    }

    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Ctx) -> AsyncResult<Self::RefMut<'t>> {
        world.get_resource_mut::<R>()
            .ok_or(AsyncFailure::ResourceNotFound)
            .map(|x| x.into_inner())
    }
}

impl<R: Resource> AsyncReadonlyAccess for AsyncResource<R> {
    fn from_ref_world<'t>(world: &'t World, _: &Self::Ctx) -> AsyncResult<Self::Ref<'t>> {
        world.get_resource().ok_or(AsyncFailure::ResourceNotFound)
    }
}

impl<R: Resource> AsyncAccessRef for AsyncResource<R> {
    type Generic = R;
}

impl<R: Resource> AsyncTake for AsyncResource<R> {
    fn take<'t>(world: &'t mut World, _: &Self::Ctx) -> AsyncResult<Self::Generic> {
        world.remove_resource().ok_or(AsyncFailure::ResourceNotFound)
    }
}

impl<R: Resource> AsyncAccess for AsyncNonSend<R> {
    type Ctx = ();
    type Ref<'t> = &'t R;
    type RefMut<'t> = &'t mut R;

    fn world(&self) -> &AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue)
    }

    fn should_continue(err: AsyncFailure) -> bool {
        err == AsyncFailure::ResourceNotFound
    }

    fn as_ctx(&self) -> Self::Ctx {
        ()
    }

    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Ctx) -> AsyncResult<Self::RefMut<'t>> {
        world.get_non_send_resource_mut::<R>()
            .ok_or(AsyncFailure::ResourceNotFound)
            .map(|x| x.into_inner())
    }
}

impl<R: Resource> AsyncReadonlyAccess for AsyncNonSend<R> {

    fn from_ref_world<'t>(world: &'t World, _: &Self::Ctx) -> AsyncResult<Self::Ref<'t>> {
        world.get_non_send_resource().ok_or(AsyncFailure::ResourceNotFound)
    }
}

impl<R: Resource> AsyncAccessRef for AsyncNonSend<R> {
    type Generic = R;
}

impl<R: Resource> AsyncTake for AsyncNonSend<R> {
    fn take<'t>(world: &'t mut World, _: &Self::Ctx) -> AsyncResult<Self::Generic> {
        world.remove_non_send_resource().ok_or(AsyncFailure::ResourceNotFound)
    }
}

impl<A: Asset> AsyncAccess for AsyncAsset<A> {
    type Ctx = Handle<A>;
    type Ref<'t> = &'t A;
    type RefMut<'t> = &'t mut A;

    fn world(&self) -> &AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue)
    }

    fn as_ctx(&self) -> Self::Ctx {
        self.handle.clone_weak()
    }

    fn should_continue(err: AsyncFailure) -> bool {
        err == AsyncFailure::AssetNotFound
    }

    fn from_mut_world<'t>(world: &'t mut World, handle: &Self::Ctx) -> AsyncResult<Self::RefMut<'t>> {
        world.get_resource_mut::<Assets<A>>()
            .ok_or(AsyncFailure::ResourceNotFound)?
            .into_inner()
            .get_mut(handle)
            .ok_or(AsyncFailure::AssetNotFound)
    }
}

impl<A: Asset> AsyncReadonlyAccess for AsyncAsset<A> {
    fn from_ref_world<'t>(world: &'t World, handle: &Self::Ctx) -> AsyncResult<Self::Ref<'t>> {
        world.get_resource::<Assets<A>>()
            .ok_or(AsyncFailure::ResourceNotFound)?
            .get(handle)
            .ok_or(AsyncFailure::AssetNotFound)
    }
}

impl<A: Asset> AsyncAccessRef for AsyncAsset<A> {
    type Generic = A;
}

impl<A: Asset> AsyncTake for AsyncAsset<A> {
    fn take<'t>(world: &'t mut World, handle: &Self::Ctx) -> AsyncResult<Self::Generic> {
        world.get_resource_mut::<Assets<A>>().ok_or(AsyncFailure::ResourceNotFound)?
            .remove(handle).ok_or(AsyncFailure::AssetNotFound)
    }
}

impl<D: QueryData + 'static, F: QueryFilter + 'static> AsyncAccess for AsyncQuery<D, F> {
    type Ctx = ();
    type Ref<'t> = OwnedQueryState<'t, D, F>;
    type RefMut<'t> = OwnedQueryState<'t, D, F>;

    fn world(&self) -> &AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue)
    }

    fn as_ctx(&self) -> Self::Ctx {
        ()
    }

    fn from_mut_world<'t>(world: &'t mut World, _: &Self::Ctx) -> AsyncResult<Self::RefMut<'t>> {
        Ok(OwnedQueryState::new(world))
    }
}
