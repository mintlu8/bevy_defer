use std::{
    cmp::Reverse,
    fmt::{Display, Formatter},
};

use bevy::{
    core::Name,
    ecs::{
        component::Component,
        entity::Entity,
        query::{QueryData, QueryFilter},
        system::Resource,
    },
};
use ref_cast::RefCast;

use crate::{executor::WORLD, AsyncAccess, AsyncWorld};

/// Provides a [`Display`] implementation for [`Entity`] inside a `bevy_defer` scope.
#[derive(Debug, Clone, Copy, RefCast)]
#[repr(transparent)]
pub struct InspectEntity(pub Entity);

impl Default for InspectEntity {
    fn default() -> Self {
        Self(Entity::PLACEHOLDER)
    }
}

fn simple(entity: Entity, f: &mut Formatter<'_>) -> std::fmt::Result {
    if entity == Entity::PLACEHOLDER {
        write!(f, "Entity(placeholder)")
    } else {
        write!(f, "Entity({},{})", entity.index(), entity.generation())
    }
}

impl Display for InspectEntity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let entity = self.0;
        if !WORLD.is_set() {
            return simple(entity, f);
        }
        if AsyncWorld.resource_scope::<EntityInspectors, _>(|inspectors| {
            for (_, fmt) in &inspectors.0 {
                if fmt(entity, f) {
                    return true;
                }
            }
            false
        }) {
            return Ok(());
        }
        simple(entity, f)
    }
}

type InspectorFn = Box<dyn Fn(Entity, &mut Formatter) -> bool + Send + Sync>;

#[derive(Resource)]
pub struct EntityInspectors(Vec<(i32, InspectorFn)>);

impl EntityInspectors {
    fn get_insert_pos(&self, priority: i32) -> usize {
        match self
            .0
            .binary_search_by_key(&Reverse(priority), |x| Reverse(x.0))
        {
            Ok(x) => x,
            Err(x) => x,
        }
    }
    pub fn push<C: Component>(
        &mut self,
        priority: i32,
        f: impl Fn(Entity, &C, &mut Formatter) + Send + Sync + 'static,
    ) {
        let idx = self.get_insert_pos(priority);
        self.0.insert(
            idx,
            (
                priority,
                Box::new(move |entity, fmt| {
                    AsyncWorld
                        .entity(entity)
                        .component::<C>()
                        .get_mut(|x| f(entity, x, fmt))
                        .is_ok()
                }),
            ),
        );
    }

    pub fn push_query<Q: QueryData + 'static, F: QueryFilter + 'static>(
        &mut self,
        priority: i32,
        f: impl Fn(Q::Item<'_>, &mut Formatter) + Send + Sync + 'static,
    ) {
        let idx = self.get_insert_pos(priority);
        self.0.insert(
            idx,
            (
                priority,
                Box::new(move |entity, fmt| {
                    AsyncWorld
                        .entity(entity)
                        .query_filtered::<Q, F>()
                        .get_mut(|x| f(x, fmt))
                        .is_ok()
                }),
            ),
        );
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl Default for EntityInspectors {
    fn default() -> Self {
        Self(vec![(
            0,
            Box::new(move |entity, fmt| {
                AsyncWorld
                    .entity(entity)
                    .component::<Name>()
                    .get_mut(|x| {
                        write!(
                            fmt,
                            "Entity({},{},{})",
                            x.as_str(),
                            entity.index(),
                            entity.generation()
                        )
                    })
                    .is_ok()
            }),
        )])
    }
}
