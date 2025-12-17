use std::{any::type_name, fmt::Debug, marker::PhantomData};

use bevy::ecs::{component::Component, entity::Entity};

use crate::access::{AsyncComponent, AsyncEntityMut};

pub struct Dyn<A: 'static + ?Sized, C: Component>(AsyncComponent<C>, PhantomData<A>);

impl<A: 'static + ?Sized, C: Component> Debug for Dyn<A, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dyn")
            .field("type", &type_name::<C>())
            .field("entity", &self.0.id())
            .finish()
    }
}

impl<A: 'static + ?Sized, C: Component> Copy for Dyn<A, C> {}

impl<A: 'static + ?Sized, C: Component> Clone for Dyn<A, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: 'static + ?Sized, C: Component> Dyn<A, C> {
    pub fn id(&self) -> Entity {
        self.0.id()
    }

    pub fn entity(&self) -> AsyncEntityMut {
        self.0.entity()
    }

    pub fn component(&self) -> AsyncComponent<C> {
        self.0
    }
}

impl<A: 'static + ?Sized, C: Component> From<AsyncComponent<C>> for Dyn<A, C> {
    fn from(value: AsyncComponent<C>) -> Self {
        Dyn(value, PhantomData)
    }
}

pub trait ComponentDowncast {
    fn downcast_ref<T>(&self) -> Option<&T>;
    fn downcast_mut<T>(&mut self) -> Option<&mut T>;
}
