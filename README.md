# Bevy Defer

[![Crates.io](https://img.shields.io/crates/v/bevy_defer.svg)](https://crates.io/crates/bevy_defer)
[![Docs](https://docs.rs/bevy_defer/badge.svg)](https://docs.rs/bevy_defer/latest/bevy_defer/)
[![Bevy tracking](https://img.shields.io/badge/Bevy%20tracking-released%20version-lightblue)](https://bevyengine.org/learn/book/plugin-development/)

A simple asynchronous runtime for executing async coroutines.

## Motivation

Async rust is incredible for modelling wait centric tasks like coroutines.
Not utilizing async in game development is a huge waste of potential.

Imagine we want to model a rapid sword attack animation, in async rust this is straightforward:

```rust, ignore
swing_animation().await;
show_damage_number().await;
damage_vfx().await;

swing_animation().await;
show_damage_number().await;
damage_vfx().await;

...
```

At each `await` point we wait for something to complete, without wasting resources
spin looping a thread or defining a complex state machine in a system.

What if we want damage number and damage vfx to run concurrently and wait for both
before out next attack? It's simple with `async` semantics!

```rust, ignore
futures::join! {
    show_damage_number(),
    damage_vfx()
};

swing_animation().await;
```

## Bridging Sync and Async

Communicating between sync and async in notoriously difficult. See
this amazing tokio article: <https://tokio.rs/tokio/topics/bridging>.

Fortunately we are running in lock step with bevy, so a lot of those headache
can be mitigated by using proper communication methods.

Communicating from sync to async is simple, async code can hand out channels
and `await` on them, pausing the task.
Once sync code sends data through the channel, it will
wake and resume the corresponding task.
`bevy_defer` heavily utilizes one-shot channels to perform its operations.

Communicating from async to sync usually requires mutating the world in an async
function, then a system can listen for that particular change in sync code.
This is pretty seamless with regular bevy workflow.

## Spawning

Spawning is a straightforward way to run some logic immediately.

You can spawn a coroutine to schedule some tasks.
The main benefit is this function can take as long as it needs
to complete, instead of a single frame like a normal system.

```rust, ignore
commands.spawn_task(|| async move {
    // This is an `AsyncWorldMut`.
    // like tokio::spawn() this only works in the async context.
    let world = world();
    // Wait for state to be `GameState::Animating`.
    world.in_state(GameState::Animating).await;
    // This function is async because we don't own the world,
    // we send a query request and wait for the response.
    let richard_entity = world.resource::<NamedEntities>()
        .get(|res| *res.get("Richard").unwrap()).await?;
    // Move to an entity's scope, does not verify the entity exists.
    let richard = world.entity(richard_entity);
    // We can also mutate the world asynchronously.
    richard.component::<HP>().set(|hp| hp.set(500)).await?;
    // Move to a component's scope, does not verify the entity or component exists.
    let animator = richard.component::<Animator>();
    // Implementing `AsyncComponentDeref` allows you to add extension methods to `AsyncComponent`.
    animator.animate("Wave").await?;
    // Spawn another future on the executor.
    let audio = spawn(sound_routine(richard_entity));
    // Dance for 5 seconds with `select`.
    futures::select!(
        _ = animator.animate("Dance").fuse() => (),
        _ = world.sleep(Duration::from_secs(5)).fuse() => println!("Dance cancelled"),
    );
    // animate back to idle
    richard.component::<Animator>().animate("Idle").await?;
    // Wait for spawned future to complete
    audio.await?;
    // Tell the bevy App to quit.
    world.quit().await;
    Ok(())
});
```

In fact a single function can drive the entire game!

## World Accessors

We provide types mimicking bevy's types:

| Query Type | Corresponding Bevy/Sync Type |
| ---- | ----- |
| `AsyncWorldMut` | `World` / `Commands` |
| `AsyncEntityMut` | `EntityMut` / `EntityCommands` |
| `AsyncQuery` | `WorldQuery` |
| `AsyncEntityQuery` | `WorldQuery` on `Entity` |
| `AsyncSystemParam` | `SystemParam` |
| `AsyncComponent` | `Component` |
| `AsyncResource` | `Resource` |
| `AsyncNonSend` | `NonSend` |
| `AsyncEventReader` | `EventReader` |
| `AsyncAsset` | `Handle` |

`world` can be accessed by the `world()` method and
for example a `Component` can be accessed by

```rust, ignore
world().entity(entity).component::<Transform>()
```

See the `access` module for more detail.

You can add extension methods to these accessors via `Deref` if you own the
underlying types. See the `extension` module for more detail.

## Signals

Signals are the cornerstone of reactive programming that bridges the sync and async world.
The `Signals` component can be added to an entity, and the `NamedSignals` resource can be used to
provide matching signals when needed.

Here are the guarantees of signals:

* A `Signal` can hold only one value.
* A `Signal` is read at most once per write for every reader.
* Values are not guaranteed to be read if updated in rapid succession.
* Value prior to reader creation will not be read by a new reader.

`Signals` erases the underlying types and utilizes the `SignalId` trait to disambiguate signals,
this ensures no archetype fragmentation.

In systems, you can use `SignalSender` and `SignalReceiver` just like you would in async,
do keep in mind these parameters do not filter archetypes.

## AsyncSystems

`AsyncSystem` is a system-like async function on a specific entity.
The component `AsyncSystems` is a collection of `AsyncSystem`s that runs independently.

### Example

To create an `AsyncSystem`, use a macro:

```rust, ignore
// Scale up for a second when clicked. 
let system = async_system!(|recv: Receiver<OnClick>, transform: AsyncComponent<Transform>|{
    let pos: Vec3 = recv.recv().await;
    transform.set(|transform| transform.scale = Vec3::splat(2.0)).await?;
    world().sleep(1.0).await;
    transform.set(|transform| transform.scale = Vec3::ONE).await?;
})
```

The parameters implement `AsyncEntityParam`.

### How an `AsyncSystem` executes

Think of an `AsyncSystem` like a loop:

* if this `Future` is not running at the start of this frame, run it.
* If the function finishes, rerun on the next frame.
* If the entity is dropped, the `Future` will be cancelled.

So this is similar to

```rust, ignore
spawn(async {
    loop {
        futures::select! {
            _ = async_system => (),
            _ = cancel => break,
        }
    }
})
```

If you want some state to persist, for example keeping a handle alive or using a
`AsyncEventReader`, you might want to implement the async system as a loop:

```rust, ignore
let system = async_system!(|recv: Receiver<OnClick>, mouse_wheel: AsyncEventReader<Input<MouseWheel>>|{
    loop {
        futures::select! {
            _ = recv.recv().fused() => ..,
            pos = mouse_wheel.poll().fused() => ..
        }
    }
})
```

## Thread Locals

You can push resources, `!Send` resources and even `&World` (readonly) onto
thread local storage during execution by adding them to the plugin:

```rust, ignore
AsyncPlugin::empty().with(MyResource).with(World);
```

This allows some access to be immediate without deferring.
If `&world` is available, all `get` access is immediate.
This would block parallelization, however.

## Implementation Details

`bevy_defer` uses a single threaded runtime that always runs on bevy's main thread inside the main schedule,
this is ideal for wait heavy or IO heavy tasks, but CPU heavy tasks should not be run in `bevy_defer`.

The executor runs synchronously as a part of the schedule.
At each execution point, we will poll our futures until no progress can be made.

Imagine `AsyncPlugin::default_settings()` is used, which means we have 3 execution points per frame, this code:

```rust, ignore
let a = query1().await;
let b = query2().await;
let c = query3().await;
let d = query4().await;
let e = query5().await;
let f = query6().await;
```

takes at least 2 frames to complete, since queries are deferred and cannot resolve immediately.

To complete the task faster, try use `futures::join!` or `futures_lite::future::zip` to
run these queries concurrently.

```rust, ignore
let (a, b, c, d, e, f) = futures::join! {
    query1, 
    query2,
    query3,
    query4,
    query5,
    query6,
}.await;
```

## Versions

| bevy | bevy_defer         |
|------|--------------------|
| 0.12 | 0.1                |
| 0.13 | 0.2-latest         |

## License

License under either of

Apache License, Version 2.0 (LICENSE-APACHE or <http://www.apache.org/licenses/LICENSE-2.0>)
MIT license (LICENSE-MIT or <http://opensource.org/licenses/MIT>)
at your option.

## Contribution

Contributions are welcome!

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
