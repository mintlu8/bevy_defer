use std::ops::Deref;

use bevy_core::Name;
use bevy_ecs::{bundle::Bundle, entity::Entity, query::QueryState, world::World};
use bevy_hierarchy::Children;
use futures::{Future, FutureExt};
use ref_cast::RefCast;
use crate::channels::channel;
use crate::{access::{AsyncEntityMut, AsyncWorldMut}, access::async_query::ResQueryCache, CHANNEL_CLOSED};

/// [`AsyncEntityMut`] with extra scene related methods.
#[derive(RefCast)]
#[repr(transparent)]
pub struct AsyncScene(AsyncEntityMut);

impl From<AsyncEntityMut> for AsyncScene {
    fn from(value: AsyncEntityMut) -> Self {
        Self(value)
    }
}

impl AsyncWorldMut {
    /// Spawn a bundle but returns a specialize version of `AsyncEntityMut`, [`AsyncScene`], with additional methods.
    pub async fn spawn_scene(&self, bun: impl Bundle) -> AsyncScene{
        let entity = self.spawn_bundle(bun).await.id();
        AsyncScene(self.entity(entity))
    }
}

impl Deref for AsyncScene {
    type Target = AsyncEntityMut;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

type Q = (Entity, Option<&'static Name>, Option<&'static Children>);

fn find_name(world: &mut World, parent: Entity, name: &str, query: &mut QueryState<Q>) -> Option<Entity> {
    let (entity, entity_name, children) = query.get(world, parent).ok()?;
    if entity_name.map(|x| x.as_str() == name) == Some(true) {
        return Some(entity);
    }
    if let Some(children) = children {
        let children: Vec<_> = children.iter().cloned().collect();
        children.into_iter().find_map(|e| find_name(world, e, name, query))
    } else {
        None
    }
}


impl AsyncScene {
    /// Obtain a spawned entity by [`Name`].
    /// 
    /// Due to having to wait and not being able to prove a negative,
    /// this method cannot fail. The user is responsible for cancelling this future.
    pub fn spawned(&self, name: impl Into<String>) -> impl Future<Output = AsyncEntityMut>{
        let (sender, receiver) = channel();
        let name = name.into();
        let entity = self.entity;
        self.queue.repeat(
            move |world: &mut World| {
                match world.remove_resource::<ResQueryCache<Q, ()>>() {
                    Some(mut state) => {
                        let result = find_name(world, entity, &name, &mut state.0);
                        world.insert_resource(state);
                        result
                    }
                    None => {
                        let mut state = ResQueryCache(world.query::<Q>());
                        let result = find_name(world, entity, &name, &mut state.0);
                        world.insert_resource(state);
                        result
                    }
                }
            },
            sender,
        );
        let queue = self.queue.clone();
        receiver.map(|entity| AsyncEntityMut{
            entity: entity.expect(CHANNEL_CLOSED),
            queue,
        })
    }
}

