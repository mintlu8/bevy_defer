use std::marker::PhantomData;

use crate::{
    executor::{with_world_mut, with_world_ref},
    AccessError, AccessResult, AsyncWorld,
};
use bevy::ecs::{
    entity::Entity,
    world::{EntityRef, EntityWorldMut},
};

/// A mapped accessor of a component or a set of components.
#[allow(clippy::wrong_self_convention)]
pub trait DynAccess<T: ?Sized> {
    fn entity(&self) -> Entity;
    fn from_entity_ref<'t>(&self, entity: &'t EntityRef) -> AccessResult<&'t T>;
    fn from_entity_mut<'t>(&self, entity: &'t mut EntityWorldMut) -> AccessResult<&'t mut T>;

    fn map<U: 'static>(
        self,
        map_ref: impl Fn(&T) -> AccessResult<&U>,
        map_mut: impl Fn(&mut T) -> AccessResult<&mut U>,
    ) -> impl DynAccess<U>
    where
        T: 'static + Sized,
        Self: Sized,
    {
        MappedDynAccess {
            base: self,
            f1: map_ref,
            f2: map_mut,
            p: PhantomData,
        }
    }

    fn exists(&self) -> bool {
        let entity = self.entity();
        with_world_ref(|world| {
            world
                .get_entity(entity)
                .is_ok_and(|x| self.from_entity_ref(&x).is_ok())
        })
    }

    #[track_caller]
    fn get<U>(&self, f: impl FnOnce(&T) -> U) -> AccessResult<U> {
        let entity = self.entity();
        with_world_ref(|world| {
            let entity = world
                .get_entity(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(f(self.from_entity_ref(&entity)?))
        })
    }

    #[track_caller]
    fn get_mut<U>(&self, f: impl FnOnce(&mut T) -> U) -> AccessResult<U> {
        let entity = self.entity();
        with_world_mut(|world| {
            let mut entity = world
                .get_entity_mut(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(f(self.from_entity_mut(&mut entity)?))
        })
    }

    #[track_caller]
    fn cloned(&self) -> AccessResult<T>
    where
        T: Sized + Clone,
    {
        let entity = self.entity();
        AsyncWorld.read(|world| {
            let entity = world
                .get_entity(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(self.from_entity_ref(&entity)?.clone())
        })
    }
}

impl<T, U> DynAccess<U> for &T
where
    T: DynAccess<U>,
{
    fn entity(&self) -> Entity {
        (**self).entity()
    }

    fn from_entity_ref<'t>(&self, entity: &'t EntityRef) -> AccessResult<&'t U> {
        (**self).from_entity_ref(entity)
    }

    fn from_entity_mut<'t>(&self, entity: &'t mut EntityWorldMut) -> AccessResult<&'t mut U> {
        (**self).from_entity_mut(entity)
    }
}

struct MappedDynAccess<T, F1, F2, A, B> {
    base: T,
    f1: F1,
    f2: F2,
    p: PhantomData<(A, B)>,
}

impl<
        T: DynAccess<A>,
        F1: Fn(&A) -> AccessResult<&B>,
        F2: Fn(&mut A) -> AccessResult<&mut B>,
        A: 'static,
        B: 'static,
    > DynAccess<B> for MappedDynAccess<T, F1, F2, A, B>
{
    fn entity(&self) -> Entity {
        self.base.entity()
    }

    fn from_entity_ref<'t>(&self, entity: &'t EntityRef) -> AccessResult<&'t B> {
        (self.f1)(self.base.from_entity_ref(entity)?)
    }

    fn from_entity_mut<'t>(&self, entity: &'t mut EntityWorldMut) -> AccessResult<&'t mut B> {
        (self.f2)(self.base.from_entity_mut(entity)?)
    }
}

/// Map a `impl DynAccess` to one of its fields
///
/// # Example
///
/// ```
/// # /*
/// map_dyn_access!(item.field)
/// # */
/// ```
#[macro_export]
macro_rules! map_dyn_access {
    ([$($tt: tt)*].$ident: tt) => {
        $crate::access::DynAccess::map($($tt)*, |x| Ok(&x.$ident), |x| Ok(&mut x.$ident))
    };
    ([$($tt: tt)*] $fst: tt $($remaining: tt)*) => {
        $crate::map_dyn_access!([$($tt)* $fst] $($remaining)*)
    };
    ($($tt: tt)*) => {
        $crate::map_dyn_access!([] $($tt)*)
    };
}
