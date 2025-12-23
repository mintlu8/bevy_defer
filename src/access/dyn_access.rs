use crate::{AccessError, AccessResult, AsyncWorld};
use bevy::ecs::{
    entity::Entity,
    world::{EntityRef, EntityWorldMut},
};

/// A mapped accessor of a component or a set of components.
#[allow(clippy::wrong_self_convention)]
pub trait DynAccess {
    type Target: 'static;
    fn entity(&self) -> Entity;
    fn from_entity_ref<'t>(&self, entity: &'t EntityRef) -> AccessResult<&'t Self::Target>;
    fn from_entity_mut<'t>(
        &self,
        entity: &'t mut EntityWorldMut,
    ) -> AccessResult<&'t mut Self::Target>;

    fn map<T: 'static>(
        self,
        map_ref: impl Fn(&Self::Target) -> AccessResult<&T>,
        map_mut: impl Fn(&mut Self::Target) -> AccessResult<&mut T>,
    ) -> impl DynAccess<Target = T>
    where
        Self: Sized,
    {
        MappedDynAccess {
            base: self,
            f1: map_ref,
            f2: map_mut,
        }
    }

    fn exists(&self) -> bool {
        let entity = self.entity();
        AsyncWorld.run(|world| {
            world
                .get_entity(entity)
                .is_ok_and(|x| self.from_entity_ref(&x).is_ok())
        })
    }

    #[track_caller]
    fn get<T>(&self, f: impl FnOnce(&Self::Target) -> T) -> AccessResult<T> {
        let entity = self.entity();
        AsyncWorld.run(|world| {
            let entity = world
                .get_entity(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(f(self.from_entity_ref(&entity)?))
        })
    }

    #[track_caller]
    fn get_mut<T>(&mut self, f: impl FnOnce(&mut Self::Target) -> T) -> AccessResult<T> {
        let entity = self.entity();
        AsyncWorld.run(|world| {
            let mut entity = world
                .get_entity_mut(entity)
                .map_err(|_| AccessError::EntityNotFound(entity))?;
            Ok(f(self.from_entity_mut(&mut entity)?))
        })
    }
}

struct MappedDynAccess<T, F1, F2> {
    base: T,
    f1: F1,
    f2: F2,
}

impl<
        T: DynAccess,
        F1: Fn(&T::Target) -> AccessResult<&U>,
        F2: Fn(&mut T::Target) -> AccessResult<&mut U>,
        U: 'static,
    > DynAccess for MappedDynAccess<T, F1, F2>
{
    type Target = U;

    fn entity(&self) -> Entity {
        self.base.entity()
    }

    fn from_entity_ref<'t>(&self, entity: &'t EntityRef) -> AccessResult<&'t Self::Target> {
        (self.f1)(self.base.from_entity_ref(entity)?)
    }

    fn from_entity_mut<'t>(
        &self,
        entity: &'t mut EntityWorldMut,
    ) -> AccessResult<&'t mut Self::Target> {
        (self.f2)(self.base.from_entity_mut(entity)?)
    }
}
