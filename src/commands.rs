use std::{cell::OnceCell, time::Duration, future::Future};
use bevy_app::AppExit;
use futures::channel::oneshot::channel;
use bevy_asset::{Asset, AssetPath, AssetServer, Assets, Handle};
use bevy_ecs::{bundle::Bundle, entity::Entity, schedule::{NextState, ScheduleLabel, State, States}, system::Command, world::World};
use bevy_time::Time;
use crate::{AsyncFailure, AsyncResult, AsyncWorldMut, BoxedQueryCallback, CHANNEL_CLOSED};

impl AsyncWorldMut {
    /// Applies a command, causing it to mutate the world.
    pub fn apply_command(&self, command: impl Command) -> impl Future<Output = ()> {
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

    /// Apply a function on the world and obtain the result.
    pub fn run<T: Send + 'static>(&self, f: impl FnOnce(&mut World) -> T + Send + 'static) -> impl Future<Output = T> {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::once(f, sender);
        {
            let mut lock = self.executor.queries.lock();
            lock.push(query);
        }
        async {
            receiver.await.expect(CHANNEL_CLOSED) 
        }
    }

    /// Runs the schedule a single time.
    pub fn run_schedule(&self, schedule: impl ScheduleLabel) -> impl Future<Output = AsyncResult> {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::once( 
            move |world: &mut World| {
                world.try_run_schedule(schedule)
                    .map_err(|_| AsyncFailure::ScheduleNotFound)
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

    /// Spawns a new [`Entity`] with no components.
    pub fn spawn_empty(&self) -> impl Future<Output = Entity> {
        let (sender, receiver) = channel::<Entity>();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.spawn_empty().id()
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

    /// Spawn a new Entity with a given Bundle of components.
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

    /// Transition to a new [`States`].
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

    /// Obtain a [`States`].
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

    /// Wait until a [`States`] is entered.
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

    /// Pause the future for the [`Duration`], according to the [`Time`] resource.
    pub fn sleep(&self, duration: Duration) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let time_cell = OnceCell::new();
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                let time = world.get_resource::<Time>()?;
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

    /// Shutdown the bevy app.
    pub fn quit(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.send_event(AppExit);
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

    /// Run a function on an `Asset` and obtain the result.
    /// 
    /// Repeat until the asset is loaded.
    pub fn asset<A: Asset, T: Send + 'static>(
        &self, 
        handle: Handle<A>,
        mut f: impl FnMut(&A) -> T + Send + 'static
    ) -> impl Future<Output = AsyncResult<T>> {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                let Some(assets) = world.get_resource::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.get(&handle).map(|x|Ok(f(x)))
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

    /// Wait until an asset is loaded.
    /// 
    /// Repeat until the asset is loaded.
    pub fn asset_loaded<A: Asset, T: Send + 'static>(
        &self, 
        handle: Handle<A>,
    ) -> impl Future<Output = AsyncResult<()>> {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::repeat(
            move |world: &mut World| {
                let Some(assets) = world.get_resource::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.contains(&handle).then_some(Ok(()))
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

    /// Run a function on an `Asset` and obtain the result.
    pub fn load_asset<A: Asset>(
        &self, 
        path: impl Into<AssetPath<'static>> + Send + 'static, 
    ) -> impl Future<Output = AsyncResult<Handle<A>>> {
        let (sender, receiver) = channel();
        let query = BoxedQueryCallback::once(
            move |world: &mut World| {
                world.get_resource::<AssetServer>()
                    .ok_or(AsyncFailure::ResourceNotFound)
                    .map(|x| x.load(path))
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

    /// Load asset from a [`AssetPath`], then run a function on the loaded [`Asset`] to obtain the result.
    pub async fn load_direct<A: Asset, T: Send + 'static>(
        &self, 
        path: impl Into<AssetPath<'static>> + Send + 'static, 
        f: impl FnMut(&A) -> T + Send + 'static,
    ) -> AsyncResult<T> {
        self.asset(self.load_asset(path).await?, f).await
    }
}
