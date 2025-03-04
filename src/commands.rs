use crate::access::AsyncResource;
use crate::channel;
use crate::executor::{with_world_mut, with_world_ref, QUERY_QUEUE, REACTORS, WORLD};
use crate::sync::oneshot::{ChannelOut, MaybeChannelOut};
use crate::{access::AsyncEntityMut, reactors::StateSignal, signals::SignalId, tween::AsSeconds};
use crate::{access::AsyncWorld, AccessError, AccessResult};
use async_shared::Value;
use bevy::app::AppExit;
use bevy::ecs::event::{Event, EventId};
use bevy::ecs::system::{IntoSystem, SystemId};
use bevy::ecs::world::{Command, CommandQueue, FromWorld, Mut};
use bevy::ecs::{bundle::Bundle, schedule::ScheduleLabel, system::Resource, world::World};
use bevy::prelude::SystemInput;
use bevy::state::state::{FreelyMutableState, NextState, State, States};
use bevy::tasks::AsyncComputeTaskPool;
use futures::future::ready;
use futures::future::Either;
use futures::stream::FusedStream;
use std::any::type_name;
use std::time::Duration;
use std::{
    future::{poll_fn, Future},
    task::Poll,
};

#[allow(unused)]
use bevy::ecs::entity::Entity;

impl AsyncWorld {
    /// Apply a command.
    ///
    /// Use [`AsyncWorld::run`] to obtain a result instead.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// AsyncWorld.apply_command(|w: &mut World| println!("{:?}", w))
    /// # );
    /// ```
    pub fn apply_command(&self, command: impl Command) {
        with_world_mut(|w| command.apply(w))
    }

    /// Apply a [`CommandQueue`].
    ///
    /// # Example
    ///
    /// ```
    /// # use bevy::ecs::world::CommandQueue;
    /// # bevy_defer::test_spawn!({
    /// let queue = CommandQueue::default();
    /// AsyncWorld.apply_command_queue(queue);
    /// # });
    /// ```
    pub fn apply_command_queue(&self, mut commands: CommandQueue) {
        with_world_mut(|w| commands.apply(w))
    }

    /// Apply a function on the [`World`] and obtain the result.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// AsyncWorld.run(|w: &mut World| w.resource::<Int>().0)
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
    /// AsyncWorld.watch(|w: &mut World| w.get_resource::<Int>().map(|r| r.0))
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

    /// Watch as [`Either::Left`].
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
    /// AsyncWorld.run_schedule(Update)
    /// # );
    /// ```
    pub fn run_schedule(&self, schedule: impl ScheduleLabel) -> AccessResult {
        with_world_mut(move |world: &mut World| {
            world
                .try_run_schedule(schedule)
                .map_err(|_| AccessError::ScheduleNotFound)
        })
    }

    /// Obtain a [`Resource`] in a scoped function.
    /// [`AsyncWorld`] can be used inside as this is not considered an access function.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// AsyncWorld.run(|w: &mut World| w.resource::<Int>().0)
    /// # );
    /// ```
    pub fn resource_scope<R: Resource, T>(&self, f: impl FnOnce(Mut<R>) -> T) -> T {
        with_world_mut(|w| w.resource_scope(|w, r| WORLD.set(w, || f(r))))
    }

