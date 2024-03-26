use std::{ops::Deref, pin::pin};

use bevy_core::Name;
use bevy_ecs::{bundle::Bundle, entity::Entity, query::QueryState, world::World};
use bevy_hierarchy::Children;
use futures::Future;
use ref_cast::RefCast;
use crate::channels::channel;
use crate::{access::{AsyncEntityMut, AsyncWorldMut}, QueryCallback, async_query::ResQueryCache, CHANNEL_CLOSED};

#[derive(RefCast)]
#[repr(transparent)]
pub struct AsyncScene(AsyncEntityMut);

impl AsyncWorldMut {
    /// Spawn a bundle but returns an `AsyncScene` with scene specific utilities.
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
    /// this method cannot fail. `try_get_spawned` can be used to pass in a future for cancellation.
    pub fn spawned(&self, name: impl Into<String>) -> impl Future<Output = AsyncEntityMut>{
        let (sender, receiver) = channel();
        let name = name.into();
        let entity = self.entity;
        let query = QueryCallback::repeat(
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
        {
            let mut lock = self.executor.queries.borrow_mut();
            lock.push(query);
        }
        let executor = self.executor.clone();
        async {
            AsyncEntityMut{
                entity: receiver.await.expect(CHANNEL_CLOSED),
                executor,
            }
        }
    }

    /// Obtain a spawned entity by [`Name`].
    /// 
    /// Use something like [`AsyncWorldMut::sleep`] for cancellation.
    pub fn try_get_spawned(&self, name: impl Into<String>, cancel_when: impl Future) -> impl Future<Output = Option<AsyncEntityMut>>{
        use futures::FutureExt;
        let f1 = self.spawned(name).fuse();
        async move {futures::select_biased! {
            e = pin!(f1) => Some(e),
            _ = cancel_when.fuse() => None
        }}
    }
}

