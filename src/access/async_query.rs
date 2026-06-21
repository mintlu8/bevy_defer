use crate::access::AsyncEntity;
use crate::executor::with_world_mut;
use crate::{access::get_entity::VirtualEntity, OwnedQueryState};
use bevy::ecs::query::IterQueryData;
#[allow(unused)]
use bevy::ecs::system::Query;
use bevy::ecs::{
    entity::Entity,
    query::{QueryData, QueryFilter},
};
use std::any::type_name;
use std::fmt::Debug;
use std::{borrow::Borrow, marker::PhantomData, ops::Deref};

/// Async version of [`Query`]
pub struct AsyncQuery<T: QueryData, F: QueryFilter = ()>(pub(crate) PhantomData<(T, F)>);

impl<T: QueryData, F: QueryFilter> Debug for AsyncQuery<T, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncQuery")
            .field("data", &type_name::<T>())
            .field("filter", &type_name::<F>())
            .finish()
    }
}

impl<T: QueryData, F: QueryFilter> Copy for AsyncQuery<T, F> {}

impl<T: QueryData, F: QueryFilter> Clone for AsyncQuery<T, F> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Async version of [`Query`] on a specific entity.
pub struct AsyncEntityQuery<T: QueryData, F: QueryFilter = (), E: VirtualEntity = Entity> {
    pub(crate) entity: E,
    pub(crate) p: PhantomData<(T, F)>,
}

impl<T: QueryData, F: QueryFilter, E: VirtualEntity + Debug> Debug for AsyncEntityQuery<T, F, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncEntityQuery")
            .field("entity", &self.entity)
            .field("data", &type_name::<T>())
            .field("filter", &type_name::<F>())
            .field("entity", &self.entity)
            .finish()
    }
}

impl<T: QueryData, F: QueryFilter> AsyncEntityQuery<T, F> {
    pub fn id(&self) -> Entity {
        self.entity
    }
}

impl<T: QueryData, F: QueryFilter, E: VirtualEntity> AsyncEntityQuery<T, F, E> {
    pub fn entity(self) -> AsyncEntity<E> {
        AsyncEntity(self.entity)
    }
}

impl<T: QueryData, F: QueryFilter, E: VirtualEntity + Copy> Copy for AsyncEntityQuery<T, F, E> {}

impl<T: QueryData, F: QueryFilter, E: VirtualEntity + Clone> Clone for AsyncEntityQuery<T, F, E> {
    fn clone(&self) -> Self {
        AsyncEntityQuery {
            entity: self.entity.clone(),
            p: PhantomData,
        }
    }
}

/// Async version of [`Query`] on a unique entity.
pub struct AsyncQuerySingle<T: QueryData, F: QueryFilter = ()>(pub(crate) PhantomData<(T, F)>);

impl<T: QueryData, F: QueryFilter> Debug for AsyncQuerySingle<T, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncQuerySingle")
            .field("data", &type_name::<T>())
            .field("filter", &type_name::<F>())
            .finish()
    }
}

impl<T: QueryData, F: QueryFilter> Copy for AsyncQuerySingle<T, F> {}

impl<T: QueryData, F: QueryFilter> Clone for AsyncQuerySingle<T, F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: QueryData, F: QueryFilter> AsyncQuery<T, F> {
    /// Obtain an [`AsyncEntityQuery`] on a specific entity.
    pub fn entity(&self, entity: impl Borrow<Entity>) -> AsyncEntityQuery<T, F> {
        AsyncEntityQuery {
            entity: *entity.borrow(),
            p: PhantomData,
        }
    }

    /// Obtain an [`AsyncQuerySingle`] on a single entity.
    pub fn single(&self) -> AsyncQuerySingle<T, F> {
        AsyncQuerySingle(PhantomData)
    }
}

impl<T: IterQueryData + 'static, F: QueryFilter + 'static> AsyncQuery<T, F> {
    /// Run a function on the iterator.
    pub fn for_each(&self, mut f: impl FnMut(T::Item<'_, '_>)) {
        with_world_mut(move |w| {
            let mut state = OwnedQueryState::<T, F>::new(w);
            for item in state.iter_mut() {
                f(item);
            }
        })
    }
}

/// Add method to [`AsyncQuery`] through deref.
///
/// It is recommended to derive [`RefCast`](ref_cast) for this.
pub trait AsyncQueryDeref: QueryData + Sized {
    type Target<F: QueryFilter>;
    fn async_deref<F: QueryFilter>(this: &AsyncQuery<Self, F>) -> &Self::Target<F>;
}

impl<C, F> Deref for AsyncQuery<C, F>
where
    C: AsyncQueryDeref,
    F: QueryFilter,
{
    type Target = <C as AsyncQueryDeref>::Target<F>;

    fn deref(&self) -> &Self::Target {
        AsyncQueryDeref::async_deref(self)
    }
}
