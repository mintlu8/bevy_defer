use crate::{oneshot, BoxedQueryCallback};
use bevy_ecs::{
    entity::Entity,
    query::{QueryData, QueryFilter, QueryState, WorldQuery},
    system::Resource,
    world::World,
};
use std::{
    borrow::{Borrow, Cow},
    marker::PhantomData,
    future::Future,
};
use triomphe::Arc;

use super::{AsyncExecutor, AsyncFailure, AsyncResult, AsyncSystemParam, Signals};

#[derive(Debug, Resource)]
struct ResQueryCache<T: QueryData, F: QueryFilter>(QueryState<T, F>);

pub struct AsyncQuery<'t, T: QueryData + 't, F: QueryFilter + 't = ()> {
    pub(crate) executor: Cow<'t, Arc<AsyncExecutor>>,
    pub(crate) p: PhantomData<(T, F)>,
}

pub struct AsyncEntityQuery<'t, T: QueryData + 't, F: QueryFilter + 't = ()> {
    pub(crate) entity: Entity,
    pub(crate) executor: Cow<'t, Arc<AsyncExecutor>>,
    pub(crate) p: PhantomData<(T, F)>,
}

unsafe impl<T: QueryData, F: QueryFilter> Send for AsyncQuery<'_, T, F> {}
unsafe impl<T: QueryData, F: QueryFilter> Sync for AsyncQuery<'_, T, F> {}

unsafe impl<T: QueryData, F: QueryFilter> Send for AsyncEntityQuery<'_, T, F> {}
unsafe impl<T: QueryData, F: QueryFilter> Sync for AsyncEntityQuery<'_, T, F> {}

impl<T: QueryData, F: QueryFilter> AsyncQuery<'_, T, F> {
    pub fn entity(&self, entity: impl Borrow<Entity>) -> AsyncEntityQuery<T, F> {
        AsyncEntityQuery {
            entity: *entity.borrow(),
            executor: self.executor.clone(),
            p: PhantomData,
        }
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncQuery<'_, T, F> {
    pub fn for_each (
        &self,
        f: impl FnMut(T::Item<'_>) + Send + Sync + 'static,
    ) -> impl Future<Output = AsyncResult<()>> + 'static {
        let (sender, receiver) = oneshot::<()>();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| match world.remove_resource::<ResQueryCache<T, F>>() {
                Some(mut state) => {
                    state.0.iter_mut(world).for_each(f);
                    world.insert_resource(state);
                }
                None => {
                    let mut state = ResQueryCache(world.query_filtered::<T, F>());
                    state.0.iter_mut(world).for_each(f);
                    world.insert_resource(state);
                }
            },
            sender,
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            match receiver.await {
                Ok(()) => Ok(()),
                Err(_) => Err(AsyncFailure::ChannelClosed),
            }
        }
    }
}

impl<T: QueryData, F: QueryFilter> AsyncSystemParam for AsyncEntityQuery<'_, T, F> {
    fn from_async_context(entity: Entity, executor: &Arc<AsyncExecutor>, _: &Signals) -> Self {
        Self {
            entity,
            executor: Cow::Owned(executor.clone()),
            p: PhantomData,
        }
    }
}

impl<T: QueryData + 'static, F: QueryFilter + 'static> AsyncEntityQuery<'_, T, F> {
    pub fn with<Out: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(T::Item<'_>) -> Out + Send + Sync + 'static,
    ) -> impl Future<Output = AsyncResult<Out>> + 'static {
        let (sender, receiver) = oneshot::<Option<Out>>();
        let entity = self.entity;
        let query = BoxedQueryCallback::once(
            move |world: &mut World| match world.remove_resource::<ResQueryCache<T, F>>() {
                Some(mut state) => {
                    let result = f(state.0.get_mut(world, entity).ok()?);
                    world.insert_resource(state);
                    Some(result)
                }
                None => {
                    let mut state = ResQueryCache(world.query_filtered::<T, F>());
                    let result = f(state.0.get_mut(world, entity).ok()?);
                    world.insert_resource(state);
                    Some(result)
                }
            },
            sender,
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            match receiver.await {
                Ok(Some(out)) => Ok(out),
                Ok(None) => Err(AsyncFailure::ComponentNotFound),
                Err(_) => Err(AsyncFailure::ChannelClosed),
            }
        }
    }

    pub fn watch<U: Send + Sync + 'static>(
        &mut self,
        f: impl Fn(<T as WorldQuery>::Item<'_>) -> Option<U> + Send + Sync + 'static,
    ) -> impl Future<Output = U> + 'static {
        let (send, recv) = oneshot::<U>();
        let entity = self.entity;
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| match world.remove_resource::<ResQueryCache<T, F>>() {
                Some(mut state) => {
                    let result = f(state.0.get_mut(world, entity).ok()?);
                    world.insert_resource(state);
                    result
                }
                None => {
                    let mut state = ResQueryCache(world.query_filtered::<T, F>());
                    let result = f(state.0.get_mut(world, entity).ok()?);
                    world.insert_resource(state);
                    result
                }
            },
            send,
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async { recv.await.unwrap() }
    }
}
