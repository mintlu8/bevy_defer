# Bevy Defer

[![Crates.io](https://img.shields.io/crates/v/bevy_defer.svg)](https://crates.io/crates/bevy_defer)
[![Docs](https://docs.rs/bevy_defer/badge.svg)](https://docs.rs/bevy_defer/latest/bevy_defer/)
[![Bevy tracking](https://img.shields.io/badge/Bevy%20tracking-released%20version-lightblue)](https://bevyengine.org/learn/book/plugin-development/)

A simple asynchronous runtime for executing deferred queries.

## Getting Started

There are two main ways to utilize this crate, `spawn_task` and `AsyncSystems`.

## Spawning in a Sync Context

This is a straightforward way to implement some logic that can wait
for signals, animations or other events to complete.
In fact you can create your game logic
entirely in async rust!

```rust
commands.spawn_task(async move {
    // This is an `AsyncWorldMut`.
    // like tokio::spawn() this only works in the async context.
    let world = world();
    // Wait for state to be `MyState::Combat`.
    world.in_state(MyState::Combat).await;
    // This function is async because we don't own the world,
    // we send a query request and wait for the response.
    let richard = world.resource::<NamedEntities>()
        .get(|res| res.get("Richard").unwrap()).await?;
    // We can also mutate the world asynchronously.
    world.entity(richard).component::<HP>()
        .set(|hp| hp.set(500)).await?;
    // Implementing `AsyncComponentDeref` allows you to add functions to `AsyncComponent`.
    world.entity(richard).component::<MyAnimator>()
        .animate("Wave").await?;
    // Dance for 5 seconds with `select`.
    futures::select!(
        _ = world.entity(richard).component::<MyAnimator>().animate("Dance"),
        _ = world.wait(Duration::from_seconds(5))
    ).await;
    world.entity(richard).component::<MyAnimator>().animate("Idle").await;
    // Spawn another future on the executor and wait for it to complete
    spawn(sound_routine).await;
    // Returns `Result<(), AsyncFailure>`
    Ok(())
});
```

You can call spawn on `Commands`, `World` or `App`.

## AsyncSystems

`AsyncSystems` is a per entity system-like function that can power reactive UIs
and other similar systems.

`AsyncSystems` have a simple premise: given two components `Signals` and `AsyncSystems`,
we can make any query we want through async semantic.
Which can be done at entity creation site without world access.

`Signals` stores synchronization primitives that
allows easy and robust cross entity communication. Combined with signals,
`AsyncSystems` allows inter-entity communication with ease.

### How do I use `AsyncSystems`?

To create an `AsyncSystems`, create an `AsyncSystem` first via a macro:

```rust
// Set scale based on received position
let system = async_system!(|recv: Receiver<PositionChanged>, transform: Ac<Transform>|{
    let pos: Vec3 = recv.recv().await;
    transform.set(|transform| transform.scale = pos).await?;
})
```

Then create a `AsyncSystems` from it:

```rust
let systems = AsyncSystems::from_single(system);
// or
let systems = AsyncSystems::from_systems([a, b, c, ...]);
```

Add the associated `Signal`:

```rust
let signal = Signals::from_receiver::<PositionChanged>(sig);
```

Spawn them as a `Bundle` and that's it! The async executor will
handle it from here.

### Let's break it down

```rust
Signals::from_receiver::<PositionChanged>(sig)
```

`PositionChanged` is a `SignalId`, or a discriminant + an associated type.
In this case the type of the signal is `Vec3`. We can have multiple signals
on an `Entity` with type `Vec3`, but not with the same `SignalId`.

```rust
|recv: Receiver<PositionChanged>, transform: AsyncComponent<Transform>| { .. }
```

`Receiver` receives the signal, `AsyncComponent` allows us to get or set data
on a component within the same entity.

Notice the function signature is a bit weird, since the macro roughly expands to

```rust
move | context | async move { 
    let recv = from_context(context);
    let transform = from_context(context);
    {..}; 
    return Ok(()); 
}
```

The body of the `AsyncSystem`. Await will wait for queued queries to complete.

```rust
let pos: Vec3 = recv.recv().await;
transform.set(|transform| transform.scale = pos).await?;
```

### How does this `AsyncSystem` run?

You can treat this system like a loop

1. At the start of the frame, run this function if not already running.
2. Wait until something sends the signal.
3. Write received position to the `Transform` component.
4. Wait for the write query to complete.
5. End and repeat step `1` on the next frame.

## Supported System Params

| Query Type | Corresponding Bevy/Sync Type |
| ---- | ----- |
| `AsyncWorldMut` | `World` / `Commands` |
| `AsyncEntityMut` | `EntityMut` / `EntityCommands` |
| `AsyncQuery` | `WorldQuery` |
| `AsyncEntityQuery` | `WorldQuery` on `Entity` |
| `AsyncSystemParam` | `SystemParam` |
| `AsyncComponent` | `Component` |
| `AsyncResource` | `Resource` |
| `Sender` | `Signals` |
| `Receiver` | `Signals` |

You can create your own `AsyncEntityParam` by implementing it.

## Signals

A `Signal` is read at most once per write for every reader.
Senders can also read from signals, if `send` is used,
the value can be read from the same sender. If `broadcast` is used,
the same sender cannot read the value sent.

## Implementation Details

`bevy_defer` uses a single threaded runtime that always runs on bevy's main thread,
this is ideal for wait heavy or IO heavy tasks, but CPU heavy tasks should not be run here.

The executor runs synchronously as a part of the schedule.
At each execution point, we will poll our futures until no progress can be made.

Imagine `DefaultAsyncPlugin` is used, which means we have 3 execution points per frame, this code:

```rust
let a = query1.await;
let b = query2.await;
let c = query3.await;
let d = query4.await;
let e = query5.await;
let f = query6.await;
```

takes at least 2 frames to complete, since queries are deferred and cannot resolve immediately.

To complete the task faster, try use `futures::join!` or `futures_lite::future::zip` to
run these queries concurrently.

```rust
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
