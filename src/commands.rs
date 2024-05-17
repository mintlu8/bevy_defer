use crate::executor::{with_world_mut, with_world_ref, QUERY_QUEUE, REACTORS, SPAWNER};
use crate::signals::SignalStream;
use crate::{
    access::AsyncEntityMut,
    reactors::StateSignal,
    signals::{Signal, SignalId},
    sync::oneshot::{channel, ChannelOut, MaybeChannelOut},
    tween::AsSeconds,
};
use crate::{access::AsyncWorld, AccessError, AccessResult};
use bevy_app::AppExit;
use bevy_ecs::system::{Command, CommandQueue, IntoSystem, SystemId};
use bevy_ecs::{
    bundle::Bundle,
    schedule::{NextState, ScheduleLabel, State, States},
    system::Resource,
    world::World,
};
use bevy_log::error;
use bevy_utils::Duration;
use futures::future::ready;
use futures::future::Either;
use rustc_hash::FxHashMap;
use std::fmt::Display;
use std::{
    any::{Any, TypeId},
    future::{poll_fn, Future},
    task::Poll,
};

#[allow(unused)]
use bevy_ecs::entity::Entity;

impl AsyncWorld {
    /// Apply a command, does not wait for it to complete.
    ///
    /// Use [`AsyncWorld::run`] to wait and obtain a result.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().apply_command(|w: &mut World| println!("{:?}", w))
    /// # );
    /// ```
    pub fn apply_command(&self, command: impl Command) {
        with_world_mut(|w| command.apply(w))
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
        with_world_mut(|w| commands.apply(w))
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
    /// world().run(|w: &mut World| w.resource::<Int>().0)
    /// # );
    /// ```
    pub fn run<T>(&self, f: impl FnOnce(&mut World) -> T) -> T {
        with_world_mut(f)
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
    /// world().watch(|w: &mut World| w.get_resource::<Int>().map(|r| r.0))
    /// # );
    /// ```
    pub fn watch<T: 'static>(
        &self,
        f: impl FnMut(&mut World) -> Option<T> + 'static,
    ) -> ChannelOut<T> {
        let (sender, receiver) = channel();
        QUERY_QUEUE.with(|queue| queue.repeat(f, sender));
        receiver.into_out()
    }

    pub(crate) fn watch_left<T: 'static, R: Future>(
        &self,
        f: impl FnMut(&mut World) -> Option<T> + 'static,
    ) -> Either<ChannelOut<T>, R> {
        let (sender, receiver) = channel();
        QUERY_QUEUE.with(|queue| queue.repeat(f, sender));
        Either::Left(receiver.into_out())
    }

    /// Runs a schedule a single time.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().run_schedule(Update)
    /// # );
    /// ```
    pub fn run_schedule(&self, schedule: impl ScheduleLabel) -> AccessResult {
        with_world_mut(move |world: &mut World| {
            world
                .try_run_schedule(schedule)
                .map_err(|_| AccessError::ScheduleNotFound)
        })
    }

