use std::hint::black_box;
use crate::access::AsyncEntityMut;
use bevy::ecs::component::ComponentId;
use bevy::prelude::{OnInsert, ResMut, World};
use futures::TryFutureExt;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct AsyncTrigger<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle = ()>(
    bevy::prelude::Entity,
    PhantomData<(E, B)>,
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

#[repr(C)]
#[derive(bevy::prelude::Component)]
pub struct AsyncObserver<E: bevy::prelude::Event + 'static, B: bevy::prelude::Bundle = ()> {
    cleared: bool,
    phantom_data: PhantomData<(E, B)>,
    data: MaybeUninit<(E, bevy::prelude::Entity)>,
}
impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle> AsyncObserver<E, B> {
    pub fn get(&self) -> Option<(E, bevy::prelude::Entity)> {
        if self.cleared {
            None
        } else {
            // SAFETY: `cleared == false` means the tuple is definitely initialised.
            Some(unsafe { self.data.assume_init_ref().clone() })
        }
    }
}

impl<E: bevy::prelude::Event + Clone + 'static, B: bevy::prelude::Bundle> Clone
    for AsyncObserver<E, B>
{
    fn clone(&self) -> Self {
        Self {
            cleared: self.cleared,
            phantom_data: Default::default(),
            data: if self.cleared {
                MaybeUninit::uninit()
            } else {
                unsafe { MaybeUninit::new(self.data.assume_init_ref().clone()) }
            },
        }
    }
}

#[derive(bevy::prelude::Resource, bevy::prelude::Deref, bevy::prelude::DerefMut, Default)]
pub(crate) struct AsyncObserversToClear(Vec<(bevy::prelude::Entity, ComponentId)>);

impl AsyncObserversToClear {
    pub(crate) fn run_clear_cycle(&mut self, world: &mut World) {
        for (entity, component_id) in self.0.drain(0..) {
            if let Ok(ptr) = world.entity_mut(entity).get_mut_by_id(component_id) {
                let raw = ptr.into_inner().as_ptr();
                unsafe {
                    let flag_ptr = raw.cast::<bool>();
                    println!("Flag is: {}", std::ptr::read(flag_ptr));
                    std::ptr::write(flag_ptr, true);
                }
            }
        }
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
                        cleared: true,
                        phantom_data: Default::default(),
                        data: MaybeUninit::uninit(),
                    });
                    let component_id = world.register_component::<AsyncObserver<E, B>>();
                    world.commands().entity(entity).observe(
                        move |e: bevy::prelude::Trigger<E, B>,
                              mut async_observer: bevy::prelude::Query<&mut AsyncObserver<E, B>>,
                              mut async_observers_to_clear: ResMut<AsyncObserversToClear>| {
                            let mut async_observer = async_observer.get_mut(entity).unwrap();
                            async_observer.data = MaybeUninit::new((e.event().clone(), e.target()));
                            async_observer.cleared = true;
                            async_observers_to_clear.push((e.target(), component_id));
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
    fn entity(entity: AsyncEntityMut) -> AsyncTrigger<E, B> {
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
        match crate::AsyncWorld
            .entity(self.0)
            .component::<AsyncObserver<E, B>>()
            .get_mut(|observer| match observer.get() {
                None => {
                    crate::executor::QUERY_QUEUE.with(|queue| queue.yielded.push_cx(cx));
                    Poll::Pending
                }
                Some(data) => Poll::Ready(data),
            })
            .map_err(|_| Poll::Pending)
        {
            Ok(data) | Err(data) => data,
        }
    }
}
