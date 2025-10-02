//! Access traits for `bevy_defer`.

use crate::access::{
    AsyncAsset, AsyncComponent, AsyncEntityQuery, AsyncNonSend, AsyncQuery, AsyncQuerySingle,
    AsyncRelatedQuery, AsyncResource, AsyncWorld, RelatedQueryState,
};
use crate::tween::{AsSeconds, Playback};
use crate::OwnedQueryState;
use crate::{
    cancellation::TaskCancellation,
    executor::{with_world_mut, with_world_ref},
    sync::oneshot::{ChannelOut, InterpolateOut},
    AccessError, AccessResult,
};
use bevy::asset::{Asset, Assets};
use bevy::ecs::component::Mutable;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::ecs::{
    component::Component,
    query::{QueryData, QueryFilter},
    resource::Resource,
};
use bevy::math::StableInterpolate;
use std::any::type_name;
use std::cell::OnceCell;
use std::marker::PhantomData;

trait ShouldContinue {
    fn should_continue(_e: AccessError) -> bool {
        false
    }
}

macro_rules! inject {
    ($var: ident $fst: expr) => {
        let $var = $fst;
    };

    ($var: ident $fst: stmt; $($stmts: tt)*) => {
        inject!([$fst]$var $($stmts)*)
    };

    ([$($prev: stmt);*] $var: ident $fst: expr) => {
        $($prev)*
        let $var = $fst;
    };

    ([$($prev: stmt);*] $var: ident $fst: stmt; $($stmts: tt)*) => {
        inject!([$($prev;)* $fst] $var $($stmts)*)
    };
}

macro_rules! tri {
    ($($tt:tt)*) => {
        (|| {$($tt)*})()
    };
}

macro_rules! impl_async_access {
    ($($tt: tt)*) => {
        impl_async_access1!($($tt)*);
        impl_async_access2!($($tt)*);
    }
}

macro_rules! impl_async_access1 {
    (impl[$($impl_generics:tt)*] $ty: ident [$($ty_generics:tt)*] {}) => {};
    (
        impl[$($impl_generics:tt)*] $ty: ident [$($ty_generics:tt)*] {
            fn get($this: ident: &Self, $world: ident: &World) -> AccessResult<$Ref: ty> {
                $($stmts: tt)*
            }

            $($remaining: tt)*
        }
    ) => {
        #[allow(unused)]
        impl<$($impl_generics)*> $ty <$($ty_generics)*> {
            /// Run a function on a readonly reference to this item and obtain the result.
            pub fn get<A>(&self, f: impl FnOnce($Ref) -> A) -> AccessResult<A> {
                let $this = self;
                with_world_ref(|$world|{
                    inject!(out $($stmts)*);
                    Ok(f(out?))
                })
            }

            /// Run a function on this item and obtain the result once loaded.
            pub fn get_on_load<A: 'static>(
                &self,
                mut f: impl FnMut($Ref) -> A + 'static,
            ) -> ChannelOut<AccessResult<A>> {
                let $this = self.clone();
                AsyncWorld.watch(move |$world| {
                    let out = tri!{
                        inject!(out $($stmts)*);
                        Ok(f(out?))
                    };

                    match out {
                        Ok(value) => Some(Ok(value)),
                        Err(err) if <Self as ShouldContinue>::should_continue(err) => None,
                        Err(err) => Some(Err(err))
                    }
                })
            }

            /// Check if item exists.
            pub fn exists(&self) -> bool {
                let $this = self;
                with_world_ref::<AccessResult<bool>>(|$world|{
                    inject!(out $($stmts)*);
                    Ok(out.is_ok())
                }).is_ok()
            }
        }

        impl_async_access1!(
            impl[$($impl_generics)*] $ty [$($ty_generics)*] {
                $($remaining)*
            }
        );
    };
    (
        impl[$($impl_generics:tt)*] $ty: ident [$($ty_generics:tt)*] {
            fn take($this: ident: &Self, $world: ident: &mut World) -> AccessResult<$Ref: ty> {
                $($stmts: tt)*
            }

            $($remaining: tt)*
        }
    ) => {
        #[allow(unused)]
        impl<$($impl_generics)*> $ty <$($ty_generics)*> {
            /// Remove the item from the world.
            pub fn remove(&self) {
                let $this = self;
                with_world_mut(|$world|{
                    inject!(out $($stmts)*);
                    AccessResult::Ok(())
                });
            }

            /// Remove and obtain the item from the world.
            pub fn take(&self) -> AccessResult<$Ref> {
                let $this = self;
                with_world_mut(|$world|{
                    inject!(out $($stmts)*);
                    out
                })
            }

            /// Remove and obtain the item from the world once loaded.
            pub fn take_on_load(&self) -> ChannelOut<AccessResult<$Ref>> {
                let $this = self.clone();
                AsyncWorld.watch(move |$world| {
                    let out = tri! {
                        inject!(out $($stmts)*);
                        out
                    };
                    match out {
                        Ok(value) => Some(Ok(value)),
                        Err(err) if <Self as ShouldContinue>::should_continue(err) => None,
                        Err(err) => Some(Err(err))
                    }
                })
            }
        }
    };
    (
        impl[$($impl_generics:tt)*] $ty: ident [$($ty_generics:tt)*] {
            fn get_mut($this: ident: &Self, $world: ident: &mut World) -> AccessResult<$Ref: ty> {
                $($stmts: tt)*
            }

            $($remaining: tt)*
        }
    ) => {
        #[allow(unused)]
        impl<$($impl_generics)*> $ty <$($ty_generics)*> {
            /// Run a function on a readonly reference to this item and obtain the result.
            pub fn get_mut<A>(&self, f: impl FnOnce($Ref) -> A) -> AccessResult<A> {
                let $this = self;
                with_world_mut(|$world|{
                    inject!(out $($stmts)*);
                    let result = f(out?);
                    Ok(result)
                })
            }

            /// Run a function on this item until it returns `Some`.
            pub fn watch<A: 'static>(
                &self,
                mut f: impl FnMut($Ref) -> Option<A> + 'static,
            ) -> ChannelOut<AccessResult<A>> {
                let $this = self.clone();
                AsyncWorld.watch(move |$world| {
                    let out = (|| {
                        inject!(out $($stmts)*);
                        let result = f(out?);
                        Ok(result)
                    })();

                    match out {
                        Ok(Some(value)) => Some(Ok(value)),
                        Ok(None) => None,
                        Err(err) if <Self as ShouldContinue>::should_continue(err) => None,
                        Err(err) => Some(Err(err))
                    }
                })
            }

            /// Run a function on this item and obtain the result once loaded.
            pub fn get_mut_on_load<A: 'static>(
                &self,
                mut f: impl FnMut($Ref) -> A + 'static,
            ) -> ChannelOut<AccessResult<A>> {
                let $this = self.clone();
                AsyncWorld.watch(move |$world| {
                    let out = tri! {
                        inject!(out $($stmts)*);
                        let result = f(out?);
                        Ok(result)
                    };

                    match out {
                        Ok(value) => Some(Ok(value)),
                        Err(err) if <Self as ShouldContinue>::should_continue(err) => None,
                        Err(err) => Some(Err(err))
                    }
                })
            }
        }

        impl_async_access1!(
            impl[$($impl_generics)*] $ty [$($ty_generics)*] {
                $($remaining)*
            }
        );
    };
}

