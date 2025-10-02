use bevy::asset::{Asset, AssetId, Handle};
use bevy::ecs::{
    prelude::{Component, Entity, Resource},
    query::{QueryData, QueryFilter},
};
use std::borrow::Borrow;

use crate::{
    access::{
        AsyncAsset, AsyncComponent, AsyncEntityMut, AsyncEntityQuery, AsyncQuery, AsyncResource,
    },
    AsyncWorld,
};

/// Obtain a value from the `World`.
///
/// # Syntax
///
/// * `fetch!(entity, Type)`
///
/// Obtain a [`struct@AsyncComponent`] or [`AsyncEntityQuery`] depend on the type.
/// `entity` can be [`Entity`] or [`AsyncEntityMut`].
///
/// * `fetch!(#expr)`
///
/// Obtain a [`AsyncEntityMut`] or [`AsyncAsset`] from an underlying expr.
///
/// * `fetch!(Type)`
///
/// Obtain a [`struct@AsyncResource`] or [`AsyncQuery`] depend on the type.
#[macro_export]
macro_rules! fetch {
    ($res: ty) => {
        $crate::fetch0::<$res, _>()
    };
    (#$expr: expr) => {
        $crate::fetch1(&$expr)
    };
    ($entity: expr, $comp: ty $(,)?) => {
        $crate::fetch::<$comp, _>(&$entity)
    };
    ($entity: expr, $data: ty, $filter: ty $(,)?) => {
        $crate::fetch2::<$data, $filter>(&$entity)
    };
}

pub trait FetchWorld<M = ()> {
    type Out;

    fn fetch() -> Self::Out;
}

pub trait FetchEntity<M = ()> {
    type Out;

    fn fetch(entity: &impl Borrow<Entity>) -> Self::Out;
}

pub trait FetchOne<M = ()> {
    type Out;

    fn fetch(&self) -> Self::Out;
}

pub struct ResourceMarker;
pub struct ComponentMarker;
pub struct QueryMarker;
pub struct QueryFilteredMarker;
pub struct AssetMarker;

impl<T: Resource> FetchWorld<ResourceMarker> for T {
    type Out = AsyncResource<T>;

    fn fetch() -> Self::Out {
        AsyncWorld.resource::<Self>()
    }
}

impl<T: QueryData> FetchWorld<QueryMarker> for T {
    type Out = AsyncQuery<T>;

    fn fetch() -> Self::Out {
        AsyncWorld.query::<T>()
    }
}

impl<T: QueryData, F: QueryFilter> FetchWorld<QueryFilteredMarker> for (T, F) {
    type Out = AsyncQuery<T, F>;

    fn fetch() -> Self::Out {
        AsyncWorld.query_filtered::<T, F>()
    }
}

impl<T: Component> FetchEntity<ComponentMarker> for T {
    type Out = AsyncComponent<T>;

    fn fetch(entity: &impl Borrow<Entity>) -> Self::Out {
        AsyncWorld.entity(*entity.borrow()).component::<T>()
    }
}

impl<T: QueryData> FetchEntity<QueryMarker> for T {
    type Out = AsyncEntityQuery<T>;

    fn fetch(entity: &impl Borrow<Entity>) -> Self::Out {
        AsyncWorld.entity(*entity.borrow()).query::<T>()
    }
}

impl<T: Borrow<Entity>> FetchOne<ComponentMarker> for T {
    type Out = AsyncEntityMut;

    fn fetch(&self) -> Self::Out {
        AsyncWorld.entity(*self.borrow())
    }
}

impl<A: Asset> FetchOne<AssetMarker> for Handle<A> {
    type Out = AsyncAsset<A>;

    fn fetch(&self) -> Self::Out {
        AsyncAsset::Weak(self.id())
    }
}

impl<A: Asset> FetchOne<AssetMarker> for AssetId<A> {
    type Out = AsyncAsset<A>;

    fn fetch(&self) -> Self::Out {
        AsyncAsset::Weak(*self)
    }
}

impl<A: Asset> FetchOne<AssetMarker> for &Handle<A> {
    type Out = AsyncAsset<A>;

    fn fetch(&self) -> Self::Out {
        AsyncAsset::Weak(self.id())
    }
}

impl<A: Asset> FetchOne<AssetMarker> for &AssetId<A> {
    type Out = AsyncAsset<A>;

    fn fetch(&self) -> Self::Out {
        AsyncAsset::Weak(**self)
    }
}

impl<A: Asset> FetchOne<AssetMarker> for AsyncAsset<A> {
    type Out = AsyncAsset<A>;

    fn fetch(&self) -> Self::Out {
        self.clone()
    }
}

pub fn fetch0<T: FetchWorld<M>, M>() -> T::Out {
    T::fetch()
}

pub fn fetch1<T: FetchOne<M>, M>(item: &T) -> T::Out {
    T::fetch(item)
}

pub fn fetch2<Q: QueryData, F: QueryFilter>(entity: impl Borrow<Entity>) -> AsyncEntityQuery<Q, F> {
    AsyncWorld.entity(*entity.borrow()).query_filtered::<Q, F>()
}

pub fn fetch<T: FetchEntity<M>, M>(entity: &impl Borrow<Entity>) -> T::Out {
    T::fetch(entity)
}

#[cfg(test)]
mod text {
    use crate::access::AsyncAsset;
    use crate::AsyncWorld;
    use bevy::asset::{AssetId, Handle};
    use bevy::diagnostic::SystemInfo;
    use bevy::prelude::{Entity, GlobalTransform, Image, Transform, With};

    #[test]
    fn test_fetch() {
        let e1 = Entity::PLACEHOLDER;
        let e2 = &e1;
        let e3 = AsyncWorld.entity(e1);
        let e4 = &e3;
        let _a = fetch!(e1, Transform);
        let _b = fetch!(e2, &Transform);
        let _c = fetch!(e3, (&Transform, &GlobalTransform));
        let _d = fetch!(e4, &Transform, With<GlobalTransform>);
        let _a = fetch!(#e1);
        let _b = fetch!(#e2);
        let _c = fetch!(#e3);
        let _d = fetch!(#e4);
        let _a = fetch!(SystemInfo);
        let _b = fetch!(&Transform);
        let _c = fetch!((&Transform, &GlobalTransform));
        let _d = fetch!((&Transform, With<GlobalTransform>));
        let a1 = Handle::<Image>::default();
        let a2 = AssetId::<Image>::default();
        let a3 = AsyncAsset::new_weak(&a1);
        let a4 = &a1;
        let a5 = &a2;
        let _a = fetch!(#a1);
        let _b = fetch!(#a2);
        let _c = fetch!(#a3);
        let _d = fetch!(#a4);
        let _e = fetch!(#a5);
    }
}
