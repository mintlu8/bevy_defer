use std::collections::HashSet;
use std::marker::{PhantomData, PhantomPinned};
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct AsyncTrigger<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle = ()>(
    bevy::prelude::Entity,
    PhantomData<(E, B, PhantomPinned)>,
);

impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle> Clone
    for AsyncTrigger<E, B>
{
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}

#[rustfmt::skip]
/// Rust allows you to leak things, so this isn't unsafe, but because we don't implement a custom drop
/// in order to maintain our important `Copy` status, we may end up not running the destructor on `E`
/// when the entity the AsyncObserver is spawned on despawns.
impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle> Copy
    for AsyncTrigger<E, B> {}

#[derive(bevy::prelude::Component)]
pub struct AsyncObserver<E: bevy::prelude::Event + 'static, B: bevy::prelude::Bundle = ()> {
    cleared: HashSet<usize>,
    phantom_data: PhantomData<(E, B)>,
    data: Option<(E, bevy::prelude::Entity)>,
}
impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle> AsyncObserver<E, B> {
    pub fn get(&self) -> Option<(E, bevy::prelude::Entity)> {
        self.data.as_ref().cloned()
    }
}

pub trait AsyncTriggerExt<
    E: bevy::prelude::Event + Clone + 'static,
    B: bevy::prelude::Bundle,
    Entity,
>
{
    fn entity(entity: Entity) -> AsyncTrigger<E, B>;
}
impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle> AsyncTrigger<E, B> {
    fn new(entity: bevy::prelude::Entity) -> AsyncTrigger<E, B> {
        crate::AsyncWorld.run(|world| {
            match world.entity_mut(entity).get::<AsyncObserver<E, B>>() {
                None => {
                    world.entity_mut(entity).insert(AsyncObserver::<E, B> {
                        cleared: HashSet::new(),
                        phantom_data: Default::default(),
                        data: None,
                    });
                    let component_id = world.register_component::<AsyncObserver<E, B>>();
                    world.commands().entity(entity).observe(
                        move |e: bevy::prelude::Trigger<E, B>,
                              mut async_observer: bevy::prelude::Query<
                            &mut AsyncObserver<E, B>,
                        >| {
                            let mut async_observer = async_observer.get_mut(entity).unwrap();
                            async_observer.data = Some((e.event().clone(), e.target()));
                            async_observer.cleared.clear();
                        },
                    );
                }
                Some(_) => {}
            }
        });
        AsyncTrigger(entity, PhantomData)
    }
}

impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle>
    AsyncTriggerExt<E, B, bevy::prelude::Entity> for bevy::prelude::Trigger<'_, E, B>
{
    fn entity(entity: bevy::prelude::Entity) -> AsyncTrigger<E, B> {
        AsyncTrigger::new(entity)
    }
}
impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle>
    AsyncTriggerExt<E, B, crate::access::AsyncEntityMut> for bevy::prelude::Trigger<'_, E, B>
{
    fn entity(entity: crate::access::AsyncEntityMut) -> AsyncTrigger<E, B> {
        AsyncTrigger::new(entity.id())
    }
}

impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle>
    AsyncTriggerExt<E, B, &bevy::prelude::Entity> for bevy::prelude::Trigger<'_, E, B>
{
    fn entity(entity: &bevy::prelude::Entity) -> AsyncTrigger<E, B> {
        AsyncTrigger::new(*entity)
    }
}

impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle> std::future::Future
    for AsyncTrigger<E, B>
{
    type Output = (E, bevy::prelude::Entity);

    //noinspection ALL
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use crate::AsyncAccess;
        crate::AsyncWorld
            .entity(self.0)
            .component::<AsyncObserver<E, B>>()
            .get_mut(|observer| {
                if observer
                    .cleared
                    // Because its `Pin` we are guaranteed the pointer address is the same every time!
                    // How wonderful!
                    .contains(&(self.as_ref().get_ref() as *const Self as usize))
                {
                    crate::executor::QUERY_QUEUE.with(|queue| queue.yielded.push_cx(cx));
                    return Poll::Pending;
                }
                match observer.get() {
                    None => {
                        crate::executor::QUERY_QUEUE.with(|queue| queue.yielded.push_cx(cx));
                        Poll::Pending
                    }
                    Some(data) => {
                        observer
                            .cleared
                            // Because its `Pin` we are guaranteed the pointer address is the same every time!
                            // How wonderful!
                            .insert(self.as_ref().get_ref() as *const Self as usize);
                        Poll::Ready(data)
                    }
                }
            })
            // Because we don't queue the executor here the future disappears
            .unwrap_or_else(|_| Poll::Pending)
    }
}
