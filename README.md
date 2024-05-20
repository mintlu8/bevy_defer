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
```

At each `await` point we wait for something to complete, without wasting resources
spin looping a thread or defining a complex state machine in a system.

What if we want damage number and damage vfx to run concurrently and wait for both
before our next attack? It's simple with `async` semantics!

```rust, ignore
futures::join! {
    show_damage_number(),
    damage_vfx()
};

swing_animation().await;
```

## Why not `bevy_tasks`?

`bevy_tasks` has no direct world access, which makes it difficult to write game
logic in it.

The core idea behind `bevy_defer` is straightforward:

```rust, ignore
// Pseudocode
static WORLD_CELL: Mutex<&mut World>;

fn run_async_executor_system(world: &mut World) {
    let executor = world.get_executor();
    WORLD_CELL.set(world);
    executor.run();
    WORLD_CELL.remove(world);
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

## Spawning a Task

You can spawn a task onto `bevy_defer`
from `World`, `App`, `Commands`, `AsyncWorld` or `AsyncExecutor`.

Here is an example:

```rust, ignore
commands.spawn_task(|| async move {
    // Wait for state to be `GameState::Animating`.
    AsyncWorld.state_stream::<GameState>().filter(|x| x == &GameState::Animating).next().await;
    // Obtain info from a resource.
    // Since the `World` stored as a thread local, 
    // a closure is the preferable syntax to access it.
    let richard_entity = AsyncWorld.resource::<NamedEntities>()
        .get(|res| *res.get("Richard").unwrap())?;
    // Move to an entity's scope, does not verify the entity exists.
    let richard = AsyncWorld.entity(richard_entity);
    // We can also mutate the world directly.
    richard.component::<HP>().set(|hp| hp.set(500))?;
    // Move to a component's scope, does not verify the entity or component exists.
    let animator = AsyncWorld.component::<Animator>();
    // Implementing `AsyncComponentDeref` allows you to add extension methods to `AsyncComponent`.
    animator.animate("Wave").await?;
    // Spawn another future on the executor.
    let audio = AsyncWorld.spawn(sound_routine(richard_entity));
    // Dance for 5 seconds with `select`.
    futures::select!(
        _ = animator.animate("Dance").fuse() => (),
        _ = AsyncWorld.sleep(Duration::from_secs(5)) => println!("Dance cancelled"),
    );
    // animate back to idle
    richard.component::<Animator>().animate("Idle").await?;
    // Wait for spawned future to complete
    audio.await?;
    // Tell the bevy App to quit.
    AsyncWorld.quit();
    Ok(())
});
```

## World Accessors

The entry point of all world access is `AsyncWorld`,
for example a `Component` can be accessed by

```rust, ignore
let translation = AsyncWorld
    .entity(entity)
    .component::<Transform>()
    .get(|t| {
        t.translation
    }
)
```

This works for all the bevy things you expect, `Resource`, `Query`, etc.
See the `access` module and the `AsyncAccess` trait for more detail.

You can add extension methods to these accessors via `Deref` if you own the
underlying types. See the `access::deref` module for more detail. The
`async_access` derive macro can be useful for adding method to
async accessors.

We do not provide a `AsyncSystemParam`, instead you should use
one-shot system based API on `AsyncWorld`.
They can cover all uses cases where you need to running systems in `bevy_defer`.

## Async Basics

Here are some common utilities you might find useful from an async ecosystem.

* `AsyncWorld.spawn()` spawns a future.

* `AsyncWorld.spawn_scoped()` spawns a future with a handle to get result from.

* `AsyncWorld.yield_now()` yields execution for the current frame, similar to how coroutines work.

* `AsyncWorld.sleep(4.0)` pauses the future for `4` seconds.

* `AsyncWorld.sleep_frame(4)` pauses the future for `4` frames.

## Bridging Sync and Async

Communicating between sync and async can be daunting for new users. See
this amazing tokio article: <https://tokio.rs/tokio/topics/bridging>.

Communicating from sync to async is simple, async code can provide channels
to sync code and `await` on them, pausing the task.
Once sync code sends data through the channel, it will
wake and resume the corresponding task.

Communicating from async to sync require more thought.
This usually means mutating the world in an async function,
then a system can listen for that particular change in sync code.

```rust,ignore
async {
    entity.component::<IsJumping>().set(|j| *j == true);
}

pub fn jump_system(query: Query<Name, Changed<IsJumping>>) {
    for name in &query {
        println!("{} is jumping!", name);
    }
}
```

The core principle is async code should help sync code to
do less work, and vice versa!

## Signals and AsyncSystems

`AsyncSystems` and `Signals` provides per-entity reactivity for user interfaces.
Checkout their respective modules for more information.

## Implementation Details

`bevy_defer` uses a single threaded runtime that always runs on bevy's main thread inside the main schedule,
this is ideal for simple game logic, wait heavy or IO heavy tasks, but CPU heavy tasks should not be run in `bevy_defer`.
The `AsyncComputeTaskPool` in `bevy_tasks` is ideal for this use case.
We can use `AsyncComputeTaskPool::get().spawn()` to spawn a future on task pool and call `await` in `bevy_defer`.

## Usage Tips

The `futures` and/or `futures_lite` crate has excellent tools to for us to use.

For example `futures::join!` can be used to run tasks concurrently, and
`futures::select!` can be used to cancel tasks, for example despawning a task
if a level has finished.

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
