use std::{borrow::BorrowMut, cell::OnceCell};

use bevy_ecs::{component::Component, entity::Entity, system::Resource, world::World};
use futures::{future::{ready, Either}, Future};
use ref_cast::RefCast;

use crate::{async_values::{AsyncComponent, AsyncNonSend, AsyncResource}, async_world::AsyncWorldMut, cancellation::TaskCancellation, channel, locals::with_world_ref, tween::{AsSeconds, Lerp, Playback}, AsyncFailure, AsyncResult, CHANNEL_CLOSED};

pub trait AsyncReadonlyAccess: AsyncAccess {
    fn from_ref_world(world: &World, cx: Self::Ctx) -> AsyncResult<Self::Ref<'_>>;
}

pub trait AsyncAccessRef: 
        for<'t> AsyncAccess<RefMut<'t> = &'t mut Self::Generic> +  
        for<'t> AsyncReadonlyAccess<Ref<'t> = &'t Self::Generic> {
    type Generic: 'static;
}

pub trait AsyncAccess {
    type Ctx: Copy + 'static;
    type Ref<'t>;
    type RefMut<'t>;

    fn world(&self) -> &AsyncWorldMut;
    fn as_ctx(&self) -> Self::Ctx;
    fn from_mut_world(world: &mut World, cx: Self::Ctx) -> AsyncResult<Self::RefMut<'_>>;

    fn set<T: 'static>(&self, f: impl FnOnce(Self::RefMut<'_>) -> T + 'static) -> impl Future<Output = AsyncResult<T>> + 'static{
        let ctx = self.as_ctx();
        self.world().run(move |w| Ok(f(Self::from_mut_world(w, ctx)?)))
    }


    fn watch<T: 'static>(&self, mut f: impl FnMut(Self::RefMut<'_>) -> Option<T> + 'static) -> impl Future<Output = AsyncResult<T>> + 'static{
        let ctx = self.as_ctx();
        self.world().watch(move |w| match Self::from_mut_world(w, ctx) {
            Ok(result) => f(result).map(Ok),
            Err(err) => Some(Err(err)),
        })
    }

    fn exists(&self) -> impl Future<Output = ()> {
        use futures::FutureExt;
        let (sender, receiver) = channel();
        let ctx = self.as_ctx();
        self.world().queue.repeat(
            move |world: &mut World| {
                Self::from_mut_world(world, ctx).ok().map(|_| ())
            },
            sender
        );
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }


    fn get<T: 'static>(&self, f: impl FnOnce(Self::Ref<'_>) -> T + 'static) -> impl Future<Output = AsyncResult<T>> + 'static where Self: AsyncReadonlyAccess{
        let ctx = self.as_ctx();
        let f = move |world: &World| {
            Ok(f(Self::from_ref_world(world, ctx)?))
        }; 
        let f = match with_world_ref(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        Either::Left(self.world().run(|w| f(w)))
    }

    fn cloned<'a>(&self) -> impl Future<Output = AsyncResult<Self::Generic>> where Self: AsyncAccessRef, Self::Generic: Clone {
        self.get(move |a| a.clone())
    }

    /// Interpolate to a new value from the previous value.
    fn interpolate_to<V: Lerp>(
        &self, 
        to: V,
        mut get: impl FnMut(Self::RefMut<'_>) -> V + Send + 'static,
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
                let component = Self::from_mut_world(world, ctx).unwrap();
                let source = source.get_or_init(||get(component)).clone();
                t += dt.as_secs_f32();
                if t > duration {
                    set(component.borrow_mut(), to.clone());
                    Some(Ok(()))
                } else {
                    let fac = curve(t / duration);
                    set(component.borrow_mut(), V::lerp(source, to.clone(), fac));
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
                let component = Self::from_mut_world(world, ctx).unwrap();
                t += dt.as_secs_f32() / duration;
                let fac = if t > 1.0 {
                    match playback {
                        Playback::Once => {
                            write(component, span(1.0));
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
                write(component, span(curve(fac)));
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

    fn from_mut_world(world: &mut World, cx: Self::Ctx) -> AsyncResult<Self::RefMut<'_>> {
        world.get_mut::<C>(cx)
            .ok_or(AsyncFailure::ComponentNotFound)
            .map(|x| x.into_inner())
    }
}

impl<C: Component> AsyncReadonlyAccess for AsyncComponent<C> {

    fn from_ref_world(world: &World, cx: Self::Ctx) -> AsyncResult<Self::Ref<'_>> {
        world.get_entity(cx)
            .ok_or(AsyncFailure::EntityNotFound)?
            .get::<C>()
            .ok_or(AsyncFailure::ComponentNotFound)
    }
}

impl<C: Component> AsyncAccessRef for AsyncComponent<C> {
    type Generic = C;
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

    fn from_mut_world(world: &mut World, _: Self::Ctx) -> AsyncResult<Self::RefMut<'_>> {
        world.get_resource_mut::<R>()
            .ok_or(AsyncFailure::ResourceNotFound)
            .map(|x| x.into_inner())
    }
}

impl<R: Resource> AsyncReadonlyAccess for AsyncResource<R> {

    fn from_ref_world(world: &World, _: Self::Ctx) -> AsyncResult<Self::Ref<'_>> {
        world.get_resource().ok_or(AsyncFailure::ResourceNotFound)
    }
}

impl<R: Resource> AsyncAccessRef for AsyncResource<R> {
    type Generic = R;
}

impl<R: Resource> AsyncAccess for AsyncNonSend<R> {
    type Ctx = ();
    type Ref<'t> = &'t R;
    type RefMut<'t> = &'t mut R;

    fn world(&self) -> &AsyncWorldMut {
        AsyncWorldMut::ref_cast(&self.queue)
    }

    fn as_ctx(&self) -> Self::Ctx {
        ()
    }

    fn from_mut_world(world: &mut World, _: Self::Ctx) -> AsyncResult<Self::RefMut<'_>> {
        world.get_non_send_resource_mut::<R>()
            .ok_or(AsyncFailure::ResourceNotFound)
            .map(|x| x.into_inner())
    }
}

impl<R: Resource> AsyncReadonlyAccess for AsyncNonSend<R> {

    fn from_ref_world(world: &World, _: Self::Ctx) -> AsyncResult<Self::Ref<'_>> {
        world.get_non_send_resource().ok_or(AsyncFailure::ResourceNotFound)
    }
}

impl<R: Resource> AsyncAccessRef for AsyncNonSend<R> {
    type Generic = R;
}