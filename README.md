# Bevy Defer

[![Crates.io](https://img.shields.io/crates/v/bevy_defer.svg)](https://crates.io/crates/bevy_defer)
[![Docs](https://docs.rs/bevy_defer/badge.svg)](https://docs.rs/bevy_defer/latest/bevy_defer/)
[![Bevy tracking](https://img.shields.io/badge/Bevy%20tracking-released%20version-lightblue)](https://bevyengine.org/learn/book/plugin-development/)

A simple asynchronous runtime for executing async coroutines.

## Motivation

`bevy_defer` is an async runtime for bevy with world access. Think of a future
in `bevy_defer` as a `Command` that can take as long as it wants to complete.
They can even wait for each other or pause for a few seconds,
which is pretty powerful!

* So who needs `bevy_defer`?

`bevy_defer` is not for every use case as it is somewhat opposite to `ecs`s
design goal, if you have a game that might not want ecs only, `bevy_defer`
might help you make your life easier.

## Common use cases

* Powerful Abstractions

`Pin<Box<dyn Future>>` in `bevy_defer` is the strongest possible abstraction
in bevy. Put that on a button's `OnClick` and it can do anything you want,
much more powerful than a command or a system.

* Turn based gameplay and event orchestration

In turn based games, you generally want to sequence actions after the player's input.
in `bevy_defer` this is incredibly simple:

```rust, ignore
move_to(position).await;
let damage = attack(enemy).await;
show_damage(damage).await;
```

* UI reactivity

`bevy_defer` provides an alternative model to handle reactivity using
async rust.

## Getting Started

First add `AsyncPlugin`

```rust, ignore
app.add_plugins(AsyncPlugin::default_settings())
```

If you want to react to events, states, or other things,
add their corresponding `react_to` functions.

```rust, ignore
app.react_to_state::<PlayerAlive>();
app.react_to_event::<PlayerAttack>();
```

### Spawning a Task

You can spawn a task onto `bevy_defer`
from `World`, `App`, `Commands`, `AsyncWorld` or `AsyncExecutor`.

Here is an example:

```rust, ignore
commands.spawn_task(|| async move {
    AsyncWorld.sleep(500.0).await;
    println!("Hello, World!");
    Ok(())
});
```

### Accessing the world

The entry point of all world access is `AsyncWorld`,
for example a `Component` can be accessed by

```rust, ignore
let translation = AsyncWorld
    .entity(entity)
    .component::<Transform>()
    .get(|t| {
        t.translation
    }).unwrap()
```

This is similar to `World` but all validation are
deferred to the access function.

* New in `0.13`: the `fetch!` macro can be used:

```rust, ignore
let translation = fetch!(entity, Transform).get(|t| t.translation).unwrap()
```

To set some data:

```rust, ignore
let richard = AsyncWorld.entity(richard_entity);
richard.component::<HP>().get_mut(|hp| hp.set(500))?;
```

This works for all the bevy things you expect, `Resource`, `Query`, etc.
See the `access` module and the `AsyncAccess` trait for more detail.

You can add extension methods to these accessors via `Deref` if you own the
underlying types. See the `access::deref` module for more detail. The
`async_access` derive macro can be useful for adding method to
async accessors.

### Run Systems

You might have noticed `AsyncSystemParam` does not exist.
In order to run more complicated logic, use one of the one-shot
system based API, like:

```rust, ignore
AsyncWorld.run_cached_system(my_system)
AsyncWorld.run_cached_system_with_input(my_system2, 4)
```

### Event Orchestration

* Coroutines

`AsyncWorld.yield_now()` yields execution for the current frame,
with default settings this makes code like this run once per frame.

```rust, ignore
loop {
    transform.set(|t| t.translation.x += delta_time)?;
    AsyncWorld.yield_now().await
}
```

* Pausing

`AsyncWorld.sleep(4.0)` pauses the future for `4` seconds,
while `AsyncWorld.sleep_frames(4)` pauses the future for `4` frames.

* Concurrency

Use `futures::join!` or `futures_lite::zip` to achieve concurrency:

```rust, ignore
// Do both at the same time.
join! {
    dance(),
    play_music(),
}
```

* Cancellation

Use `futures::select!` or `futures_lite::or` to achieve cancellation:

```rust, ignore
// Dance for no more than 5 seconds.
select! {
    _ = dance() => (),
    _ = sleep(5.0) => (),
}
```

## Bridging Sync and Async

Communicating between sync and async can be daunting for new users. See
this amazing tokio article: <https://tokio.rs/tokio/topics/bridging>.

Communicating from sync to async is simple, async code can provide channels
to sync code and `await` on them, pausing the task.
Once sync code sends data through the channel, it will
wake and resume the corresponding task.

Communicating from async to sync require more thought.
This usually means mutating the world or sending an event
in an async function,
then a system can listen for that particular change in sync code.

```rust,ignore
async {
    fetch!(entity, IsJumping).set(|j| *j == true);
    AsyncWold.send_event(Jump(entity))
}

pub fn jump_system(query: Query<Name, Changed<IsJumping>>) {
    for name in &query {
        println!("{} is jumping!", name);
    }
}
```

`States` is particularly powerful for this type of communication:

```rust, ignore
AsyncWorld.set_state(GameState::Loading);
```

The core principle is async code should help sync code to
do less work, and vice versa!

## Comparison with `bevy_tasks`

`bevy_tasks` has no direct world access, which makes it difficult to write game
logic in it.

The core idea behind `bevy_defer` is simple:

```rust, ignore
// Pseudocode
static WORLD_CELL: Mutex<&mut World>;

fn run_async_executor(world: &mut World) {
    let executor = world.get_executor();
    WORLD_CELL.set(world);
    executor.run();
    WORLD_CELL.clear();
}
```

Futures spawned onto the executor can access the `World`
via access functions, similar to how database transaction works:

```rust, ignore
WORLD_CELL.with(|world: &mut World| {
    world.entity(entity).get::<Transform>().clone()
})
```

As long as no references can be borrowed from the world,
and the executor is single threaded, this is perfectly sound!

## Implementation Details

`bevy_defer` uses a single threaded runtime that always runs on bevy's main thread inside the main schedule,
this is ideal for simple game logic, wait heavy or IO heavy tasks, but CPU heavy tasks should not be run in `bevy_defer`. Unlike multithreaded runtimes, single threaded runtimes are more vulnerable to
blocking tasks, so pay extra attention to this.

`AsyncWorld::unblock` is an abstraction for using `AsyncComputeTaskPool` to run cpu intensive tasks. for blocking io, checkout crates like `async-fs` which are more suited for these tasks.

## Versions

| bevy | bevy_defer         |
|------|--------------------|
| 0.12 | 0.1                |
| 0.13 | 0.2-0.11           |
| 0.14 | 0.12               |
| 0.15 | 0.13-latest        |

## License

License under either of

Apache License, Version 2.0 (LICENSE-APACHE or <http://www.apache.org/licenses/LICENSE-2.0>)
MIT license (LICENSE-MIT or <http://opensource.org/licenses/MIT>)
at your option.

## Contribution

Contributions are welcome!

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
