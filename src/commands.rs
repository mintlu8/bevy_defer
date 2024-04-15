use std::{any::{Any, TypeId}, future::{poll_fn, Future}, task::Poll};
use bevy_app::AppExit;
use bevy_utils::Duration;
use futures::{future::ready, Stream};
use rustc_hash::FxHashMap;
use crate::{access::async_world::AsyncEntityMutFuture, sync::oneshot::{channel, ChannelOut, MaybeChannelOut}, reactors::{StateSignal, REACTORS}, signals::{Signal, SignalId}, tween::AsSeconds};
use bevy_ecs::{bundle::Bundle, entity::Entity, schedule::{NextState, ScheduleLabel, State, States}, system::Resource, world::World};
use bevy_ecs::system::{Command, CommandQueue, IntoSystem, SystemId};
use crate::{access::AsyncWorldMut, AsyncFailure, AsyncResult};
use futures::future::Either;

impl AsyncWorldMut {
    /// Apply a command, does not wait for it to complete.
    /// 
    /// Use [`AsyncWorldMut::run`] to wait and obtain a result.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().apply_command(|w: &mut World| println!("{:?}", w))
    /// # );
    /// ```
    pub fn apply_command(&self, command: impl Command) {
        self.queue.command_queue.borrow_mut().push(command)
    }

    /// Apply a [`CommandQueue`], does not wait for it to complete.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy::ecs::system::CommandQueue;
    /// # bevy_defer::test_spawn!({
    /// let queue = CommandQueue::default();
    /// world().apply_command_queue(queue);
    /// # });
    /// ```
    pub fn apply_command_queue(&self, mut commands: CommandQueue) {
        self.queue.command_queue.borrow_mut().append(&mut commands)
    }