    /// Register a system and return a [`SystemId`] so it can later be called by `run_system`.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// AsyncWorld.register_system(|time: Res<Time>| println!("{}", time.delta_secs()))
    /// # );
    /// ```
    pub fn register_system<
        I: SystemInput + 'static,
        O: 'static,
        M,
        S: IntoSystem<I, O, M> + 'static,
    >(
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
    /// let id = AsyncWorld.register_system(|time: Res<Time>| println!("{}", time.delta_secs()));
    /// AsyncWorld.run_system(id).unwrap();
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
    /// let id = AsyncWorld.register_system(|input: In<f32>, time: Res<Time>| time.delta_secs() + *input);
    /// AsyncWorld.run_system_with_input(id, 4.0).unwrap();
    /// # });
    /// ```
    pub fn run_system_with_input<I: SystemInput + 'static, O: 'static>(
        &self,
        system: SystemId<I, O>,
        input: I::Inner<'_>,
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
    /// AsyncWorld.run_system_cached(|time: Res<Time>| println!("{}", time.delta_secs())).unwrap();
    /// # });
    /// ```
    pub fn run_system_cached<O: 'static, M, S: IntoSystem<(), O, M> + 'static>(
        &self,
        system: S,
    ) -> AccessResult<O> {
        with_world_mut(move |world: &mut World| {
            world
                .run_system_cached(system)
                .map_err(|_| AccessError::SystemIdNotFound)
        })
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
    /// AsyncWorld.run_system_cached_with_input(|input: In<f32>, time: Res<Time>| time.delta_secs() + *input, 4.0).unwrap();
    /// # });
    /// ```
    pub fn run_system_cached_with_input<
        I: SystemInput + 'static,
        O: 'static,
        M,
        S: IntoSystem<I, O, M> + 'static,
    >(
        &self,
        system: S,
        input: I::Inner<'_>,
    ) -> AccessResult<O> {
        with_world_mut(move |world: &mut World| {
            world
                .run_system_cached_with(system, input)
                .map_err(|_| AccessError::SystemIdNotFound)
        })
    }

    /// Spawns a new [`Entity`] with no components.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!(
    /// AsyncWorld.spawn_empty()
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
    /// AsyncWorld.spawn_bundle(SpriteBundle::default())
    /// # );
    /// ```
    pub fn spawn_bundle(&self, bundle: impl Bundle) -> AsyncEntityMut {
        self.entity(with_world_mut(move |world: &mut World| {
            world.spawn(bundle).id()
        }))
    }

    /// Initializes a new resource.
    ///
    /// If the resource already exists, nothing happens.
    pub fn init_resource<R: Resource + FromWorld>(&self) -> AsyncResource<R> {
        with_world_mut(move |world: &mut World| world.init_resource::<R>());
        self.resource()
    }

    /// Inserts a new resource with the given value.
    pub fn insert_resource<R: Resource>(&self, resource: R) -> AsyncResource<R> {
        with_world_mut(move |world: &mut World| world.insert_resource(resource));
        self.resource()
    }

    /// Transition to a new [`States`].
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// AsyncWorld.set_state(MyState::A)
    /// # });
    /// ```
    pub fn set_state<S: FreelyMutableState>(&self, state: S) -> AccessResult<()> {
        with_world_mut(move |world: &mut World| {
            world
                .get_resource_mut::<NextState<S>>()
                .map(|mut s| s.set(state))
                .ok_or(AccessError::ResourceNotFound {
                    name: type_name::<State<S>>(),
                })
        })
    }

    /// Obtain a [`States`].
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// AsyncWorld.get_state::<MyState>()
    /// # });
    /// ```
    pub fn get_state<S: States>(&self) -> AccessResult<S> {
        with_world_ref(|world| {
            world
                .get_resource::<State<S>>()
                .map(|s| s.get().clone())
                .ok_or(AccessError::ResourceNotFound {
                    name: type_name::<State<S>>(),
                })
        })
    }

    /// Wait until a [`States`] is entered.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// AsyncWorld.in_state(MyState::A)
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

    /// Obtain a `Stream` that reacts to changes of a [`States`].
    ///
    /// Requires system [`react_to_state`](crate::systems::react_to_state).
    pub fn state_stream<S: States>(&self) -> impl FusedStream<Item = S> + '_ {
        let signal = self.typed_signal::<StateSignal<S>>();
        signal.make_readable();
        signal.into_stream()
    }

    /// Send an [`Event`].
    pub fn send_event<E: Event>(&self, event: E) -> AccessResult<EventId<E>> {
        with_world_mut(move |world: &mut World| {
            world
                .send_event(event)
                .ok_or(AccessError::EventNotRegistered {
                    name: type_name::<E>(),
                })
        })
    }

    /// Perform a blocking operation on [`AsyncComputeTaskPool`].
    pub fn unblock<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + Sync + 'static,
    ) -> impl Future<Output = T> + 'static {
        let (mut send, recv) = async_oneshot::oneshot();
        let handle = AsyncComputeTaskPool::get().spawn(async move {
            let _ = send.send(f());
        });
        async move {
            let result = recv.await.unwrap();
            #[allow(clippy::drop_non_drop)]
            drop(handle);
            result
        }
    }

    /// Pause the future for the duration, according to the `Time` resource.
    ///
    /// # Example
    ///
    /// ```
    /// # bevy_defer::test_spawn!({
    /// AsyncWorld.sleep(5.4).await
    /// # });
    /// ```
    #[must_use = "futures do nothing unless you `.await` or poll them"]
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
    /// AsyncWorld.sleep_frames(12).await
    /// # });
    /// ```
    #[must_use = "futures do nothing unless you `.await` or poll them"]
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
    /// AsyncWorld.quit()
    /// # });
    /// ```
    pub fn quit(&self) {
        with_world_mut(move |world: &mut World| {
            world.send_event(AppExit::Success);
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
    /// let signal = AsyncWorld.typed_signal::<MySignal>();
    /// signal.write(3.14);
    /// signal.read_async().await;
    /// # });
    /// ```
    pub fn typed_signal<T: SignalId>(&self) -> Value<T::Data> {
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
    /// let signal = AsyncWorld.named_signal::<MySignal>("signal 1");
    /// signal.write(3.14);
    /// signal.read_async().await;
    /// # });
    /// ```
    pub fn named_signal<T: SignalId>(&self, name: &str) -> Value<T::Data> {
        if !REACTORS.is_set() {
            panic!("Can only obtain named signal in async context.")
        }
        REACTORS.with(|signals| signals.get_named::<T>(name))
    }
}