macro_rules! impl_async_access2 {
    (impl[$($impl_generics:tt)*] $ty: ident [$($ty_generics:tt)*] {}) => {};

    (
        impl[$($impl_generics:tt)*] $ty: ident [$($ty_generics:tt)*] {
            fn get($this: ident: &Self, $world: ident: &World) -> AccessResult<&$Ref: ty> {
                $($stmts: tt)*
            }

            $($remaining: tt)*
        }
    ) => {
        #[allow(unused)]
        impl<$($impl_generics)*> $ty <$($ty_generics)*> {
            /// Obtain a copy of the underlying item.
            pub fn copied(&self) -> AccessResult<$Ref> where $Ref: Copy {
                let $this = self;
                with_world_ref(|$world|{
                    inject!(out $($stmts)*);
                    Ok(*(out?))
                })
            }

            /// Obtain a clone of the underlying item.
            pub fn cloned(&self) -> AccessResult<$Ref> where $Ref: Clone {
                let $this = self;
                with_world_ref(|$world|{
                    inject!(out $($stmts)*);
                    Ok((out?).clone())
                })
            }

            /// Run a function on this item and obtain the result once loaded.
            pub fn clone_on_load(&self) -> ChannelOut<AccessResult<$Ref>> where $Ref: Clone {
                let $this = self.clone();
                AsyncWorld.watch(move |$world| {
                    let out = tri!{
                        inject!(out $($stmts)*);
                        Ok((out?).clone())
                    };

                    match out {
                        Ok(value) => Some(Ok(value)),
                        Err(err) if <Self as ShouldContinue>::should_continue(err) => None,
                        Err(err) => Some(Err(err))
                    }
                })
            }
        }


        impl_async_access2!(
            impl[$($impl_generics)*] $ty [$($ty_generics)*] {
                $($remaining)*
            }
        );
    };

    (
        impl[$($impl_generics:tt)*] $ty: ident [$($ty_generics:tt)*] {
            fn get_mut($this: ident: &Self, $world: ident: &mut World) -> AccessResult<&mut $Ref: ty> {
                $($stmts: tt)*
            }

            $($remaining: tt)*
        }
    ) => {
        #[allow(unused)]
        impl<$($impl_generics)*> $ty <$($ty_generics)*> {
            /// Interpolate to a new value from the previous value.
            pub fn interpolate_to<V: StableInterpolate + 'static>(
                &self,
                to: V,
                mut get: impl FnMut(&$Ref) -> V + Send + 'static,
                mut set: impl FnMut(&mut $Ref, V) + Send + 'static,
                mut curve: impl FnMut(f32) -> f32 + Send + 'static,
                duration: impl AsSeconds,
                cancel: impl Into<TaskCancellation>,
            ) -> InterpolateOut {
                let $this = self.clone();
                let mut t = 0.0;
                let duration = duration.as_secs();
                let source = OnceCell::<V>::new();
                let cancel = cancel.into();
                AsyncWorld
                    .timed_routine(
                        move |$world, dt| {
                            tri! {
                                inject!(out $($stmts)*);
                                let item = out?;
                                t += dt.as_secs_f32();
                                let source = source.get_or_init(|| get(item)).clone();
                                if t > duration {
                                    set(item, to.clone());
                                    Ok(Ok(()))
                                } else {
                                    let fac = curve(t / duration);
                                    set(item, V::interpolate_stable(&source, &to, fac));
                                    Err(AccessError::ShouldNotHappen)
                                }
                            }.ok()
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
            pub fn interpolate<V>(
                &self,
                mut span: impl FnMut(f32) -> V + 'static,
                mut write: impl FnMut(&mut $Ref, V) + 'static,
                mut curve: impl FnMut(f32) -> f32 + 'static,
                duration: impl AsSeconds,
                playback: Playback,
                cancel: impl Into<TaskCancellation>,
            ) -> InterpolateOut {
                let $this = self.clone();
                let duration = duration.as_secs();
                let mut t = 0.0;
                let cancel = cancel.into();
                AsyncWorld
                    .timed_routine(
                        move |$world, dt| {
                            tri! {
                                inject!(out $($stmts)*);
                                let item = out?;
                                t += dt.as_secs_f32() / duration;
                                let fac = if t > 1.0 {
                                    match playback {
                                        Playback::Once => {
                                            write(item, span(curve(1.0)));
                                            return Ok(Ok(()));
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
                                Err(AccessError::ShouldNotHappen)
                            }.ok()
                        },
                        cancel,
                    )
                    .into_interpolate_out()
            }
        }

        impl_async_access2!(
            impl[$($impl_generics)*] $ty [$($ty_generics)*] {
                $($remaining)*
            }
        );
    };

    (
        impl[$($impl_generics:tt)*] $ty: ident [$($ty_generics:tt)*] {
            fn $name: ident ($($a:tt)*) -> $b: ty {$($c:tt)*}
            $($remaining: tt)*
        }
    ) => {
        impl_async_access2!(
            impl[$($impl_generics)*] $ty [$($ty_generics)*] {
                $($remaining)*
            }
        );
    }
}

impl_async_access! {
    impl[C: Component<Mutability = Mutable>] AsyncComponent [C] {
        fn get_mut(this: &Self, world: &mut World) -> AccessResult<&mut C> {
            let entity = this.id();
            let mut entity_mut = world
                .get_entity_mut(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            entity_mut
                .get_mut::<C>()
                .map(|x| x.into_inner())
                .ok_or(AccessError::ComponentNotFound {
                    name: type_name::<C>(),
                })
        }
    }
}

/// Not a loading resource, ignore.
impl<C: Component> ShouldContinue for AsyncComponent<C> {}

impl_async_access! {
    impl[C: Component] AsyncComponent [C] {
        fn get(this: &Self, world: &World) -> AccessResult<&C> {
            let entity = this.id();
            world
                .get_entity(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?
                .get::<C>()
                .ok_or(AccessError::ComponentNotFound {
                    name: type_name::<C>(),
                })
        }

        fn take(this: &Self, world: &mut World) -> AccessResult<C> {
            let entity = this.id();
            world
                .get_entity_mut(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?
                .take::<C>()
                .ok_or(AccessError::ComponentNotFound {
                    name: type_name::<C>(),
                })
        }
    }
}

impl<R: Resource> ShouldContinue for AsyncResource<R> {
    fn should_continue(e: AccessError) -> bool {
        e == AccessError::ResourceNotFound {
            name: type_name::<R>(),
        }
    }
}

impl_async_access! {
    impl[R: Resource] AsyncResource [R] {
        fn get(this: &Self, world: &World) -> AccessResult<&R> {
            world.get_resource::<R>().ok_or(AccessError::ResourceNotFound {
                name: type_name::<R>(),
            })
        }

        fn get_mut(this: &Self, world: &mut World) -> AccessResult<&mut R> {
            world.get_resource_mut::<R>()
                .map(|x| x.into_inner())
                .ok_or(AccessError::ResourceNotFound {
                    name: type_name::<R>(),
                })
        }
    }
}

impl<R: 'static> ShouldContinue for AsyncNonSend<R> {
    fn should_continue(e: AccessError) -> bool {
        e == AccessError::ResourceNotFound {
            name: type_name::<R>(),
        }
    }
}

impl_async_access! {
    impl[R: 'static] AsyncNonSend [R] {
        fn get(this: &Self, world: &World) -> AccessResult<&R> {
            world.get_non_send_resource::<R>().ok_or(AccessError::ResourceNotFound {
                name: type_name::<R>(),
            })
        }

        fn get_mut(this: &Self, world: &mut World) -> AccessResult<&mut R> {
            world.get_non_send_resource_mut::<R>()
                .map(|x| x.into_inner())
                .ok_or(AccessError::ResourceNotFound {
                    name: type_name::<R>(),
                })
        }
    }
}

impl<T: Asset> ShouldContinue for AsyncAsset<T> {
    fn should_continue(e: AccessError) -> bool {
        e == AccessError::AssetNotFound {
            name: type_name::<T>(),
        }
    }
}

impl_async_access! {
    impl[T: Asset] AsyncAsset [T] {
        fn get(this: &Self, world: &World) -> AccessResult<&T> {
            let id = this.id();
            world
                .get_resource::<Assets<T>>()
                .ok_or(AccessError::ResourceNotFound {
                    name: type_name::<Assets<T>>(),
                })?
                .get(id)
                .ok_or(AccessError::AssetNotFound {
                    name: type_name::<T>(),
                })
        }

        fn get_mut(this: &Self, world: &mut World) -> AccessResult<&mut T> {
            let id = this.id();
            world
                .get_resource_mut::<Assets<T>>()
                .map(|x| x.into_inner())
                .ok_or(AccessError::ResourceNotFound {
                    name: type_name::<Assets<T>>(),
                })?
                .get_mut(id)
                .ok_or(AccessError::AssetNotFound {
                    name: type_name::<T>(),
                })
        }

        fn take(this: &Self, world: &mut World) -> AccessResult<T> {
            let id = this.id();
            world
                .get_resource_mut::<Assets<T>>()
                .map(|x| x.into_inner())
                .ok_or(AccessError::ResourceNotFound {
                    name: type_name::<Assets<T>>(),
                })?
                .remove(id)
                .ok_or(AccessError::AssetNotFound {
                    name: type_name::<T>(),
                })
        }
    }
}

impl<D: QueryData, F: QueryFilter> ShouldContinue for AsyncQuery<D, F> {}

impl_async_access! {
    impl[D: QueryData + 'static, F: QueryFilter + 'static] AsyncQuery [D, F] {
        fn get_mut(this: &Self, world: &mut World) -> AccessResult<OwnedQueryState<D, F>> {
            Ok(OwnedQueryState::<D, F>::new(world))
        }
    }
}

impl<D: QueryData, F: QueryFilter> ShouldContinue for AsyncEntityQuery<D, F> {}

impl_async_access! {
    impl[D: QueryData + 'static, F: QueryFilter + 'static] AsyncEntityQuery [D, F] {
        fn get_mut(this: &Self, world: &mut World) -> AccessResult<D::Item<'_, '_>> {
            let entity = this.id();
            let mut query = OwnedQueryState::<D, F>::new(world);
            query.get_mut(entity)
        }
    }
}

impl<D: QueryData, F: QueryFilter> ShouldContinue for AsyncQuerySingle<D, F> {}

impl_async_access! {
    impl[D: QueryData + 'static, F: QueryFilter + 'static] AsyncQuerySingle [D, F] {
        fn get_mut(this: &Self, world: &mut World) -> AccessResult<D::Item<'_, '_>> {
            let mut query = OwnedQueryState::<D, F>::new(world);
            query.single_mut()
        }
    }
}

impl<R: RelationshipTarget, D: QueryData, F: QueryFilter> ShouldContinue
    for AsyncRelatedQuery<R, D, F>
{
}

impl_async_access! {
    impl[R: RelationshipTarget + 'static, D: QueryData + 'static, F: QueryFilter + 'static] AsyncRelatedQuery [R, D, F] {
        fn get_mut(this: &Self, world: &mut World) -> AccessResult<RelatedQueryState<R, D, F>> {
            let parent = this.id();
            let mut query = OwnedQueryState::<D, F>::new(world);
            Ok(RelatedQueryState::<R, D, F> {
                world: query.world.as_unsafe_world_cell(),
                query: query.state.as_mut().unwrap(),
                parent,
                p: PhantomData,
            })
        }
    }
}
