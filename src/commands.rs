use std::{cell::OnceCell, future::Future};
use bevy_core::FrameCount;
use bevy_app::AppExit;
use futures::FutureExt;
use crate::{async_asset::AsyncAsset, channels::channel, executor::COMMAND_QUEUE, locals::ASSET_SERVER, signals::{Signal, SignalId, NAMED_SIGNALS}, tween::AsSeconds};
use bevy_asset::{Asset, AssetPath, Handle};
use bevy_ecs::{bundle::Bundle, entity::Entity, schedule::{NextState, ScheduleLabel, State, States}, system::{Command, CommandQueue}, world::World};
use bevy_time::Time;
use crate::{access::{AsyncWorldMut, AsyncEntityMut}, AsyncFailure, AsyncResult, QueryCallback, CHANNEL_CLOSED};

impl AsyncWorldMut {
    /// Apply a command, does not wait for it to complete.
    /// 
    /// Use [`AsyncWorldMut::run`] to wait for and obtain a result.
    pub fn apply_command(&self, command: impl Command) {
        if !COMMAND_QUEUE.is_set() {
            panic!("Cannot use `apply_command` in non_async context, use `run` instead.")
        }
        COMMAND_QUEUE.with(|q| q.borrow_mut().push(command))
    }

    /// Apply a [`CommandQueue`], does not wait for it to complete.
    pub fn apply_command_queue(&self, mut commands: CommandQueue) {
        if !COMMAND_QUEUE.is_set() {
            panic!("Cannot use `apply_command_queue` in non_async context, use `run` instead.")
        }
        COMMAND_QUEUE.with(|q| q.borrow_mut().append(&mut commands))
    }

    /// Apply a function on the [`World`] and obtain the result.
    /// 
    /// ## Note
    /// 
    /// Dropping the future will stop the task.
    pub fn run<T: Send + 'static>(&self, f: impl FnOnce(&mut World) -> T + 'static) -> impl Future<Output = T> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(f, sender);
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Apply a function on the [`World`], repeat until it returns `Some`.
    /// 
    /// ## Note
    /// 
    /// Dropping the future will stop the task.
    pub fn watch<T: Send + 'static>(&self, f: impl FnMut(&mut World) -> Option<T> + 'static) -> impl Future<Output = T> {
        let (sender, receiver) = channel();
        let query = QueryCallback::repeat(f, sender);
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Runs the schedule a single time.
    pub fn run_schedule(&self, schedule: impl ScheduleLabel) -> impl Future<Output = AsyncResult> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once( 
            move |world: &mut World| {
                world.try_run_schedule(schedule)
                    .map_err(|_| AsyncFailure::ScheduleNotFound)
            }, 
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Spawns a new [`Entity`] with no components.
    pub fn spawn_empty(&self) -> impl Future<Output = Entity> {
        let (sender, receiver) = channel::<Entity>();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.spawn_empty().id()
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Spawn a new Entity with a given Bundle of components.
    pub fn spawn_bundle(&self, bundle: impl Bundle) -> impl Future<Output = AsyncEntityMut> {
        let (sender, receiver) = channel::<Entity>();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.spawn(bundle).id()
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        let queue = self.queue.clone();
        async {
            AsyncEntityMut {
                entity: receiver.await.expect(CHANNEL_CLOSED),
                executor: queue
            }   
        }
    }

    /// Transition to a new [`States`].
    pub fn set_state<S: States>(&self, state: S) -> impl Future<Output = AsyncResult<()>> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.get_resource_mut::<NextState<S>>()
                    .map(|mut s| s.set(state))
                    .ok_or(AsyncFailure::ResourceNotFound)
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Obtain a [`States`].
    pub fn get_state<S: States>(&self) -> impl Future<Output = Option<S>> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.get_resource::<State<S>>().map(|s| s.get().clone())
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Wait until a [`States`] is entered.
    pub fn in_state<S: States>(&self, state: S) -> impl Future<Output = ()> {
        let (sender, receiver) = channel::<()>();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                world.get_resource::<State<S>>()
                    .and_then(|s| (s.get() == &state).then_some(()))
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Pause the future for the duration, according to the [`Time`] resource.
    pub fn sleep(&self, duration: impl AsSeconds) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let time_cell = OnceCell::new();
        let duration = duration.as_duration();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let time = world.get_resource::<Time>()?;
                let prev = time_cell.get_or_init(||time.elapsed());
                let now = time.elapsed();
                (now - *prev > duration).then_some(())
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Pause the future for some frames, according to the [`FrameCount`] resource.
    pub fn sleep_frames(&self, frames: u32) -> impl Future<Output = ()> {
        fn diff(a: u32, b: u32) -> u32{
            if a >= b {
                a - b
            } else {
                u32::MAX - b + a
            }
        }
        let (sender, receiver) = channel();
        let time_cell = OnceCell::new();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let frame = world.get_resource::<FrameCount>()?;
                let prev = time_cell.get_or_init(||frame.0);
                let now = frame.0;
                (diff(now, *prev) >= frames).then_some(())
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Shutdown the bevy app.
    pub fn quit(&self) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(
            move |world: &mut World| {
                world.send_event(AppExit);
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Obtain an [`AsyncAsset`] from a [`Handle`].
    pub fn asset<A: Asset>(
        &self, 
        handle: Handle<A>, 
    ) -> AsyncAsset<A> {
        AsyncAsset {
            queue: self.queue.clone(),
            handle,
        }
    }

    /// Load an asset from an [`AssetPath`], equivalent to `AssetServer::load`.
    /// Does not wait for `Asset` to be loaded.
    /// 
    /// # Panics
    /// 
    /// If `AssetServer` does not exist in the world.
    pub fn load_asset<A: Asset>(
        &self, 
        path: impl Into<AssetPath<'static>> + Send + 'static, 
    ) -> AsyncAsset<A> {
        AsyncAsset {
            queue: self.queue.clone(),
            handle: ASSET_SERVER.with(|s| s.load::<A>(path)),
        }
    }

    /// Obtain or init a signal by name and [`SignalId`].
    pub fn signal<T: SignalId>(&self, name: &str) -> Signal<T::Data> {
        if !NAMED_SIGNALS.is_set() {
            panic!("Can only obtain named signal in async context.")
        }
        NAMED_SIGNALS.with(|signals| signals.get_from_ref::<T>(name)).into()
    }
}
