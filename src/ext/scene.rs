use std::ops::Deref;

use bevy_ecs::component::Component;
use bevy_ecs::query::With;
use bevy_ecs::system::{Commands, Query};
use bevy_scene::SceneInstance;
use bevy_core::Name;
use bevy_ecs::{bundle::Bundle, entity::Entity, query::QueryState, world::World};
use bevy_hierarchy::Children;
use futures::{Future, FutureExt};
use ref_cast::RefCast;
use crate::channels::channel;
use crate::{AsyncFailure, AsyncResult};
use crate::{access::{AsyncEntityMut, AsyncWorldMut}, access::async_query::ResQueryCache, CHANNEL_CLOSED};

#[derive(Debug, Component)]
pub struct SceneSignal(async_oneshot::Sender<()>);

/// Send [`SceneSignal`] once scene is loaded.
pub fn react_to_scene_load(
    mut commands: Commands,
    mut query: Query<(Entity, &mut SceneSignal), With<SceneInstance>>
) {
    for (entity, mut signal) in query.iter_mut() {
        let _ = signal.0.send(());
        commands.entity(entity).remove::<SceneSignal>();
    }
}

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
    /// Requires [`react_to_scene_load`] to function.
    pub async fn spawn_scene(&self, bun: impl Bundle) -> AsyncEntityMut{
        let (send, recv) = async_oneshot::oneshot();
        let entity = self.spawn_bundle((bun, SceneSignal(send))).await.id();
        let _ = recv.await;
        AsyncEntityMut { entity, queue: self.queue.clone() }
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
    pub fn spawned(&self, name: impl Into<String>) -> impl Future<Output = AsyncResult<AsyncEntityMut>>{
        let (sender, receiver) = channel();
        let name = name.into();
        let entity = self.entity;
        self.queue.repeat(
            move |world: &mut World| {
                if !world.entity(entity).contains::<SceneInstance>() {
                    return None;
                }
                match world.remove_resource::<ResQueryCache<Q, ()>>() {
                    Some(mut state) => {
                        let result = find_name(world, entity, &name, &mut state.0);
                        world.insert_resource(state);
                        Some(result.ok_or(AsyncFailure::NameNotFound))
                    }
                    None => {
                        let mut state = ResQueryCache(world.query::<Q>());
                        let result = find_name(world, entity, &name, &mut state.0);
                        world.insert_resource(state);
                        Some(result.ok_or(AsyncFailure::NameNotFound))
                    }
                }
            },
            sender,
        );
        let queue = self.queue.clone();
        receiver.map(|entity| Ok(AsyncEntityMut{
            entity: entity.expect(CHANNEL_CLOSED)?,
            queue,
        }))
    }
}

