use std::{cell::OnceCell, time::Duration, future::Future};
use triomphe::Arc;
use futures::channel::oneshot::channel;
use bevy_asset::{Asset, Assets, Handle};
use bevy_ecs::{bundle::Bundle, entity::Entity, schedule::{NextState, State, States}, system::Command, world::World};
use bevy_time::Time;
use rustc_hash::FxHashMap;
use crate::{signal_inner::SignalData, AsyncFailure, AsyncResult, AsyncWorldMut, BoxedQueryCallback, Object, CHANNEL_CLOSED};

pub struct SignalServer {
    signals: FxHashMap<String, Arc<SignalData<Object>>>,
}

impl AsyncWorldMut {
    pub fn apply_command(&self, command: impl Command + Sync) -> impl Future<Output = ()> {
        let (sender, receiver) = channel::<()>();
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

    pub fn spawn_bundle(&self, bundle: impl Bundle) -> impl Future<Output = Entity> {
        let (sender, receiver) = channel::<Entity>();
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
        let (sender, receiver) = channel();
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
        let (sender, receiver) = channel();
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
        let (sender, receiver) = channel::<()>();
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
        let (sender, receiver) = channel::<()>();
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
        let (sender, receiver) = channel();
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