    /// Apply a function on the [`World`] and obtain the result.
    /// 
    /// ## Note
    /// 
    /// Dropping the future will stop the task.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().run(|w: &mut World| w.resource::<Int>().0).await
    /// # );
    /// ```
    pub fn run<T: 'static>(&self, f: impl FnOnce(&mut World) -> T + 'static) -> ChannelOut<T> {
        let (sender, receiver) = channel();
        self.queue.once(f, sender);
        receiver.into_out()
    }

    /// Apply a function on the [`World`], repeat until it returns `Some`.
    /// 
    /// ## Note
    /// 
    /// Dropping the future will stop the task.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().watch(|w: &mut World| w.get_resource::<Int>().map(|r| r.0)).await
    /// # );
    /// ```
    pub fn watch<T: 'static>(&self, f: impl FnMut(&mut World) -> Option<T> + 'static) -> ChannelOut<T> {
        let (sender, receiver) = channel();
        self.queue.repeat(f, sender);
        receiver.into_out()
    }

    /// Runs a schedule a single time.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().run_schedule(Update).await
    /// # );
    /// ```
    pub fn run_schedule(&self, schedule: impl ScheduleLabel) -> ChannelOut<AsyncResult> {
        let (sender, receiver) = channel();
        self.queue.once(move |world: &mut World| {
                world.try_run_schedule(schedule)
                    .map_err(|_| AsyncFailure::ScheduleNotFound)
            }, 
            sender
        );
        receiver.into_out()
    }

    /// Register a system and return a [`SystemId`] so it can later be called by `run_system`.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().register_system(|time: Res<Time>| println!("{}", time.delta_seconds())).await
    /// # );
    /// ```
    pub fn register_system<I: 'static, O: 'static, M, S: IntoSystem<I, O, M> + 'static>(&self, system: S) -> ChannelOut<SystemId<I, O>> {
        let (sender, receiver) = channel();
        self.queue.once(move |world: &mut World| {
                world.register_system(system)
            }, 
            sender
        );
        receiver.into_out()
    }

    /// Run a stored system by their [`SystemId`].
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// let id = world().register_system(|time: Res<Time>| println!("{}", time.delta_seconds())).await;
    /// world().run_system(id).await.unwrap();
    /// # });
    /// ```
    pub fn run_system<O: 'static>(&self, system: SystemId<(), O>) -> ChannelOut<AsyncResult<O>> {
        self.run_system_with_input(system, ())
    }

    /// Run a stored system by their [`SystemId`] with input.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// let id = world().register_system(|input: In<f32>, time: Res<Time>| time.delta_seconds() + *input).await;
    /// world().run_system_with_input(id, 4.0).await.unwrap();
    /// # });
    /// ```
    pub fn run_system_with_input<I: 'static, O: 'static>(&self, system: SystemId<I, O>, input: I) -> ChannelOut<AsyncResult<O>> {
        let (sender, receiver) = channel();
        self.queue.once(move |world: &mut World| {
                world.run_system_with_input(system, input)
                    .map_err(|_| AsyncFailure::SystemIdNotFound)
            }, 
            sender
        );
        receiver.into_out()
    }

    /// Run a system that will be stored and reused upon repeated usage.
    /// 
    /// # Note
    /// 
    /// The system is disambiguated by [`IntoSystem::system_type_id`].
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().run_cached_system(|time: Res<Time>| println!("{}", time.delta_seconds())).await.unwrap();
    /// # });
    /// ```
    pub fn run_cached_system<O: 'static, M, S: IntoSystem<(), O, M> + 'static>(&self, system: S) -> ChannelOut<AsyncResult<O>> {
        self.run_cached_system_with_input(system, ())
    }

    /// Run a system with input that will be stored and reused upon repeated usage.
    /// 
    /// # Note
    /// 
    /// The system is disambiguated by [`IntoSystem::system_type_id`].
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().run_cached_system_with_input(|input: In<f32>, time: Res<Time>| time.delta_seconds() + *input, 4.0).await.unwrap();
    /// # });
    /// ```
    pub fn run_cached_system_with_input<I: 'static, O: 'static, M, S: IntoSystem<I, O, M> + 'static>(&self, system: S, input: I) -> ChannelOut<AsyncResult<O>> {
        #[derive(Debug, Resource, Default)]
        struct SystemCache(FxHashMap<TypeId, Box<dyn Any + Send + Sync>>);

        let (sender, receiver) = channel();
        self.queue.once(move |world: &mut World| {
                let res = world.get_resource_or_insert_with::<SystemCache>(Default::default);
                let type_id = IntoSystem::system_type_id(&system);
                if let Some(id) = res.0.get(&type_id).and_then(|x| x.downcast_ref::<SystemId<I, O>>()).copied() {
                    world.run_system_with_input(id, input)
                        .map_err(|_| AsyncFailure::SystemIdNotFound)
                } else {
                    let id = world.register_system(system);
                    world.resource_mut::<SystemCache>().0.insert(type_id, Box::new(id));
                    world.run_system_with_input(id, input)
                        .map_err(|_| AsyncFailure::SystemIdNotFound)
                }
            },
            sender
        );
        receiver.into_out()
    }

    /// Spawns a new [`Entity`] with no components.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().spawn_empty().await
    /// # );
    /// ```
    pub fn spawn_empty(&self) -> AsyncEntityMutFuture {
        let (sender, receiver) = channel::<Entity>();
        self.queue.once(
            move |world: &mut World| {
                world.spawn_empty().id()
            },
            sender
        );
        receiver.into_out().into_entity_mut_future(self.queue.clone())
    }

    /// Spawn a new [`Entity`] with a given Bundle of components.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().spawn_bundle(SpriteBundle::default()).await
    /// # );
    /// ```
    pub fn spawn_bundle(&self, bundle: impl Bundle) -> AsyncEntityMutFuture {
        let (sender, receiver) = channel::<Entity>();
        self.queue.once(
            move |world: &mut World| {
                world.spawn(bundle).id()
            },
            sender
        );        
        receiver.into_out().into_entity_mut_future(self.queue.clone())
    }

    /// Transition to a new [`States`].
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().set_state(MyState::A).await
    /// # });
    /// ```
    pub fn set_state<S: States>(&self, state: S) -> ChannelOut<AsyncResult<()>> {
        let (sender, receiver) = channel();
        self.queue.once(
            move |world: &mut World| {
                world.get_resource_mut::<NextState<S>>()
                    .map(|mut s| s.set(state))
                    .ok_or(AsyncFailure::ResourceNotFound)
            },
            sender
        );
        receiver.into_out()
    }

    /// Obtain a [`States`].
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().get_state::<MyState>().await
    /// # });
    /// ```
    pub fn get_state<S: States>(&self) -> MaybeChannelOut<AsyncResult<S>> {
        let f = move |world: &World| {
            world.get_resource::<State<S>>().map(|s| s.get().clone())
                    .ok_or(AsyncFailure::ResourceNotFound)
        };
        let f = match self.with_world_ref(f) {
            Ok(result) => return Either::Right(ready(result)),
            Err(f) => f,
        };
        let (sender, receiver) = channel();
        self.queue.once(move |w|f(w), sender);
        Either::Left(receiver.into_out())
    }

    /// Wait until a [`States`] is entered.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().in_state(MyState::A).await
    /// # });
    /// ```
    #[deprecated = "Use `state_stream` instead."]
    pub fn in_state<S: States>(&self, state: S) -> ChannelOut<()> {
        let (sender, receiver) = channel::<()>();
        self.queue.repeat(
            move |world: &mut World| {
                world.get_resource::<State<S>>()
                    .and_then(|s| (s.get() == &state).then_some(()))
            },
            sender
        );
        receiver.into_out()
    }

    /// Obtain a [`Stream`] that reacts to changes of a [`States`].
    /// 
    /// Requires system [`react_to_state`](crate::systems::react_to_state).
    pub fn state_stream<S: States + Clone + Default>(&self) -> impl Stream<Item = S> + 'static {
        let signal = self.typed_signal::<StateSignal<S>>();
        signal.rewind();
        signal
    }

    /// Pause the future for the duration, according to the `Time` resource.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().sleep(5.4).await
    /// # });
    /// ```
    pub fn sleep(&self, duration: impl AsSeconds) -> MaybeChannelOut<()> {
        let duration = duration.as_duration();
        if duration <= Duration::ZERO {
            return Either::Right(ready(()));
        }
        let (sender, receiver) = channel();
        self.queue.timed(duration.as_duration(), sender);
        Either::Left(receiver.into_out())
    }

    /// Pause the future for some frames, according to the `FrameCount` resource.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().sleep_frames(12).await
    /// # });
    /// ```
    pub fn sleep_frames(&self, frames: u32) -> MaybeChannelOut<()> {
        let (sender, receiver) = channel();
        if frames == 0{
            return Either::Right(ready(()));
        }
        self.queue.timed_frames(frames, sender);
        Either::Left(receiver.into_out())
    }

    /// Yield control back to the `bevy_defer` executor.
    /// 
    /// Unlike `yield_now` from `futures_lite`,
    /// the future will be resumed on the next execution point.
    pub fn yield_now(&self) -> impl Future<Output = ()> + 'static {
        let mut yielded = false;
        let queue = self.queue.clone();
        poll_fn(move |cx| {
            if yielded { return Poll::Ready(()); }
            yielded = true;
            queue.yielded.push_cx(cx);
            Poll::Pending
        })
    }

    /// Shutdown the bevy app.
    /// 
    /// # Example
    /// 
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().quit().await
    /// # });
    /// ```
    pub fn quit(&self) -> ChannelOut<()> {
        let (sender, receiver) = channel();
        self.queue.once(
            move |world: &mut World| {
                world.send_event(AppExit);
            },
            sender
        );
        receiver.into_out()
    }

    /// Obtain or init a signal by [`SignalId`].
    /// 
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// let signal = world().typed_signal::<MySignal>();
    /// signal.send(3.14);
    /// signal.poll().await;
    /// # });
    /// ```
    pub fn typed_signal<T: SignalId>(&self) -> Signal<T::Data> {
        if !REACTORS.is_set() {
            panic!("Can only obtain typed signal in async context.")
        }
        REACTORS.with(|signals| signals.get_typed::<T>())
    }

    /// Obtain or init a signal by name and [`SignalId`].
    /// 
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    /// 
    /// # Example
    /// 
    /// ```
    /// # use bevy_defer::signal_ids;
    /// # signal_ids!(MySignal: f32);
    /// # bevy_defer::test_spawn!({
    /// let signal = world().named_signal::<MySignal>("signal 1");
    /// signal.send(3.14);
    /// signal.poll().await;
    /// # });
    /// ```
    pub fn named_signal<T: SignalId>(&self, name: &str) -> Signal<T::Data> {
        if !REACTORS.is_set() {
            panic!("Can only obtain named signal in async context.")
        }
        REACTORS.with(|signals| signals.get_named::<T>(name))
    }
}