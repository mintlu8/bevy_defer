use std::{cell::OnceCell, future::Future, time::Duration};
use bevy_core::FrameCount;
use bevy_app::AppExit;
use futures::FutureExt;
use crate::{channels::channel, locals::ASSET_SERVER};
use bevy_asset::{Asset, AssetId, AssetPath, Assets, Handle};
use bevy_ecs::{bundle::Bundle, entity::Entity, schedule::{NextState, ScheduleLabel, State, States}, system::Command, world::World};
use bevy_time::Time;
use crate::{access::{AsyncWorldMut, AsyncEntityMut}, AsyncFailure, AsyncResult, QueryCallback, CHANNEL_CLOSED};

impl AsyncWorldMut {
    /// Applies a command, causing it to mutate the world.
    pub fn apply_command(&self, command: impl Command) -> impl Future<Output = ()> {
        let (sender, receiver) = channel::<()>();
        let query = QueryCallback::once(
            move |world: &mut World| {
                command.apply(world)
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Apply a function on the world and obtain the result.
    pub fn run<T: Send + 'static>(&self, f: impl FnOnce(&mut World) -> T + 'static) -> impl Future<Output = T> {
        let (sender, receiver) = channel();
        let query = QueryCallback::once(f, sender);
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

    /// Pause the future for the [`Duration`], according to the [`Time`] resource.
    pub fn sleep(&self, duration: Duration) -> impl Future<Output = ()> {
        let (sender, receiver) = channel();
        let time_cell = OnceCell::new();
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

    /// Run a function on an `Asset` and obtain the result,
    /// repeat until the asset is loaded.
    /// 
    /// # Note
    /// 
    /// `Handle<T>` is not allowed as that could drop the asset immediately,
    /// pass in `&Handle<T>` instead.
    pub fn asset<A: Asset, T: Send + 'static>(
        &self, 
        handle: impl Into<AssetId<A>> + Copy,
        mut f: impl FnMut(&A) -> T + Send + 'static
    ) -> impl Future<Output = AsyncResult<T>> {
        let (sender, receiver) = channel();
        let handle = handle.into();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let Some(assets) = world.get_resource::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.get(handle).map(|x|Ok(f(x)))
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
    }

    /// Remove an `Asset` and obtain it and repeat until the asset is loaded.
    /// 
    /// # Note
    /// 
    /// `Handle<T>` is not allowed as that could drop the asset immediately,
    /// pass in `&Handle<T>` instead.
    pub fn take_asset<A: Asset>(
        &self, 
        handle: impl Into<AssetId<A>> + Copy,
    ) -> impl Future<Output = AsyncResult<A>> {
        let (sender, receiver) = channel();
        let handle = handle.into();
        let query = QueryCallback::repeat(
            move |world: &mut World| {
                let Some(mut assets) = world.get_resource_mut::<Assets<A>>()
                    else { return Some(Err(AsyncFailure::ResourceNotFound)) };
                assets.remove(handle).map(Ok)
            },
            sender
        );
        {
            let mut lock = self.queue.queries.borrow_mut();
            lock.push(query);
        }
        receiver.map(|x| x.expect(CHANNEL_CLOSED))
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
    ) -> Handle<A> {
        ASSET_SERVER.with(|s| s.load::<A>(path))
    }

    /// Load an asset from a [`AssetPath`],
    /// then run a function on the loaded [`Asset`] to obtain the result.
    /// 
    /// # Panics
    /// 
    /// If `AssetServer` does not exist in the world.
    pub async fn load_direct<A: Asset, T: Send + 'static>(
        &self, 
        path: impl Into<AssetPath<'static>> + Send + 'static, 
        mut f: impl FnMut(Handle<A>, &A) -> T + Send + 'static,
    ) -> AsyncResult<T> {
        let handle = self.load_asset(path);
        self.asset(&handle.clone_weak(), move |x| f(handle.clone(), x)).await
    }

    /// Load an asset from a [`AssetPath`], then remove the result from [`Asset`] to obtain the result.
    pub async fn load_take<A: Asset>(
        &self, 
        path: impl Into<AssetPath<'static>> + Send + 'static, 
    ) -> AsyncResult<A> {
        let handle = self.load_asset(path);
        self.take_asset(&handle).await
    }
}