    /// Register a system and return a [`SystemId`] so it can later be called by `run_system`.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().register_system(|time: Res<Time>| println!("{}", time.delta_seconds()))
    /// # );
    /// ```
    pub fn register_system<I: 'static, O: 'static, M, S: IntoSystem<I, O, M> + 'static>(
        &self,
        system: S,
    ) -> SystemId<I, O> {
        with_world_mut(move |world: &mut World| world.register_system(system))
    }

    /// Run a stored system by their [`SystemId`].
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// let id = world().register_system(|time: Res<Time>| println!("{}", time.delta_seconds()));
    /// world().run_system(id).unwrap();
    /// # });
    /// ```
    pub fn run_system<O: 'static>(&self, system: SystemId<(), O>) -> AccessResult<O> {
        self.run_system_with_input(system, ())
    }

    /// Run a stored system by their [`SystemId`] with input.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// let id = world().register_system(|input: In<f32>, time: Res<Time>| time.delta_seconds() + *input);
    /// world().run_system_with_input(id, 4.0).unwrap();
    /// # });
    /// ```
    pub fn run_system_with_input<I: 'static, O: 'static>(
        &self,
        system: SystemId<I, O>,
        input: I,
    ) -> AccessResult<O> {
        with_world_mut(move |world: &mut World| {
            world
                .run_system_with_input(system, input)
                .map_err(|_| AccessError::SystemIdNotFound)
        })
    }

    /// Run a system that will be stored and reused upon repeated usage.
    ///
    /// # Note
    ///
    /// The system is disambiguated by the type ID of the closure.
    /// Be careful not to pass in a `fn`.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().run_cached_system(|time: Res<Time>| println!("{}", time.delta_seconds())).unwrap();
    /// # });
    /// ```
    pub fn run_cached_system<O: 'static, M, S: IntoSystem<(), O, M> + 'static>(
        &self,
        system: S,
    ) -> AccessResult<O> {
        self.run_cached_system_with_input(system, ())
    }

    /// Run a system with input that will be stored and reused upon repeated usage.
    ///
    /// # Note
    ///
    /// The system is disambiguated by the type ID of the closure.
    /// Be careful not to pass in a `fn`.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().run_cached_system_with_input(|input: In<f32>, time: Res<Time>| time.delta_seconds() + *input, 4.0).unwrap();
    /// # });
    /// ```
    pub fn run_cached_system_with_input<
        I: 'static,
        O: 'static,
        M,
        S: IntoSystem<I, O, M> + 'static,
    >(
        &self,
        system: S,
        input: I,
    ) -> AccessResult<O> {
        #[derive(Debug, Resource, Default)]
        struct SystemCache(FxHashMap<TypeId, Box<dyn Any + Send + Sync>>);

        with_world_mut(move |world: &mut World| {
            let res = world.get_resource_or_insert_with::<SystemCache>(Default::default);
            let type_id = TypeId::of::<S>();
            if let Some(id) = res
                .0
                .get(&type_id)
                .and_then(|x| x.downcast_ref::<SystemId<I, O>>())
                .copied()
            {
                world
                    .run_system_with_input(id, input)
                    .map_err(|_| AccessError::SystemIdNotFound)
            } else {
                let id = world.register_system(system);
                world
                    .resource_mut::<SystemCache>()
                    .0
                    .insert(type_id, Box::new(id));
                world
                    .run_system_with_input(id, input)
                    .map_err(|_| AccessError::SystemIdNotFound)
            }
        })
    }

    /// Spawns a new [`Entity`] with no components.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().spawn_empty()
    /// # );
    /// ```
    pub fn spawn_empty(&self) -> AsyncEntityMut {
        self.entity(with_world_mut(move |world: &mut World| {
            world.spawn_empty().id()
        }))
    }

    /// Spawn a new [`Entity`] with a given Bundle of components.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// world().spawn_bundle(SpriteBundle::default())
    /// # );
    /// ```
    pub fn spawn_bundle(&self, bundle: impl Bundle) -> AsyncEntityMut {
        self.entity(with_world_mut(move |world: &mut World| {
            world.spawn(bundle).id()
        }))
    }

    /// Transition to a new [`States`].
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().set_state(MyState::A)
    /// # });
    /// ```
    pub fn set_state<S: States>(&self, state: S) -> AccessResult<()> {
        with_world_mut(move |world: &mut World| {
            world
                .get_resource_mut::<NextState<S>>()
                .map(|mut s| s.set(state))
                .ok_or(AccessError::ResourceNotFound)
        })
    }

    /// Obtain a [`States`].
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().get_state::<MyState>()
    /// # });
    /// ```
    pub fn get_state<S: States>(&self) -> AccessResult<S> {
        with_world_ref(|world| {
            world
                .get_resource::<State<S>>()
                .map(|s| s.get().clone())
                .ok_or(AccessError::ResourceNotFound)
        })
    }

    /// Wait until a [`States`] is entered.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().in_state(MyState::A)
    /// # });
    /// ```
    #[deprecated = "Use `state_stream` instead."]
    pub fn in_state<S: States>(&self, state: S) -> ChannelOut<()> {
        self.watch(move |world: &mut World| {
            world
                .get_resource::<State<S>>()
                .and_then(|s| (s.get() == &state).then_some(()))
        })
    }

    /// Obtain a [`Stream`] that reacts to changes of a [`States`].
    ///
    /// Requires system [`react_to_state`](crate::systems::react_to_state).
    pub fn state_stream<S: States + Clone + Default>(&self) -> SignalStream<S> {
        let signal = self.typed_signal::<StateSignal<S>>();
        signal.rewind();
        signal.into_stream()
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
        QUERY_QUEUE.with(|queue| queue.timed(duration.as_duration(), sender));
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
        if frames == 0 {
            return Either::Right(ready(()));
        }
        QUERY_QUEUE.with(|queue| queue.timed_frames(frames, sender));
        Either::Left(receiver.into_out())
    }

    /// Yield control back to the `bevy_defer` executor.
    ///
    /// Unlike `yield_now` from `futures_lite`,
    /// the future will be resumed on the next execution point.
    pub fn yield_now(&self) -> impl Future<Output = ()> + 'static {
        let mut yielded = false;
        poll_fn(move |cx| {
            if yielded {
                return Poll::Ready(());
            }
            yielded = true;
            QUERY_QUEUE.with(|queue| queue.yielded.push_cx(cx));
            Poll::Pending
        })
    }

    /// Shutdown the bevy app.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// world().quit()
    /// # });
    /// ```
    pub fn quit(&self) {
        with_world_mut(move |world: &mut World| {
            world.send_event(AppExit);
        })
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

    /// Spawn a `bevy_defer` compatible future with a handle.
    ///
    /// # Handle
    ///
    /// The handle can be used to obtain the result,
    /// if dropped, the associated future will be dropped by the executor.
    ///
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    pub fn spawn_scoped<T: 'static>(fut: impl Future<Output = T> + 'static) -> impl Future<Output = T> {
        if !SPAWNER.is_set() {
            panic!("bevy_defer::spawn_scoped can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| s.spawn(fut))
    }

    /// Spawn a `bevy_defer` compatible future.
    ///
    /// The spawned future will not be dropped until finished.
    ///
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    pub fn spawn<T: 'static>(fut: impl Future<Output = T> + 'static) {
        if !SPAWNER.is_set() {
            panic!("bevy_defer::spawn can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| s.spawn(fut).detach());
    }

    /// Spawn a `bevy_defer` compatible future and logs errors.
    ///
    /// The spawned future will not be dropped until finished.
    ///
    /// # Panics
    ///
    /// If used outside a `bevy_defer` future.
    pub fn spawn_log<T: 'static, E: Display + 'static>(fut: impl Future<Output = Result<T, E>> + 'static) {
        use futures::FutureExt;
        if !SPAWNER.is_set() {
            panic!("bevy_defer::spawn can only be used in a bevy_defer future.")
        }
        SPAWNER.with(|s| s.spawn(fut.map(|r| if let Err(e) = r {
            error!("{e}");
        })).detach());
    }
}
