use std::{cell::OnceCell, time::Duration};

use async_oneshot::oneshot;
use bevy_asset::{Asset, Assets, Handle};
use bevy_ecs::{bundle::Bundle, entity::Entity, schedule::{NextState, State, States}, system::Command, world::World};
use bevy_time::Time;
use futures_lite::Future;
use crate::{AsyncFailure, AsyncResult, AsyncWorldMut, BoxedQueryCallback, KeepAlive, SystemFuture, CHANNEL_CLOSED};


impl AsyncWorldMut {
    pub fn apply_command(&self, command: impl Command + Sync) -> impl Future<Output = ()> {
        let (sender, receiver) = oneshot::<()>();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                command.apply(world)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED) 
        }
    }

    pub fn spawn_task<T: Send + Sync + 'static>(&self, future: impl Future<Output = T> + Send + 'static) -> impl Future<Output = T> {
        let (mut sender, receiver) = oneshot();
        let alive = KeepAlive::new();
        let alive2 = alive.clone();
        {
            let mut lock = self.executor.spawn_queue.lock();
            lock.push(SystemFuture{
                future: Box::pin(async move { 
                    sender.send(future.await).map_err(|_|AsyncFailure::ChannelClosed)
                }),
                alive,
            });
        }
        async move {
            let result = receiver.await.expect(CHANNEL_CLOSED);
            drop(alive2);
            result
        }
    }

    pub fn spawn_bundle(&self, bundle: impl Bundle) -> impl Future<Output = Entity> {
        let (sender, receiver) = oneshot::<Entity>();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.spawn(bundle).id()
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    pub fn set_state<S: States>(&self, state: S) -> impl Future<Output = AsyncResult<()>> {
        let (sender, receiver) = oneshot();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_resource_mut::<NextState<S>>()
                    .map(|mut s| s.set(state))
                    .ok_or(AsyncFailure::ResourceNotFound)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }


    pub fn get_state<S: States>(&self) -> impl Future<Output = Option<S>> {
        let (sender, receiver) = oneshot();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_resource::<State<S>>().map(|s| s.get().clone())
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    pub fn in_state<S: States>(&self, state: S) -> impl Future<Output = ()> {
        let (sender, receiver) = oneshot::<()>();
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                world.get_resource::<State<S>>()
                    .and_then(|s| (s.get() == &state).then_some(()))
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    pub fn pause<S: States>(&self, duration: Duration) -> impl Future<Output = ()> {
        let (sender, receiver) = oneshot::<()>();
        let time_cell = OnceCell::new();
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                let Some(time) = world.get_resource::<Time>() else {return None};
                let prev = time_cell.get_or_init(||time.elapsed());
                let now = time.elapsed();
                (now - *prev > duration).then_some(())
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

    pub fn asset<A: Asset, T: Send + Sync + 'static>(&self, handle: Handle<A>, f: impl FnOnce(&A) -> T + Send + Sync + 'static) -> impl Future<Output = Option<T>> {
        let (sender, receiver) = oneshot();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_resource::<Assets<A>>()
                    .and_then(|assets| assets.get(handle))
                    .map(f)
            },
            sender
        );
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED)
        }
    }

}
