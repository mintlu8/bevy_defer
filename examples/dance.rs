use bevy::tasks::futures_lite::StreamExt;
use bevy::MinimalPlugins;
use bevy::prelude::*;
use bevy_defer::{
    access::{deref::AsyncComponentDeref, AsyncComponent, AsyncWorld},
    signal_ids, AccessError, AppReactorExtension, AsyncAccess, AsyncExtension, AsyncPlugin,
};
use futures::FutureExt;
use ref_cast::RefCast;
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    pin::pin,
    time::Duration,
};
signal_ids! {
    SigText: &'static str,
}

#[derive(Component)]
pub struct HP(usize);

impl HP {
    pub fn set(&mut self, value: usize) {
        println!("HP from {} to {}.", self.0, value);
        self.0 = value;
    }
}

#[derive(Component)]
pub struct Animator(String);

#[derive(Debug, States, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy, Default)]
pub enum GameState {
    #[default]
    Menu,
    Animating,
}

#[derive(Debug, Resource, Default)]
pub struct NamedEntities(HashMap<String, Entity>);

impl Deref for NamedEntities {
    type Target = HashMap<String, Entity>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NamedEntities {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AsyncComponentDeref for Animator {
    type Target = AsyncAnimator;

    fn async_deref(this: &AsyncComponent<Self>) -> &Self::Target {
        AsyncAnimator::ref_cast(this)
    }
}

#[derive(RefCast)]
#[repr(transparent)]
pub struct AsyncAnimator(AsyncComponent<Animator>);

impl AsyncAnimator {
    pub async fn animate(&self, name: &'static str) -> Result<(), AccessError> {
        let len = name.len();
        self.0.set(move |comp| {
            println!("Animating from {} to {}", &comp.0, name);
            name.clone_into(&mut comp.0);
        })?;
        AsyncWorld.sleep(Duration::from_secs(len as u64)).await;
        println!("Animation {name} done!");
        Ok(())
    }

    pub async fn until_exit(&self, name: &'static str) -> Result<(), AccessError> {
        self.0.watch(move |x| (x.0 != name).then_some(())).await
    }
}

async fn sound_routine(entity: Entity) -> Result<(), AccessError> {
    println!("dancing~");
    AsyncWorld
        .entity(entity)
        .component::<Animator>()
        .until_exit("Dance")
        .await?;
    println!("ballet~~");
    AsyncWorld
        .entity(entity)
        .component::<Animator>()
        .until_exit("Ballet")
        .await?;
    println!("fin~~");
    Ok(())
}

pub fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AsyncPlugin::default_settings());
    app.react_to_state::<GameState>();

    let e1 = app
        .world_mut()
        .spawn((HP(0), Animator("Idle".to_owned())))
        .id();
    let e2 = app
        .world_mut()
        .spawn((HP(0), Animator("Idle".to_owned())))
        .id();
    app.insert_resource(NamedEntities(HashMap::from([
        ("Richard".to_owned(), e1),
        ("Jen".to_owned(), e2),
    ])));

    app.spawn_task(async move {
        // This is an `AsyncWorld`.
        // like tokio::spawn() this only works in the async context.
        let world = AsyncWorld;
        let mut three = pin!(&mut world.sleep(3.0));
        let mut two = pin!(&mut world.sleep(2.0));
        let mut one = pin!(&mut world.sleep(1.0));
        loop {
            futures::select!(
                () = one => println!("3"),
                () = two => println!("2"),
                () = three => { println!("1"); break; },
            );
        }

        // Wait for state to be `MyState::Combat`.
        world
            .state_stream::<GameState>()
            .filter(|x| x == &GameState::Animating)
            .next()
            .await
            .unwrap();
        // This function is async because we don't own the world,
        // we send a query request and wait for the response.
        let richard_entity = world
            .resource::<NamedEntities>()
            .get(|res| *res.get("Richard").unwrap())?;
        let richard = world.entity(richard_entity);
        // We can also mutate the world asynchronously.
        richard.component::<HP>().set(|hp| hp.set(500))?;
        // Implementing `AsyncComponentDeref` allows you to add functions to `AsyncComponent`.
        let animator = richard.component::<Animator>();
        animator.animate("Wave").await?;
        let audio = AsyncWorld.spawn_task(sound_routine(richard_entity));
        // Dance for 5 seconds with `select`.
        futures::select!(
            _ = animator.animate("Dance").fuse() => (),
            _ = world.sleep(Duration::from_secs(6)).fuse() => println!("Dance cancelled"),
        );
        futures::select!(
            _ = animator.animate("Ballet").fuse() => (),
            _ = world.sleep(Duration::from_secs(4)).fuse() => println!("Ballet cancelled"),
        );
        richard.component::<Animator>().animate("Idle").await?;
        // Spawn another future on the executor and wait for it to complete
        // Returns `Result<(), AsyncFailure>`
        audio.await?;
        world.quit();
        Ok(())
    });
    app.insert_state(GameState::Animating);
    app.run();
}
