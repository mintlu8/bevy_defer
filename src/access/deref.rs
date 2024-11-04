//! Traits for adding extension methods on asynchronous accessors to the `World` through `deref`.

use bevy::asset::Asset;
use bevy::ecs::{
    component::Component,
    query::{QueryData, QueryFilter},
    system::Resource,
};
use std::ops::Deref;

use super::{
    AsyncAsset, AsyncComponent, AsyncEntityQuery, AsyncNonSend, AsyncQuerySingle, AsyncResource,
};

/// Add method to [`struct@AsyncComponent`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncComponentDeref: Component + Sized {
    type Target;
    fn async_deref(this: &AsyncComponent<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncComponent<C>
where
    C: AsyncComponentDeref,
{
    type Target = <C as AsyncComponentDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncComponentDeref::async_deref(self)
    }
}

/// Add method to [`struct@AsyncResource`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncResourceDeref: Resource + Sized {
    type Target;
    fn async_deref(this: &AsyncResource<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncResource<C>
where
    C: AsyncResourceDeref,
{
    type Target = <C as AsyncResourceDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncResourceDeref::async_deref(self)
    }
}

/// Add method to [`struct@AsyncNonSend`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncNonSendDeref: 'static + Sized {
    type Target;
    fn async_deref(this: &AsyncNonSend<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncNonSend<C>
where
    C: AsyncNonSendDeref,
{
    type Target = <C as AsyncNonSendDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncNonSendDeref::async_deref(self)
    }
}

/// Add method to [`AsyncAsset`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncAssetDeref: Asset + Sized {
    type Target;
    fn async_deref(this: &AsyncAsset<Self>) -> &Self::Target;
}

impl<C> Deref for AsyncAsset<C>
where
    C: AsyncAssetDeref,
{
    type Target = <C as AsyncAssetDeref>::Target;

    fn deref(&self) -> &Self::Target {
        AsyncAssetDeref::async_deref(self)
    }
}

/// Add method to [`AsyncQuerySingle`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncQuerySingleDeref: QueryData + Sized {
    type Target<F: QueryFilter>;
    fn async_deref<F: QueryFilter>(this: &AsyncQuerySingle<Self, F>) -> &Self::Target<F>;
}

impl<C, F> Deref for AsyncQuerySingle<C, F>
where
    C: AsyncQuerySingleDeref,
    F: QueryFilter,
{
    type Target = <C as AsyncQuerySingleDeref>::Target<F>;

    fn deref(&self) -> &Self::Target {
        AsyncQuerySingleDeref::async_deref(self)
    }
}

/// Add method to [`AsyncEntityQuery`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncEntityQueryDeref: QueryData + Sized {
    type Target<F: QueryFilter>;
    fn async_deref<F: QueryFilter>(this: &AsyncEntityQuery<Self, F>) -> &Self::Target<F>;
}

impl<C, F> Deref for AsyncEntityQuery<C, F>
where
    C: AsyncEntityQueryDeref,
    F: QueryFilter,
{
    type Target = <C as AsyncEntityQueryDeref>::Target<F>;

    fn deref(&self) -> &Self::Target {
        AsyncEntityQueryDeref::async_deref(self)
    }
}
