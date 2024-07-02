use crate::reactors::Change;
use crate::signals::{Sender, SignalSender, Signals};
use crate::tween::AsSeconds;
use crate::{
    access::{deref::AsyncComponentDeref, AsyncComponent},
    AccessResult, AsyncAccess,
};
use crate::{AccessError, AsyncWorld, OwnedQueryState};
use bevy_animation::prelude::{AnimationNodeIndex, AnimationTransitions};
use bevy_animation::AnimationPlayer;
use bevy_ecs::component::Component;
use bevy_ecs::entity::{Entity, EntityHashMap};
use bevy_ecs::query::With;
use bevy_ecs::system::Local;
use bevy_ecs::{query::Changed, system::Query};
use futures::{Future, FutureExt};
use ref_cast::RefCast;
use rustc_hash::FxHashMap;
use std::ops::{Deref, DerefMut};

/// Async accessor to [`AnimationPlayer`].
#[derive(RefCast)]
#[repr(transparent)]
pub struct AsyncAnimationPlayer(AsyncComponent<AnimationPlayer>);

impl AsyncComponentDeref for AnimationPlayer {
    type Target = AsyncAnimationPlayer;

    fn async_deref(this: &AsyncComponent<Self>) -> &Self::Target {
        AsyncAnimationPlayer::ref_cast(this)
    }
}

/// Async accessor to [`AnimationTransitions`].
#[derive(RefCast)]
#[repr(transparent)]
pub struct AsyncAnimationTransitions(AsyncComponent<AnimationTransitions>);

impl AsyncComponentDeref for AnimationTransitions {
    type Target = AsyncAnimationTransitions;

    fn async_deref(this: &AsyncComponent<Self>) -> &Self::Target {
        AsyncAnimationTransitions::ref_cast(this)
    }
}

impl AsyncAnimationPlayer {
    /// Start playing an animation, restarting it if necessary.
    pub fn start(&self, clip: AnimationNodeIndex) -> AccessResult {
        self.0.set(move |player| {
            player.start(clip);
        })
    }

    /// Start playing an animation.
    pub fn play(&self, clip: AnimationNodeIndex) -> AccessResult {
        self.0.set(move |player| {
            player.play(clip);
        })
    }

    /// Start playing an animation and repeat.
    pub fn play_repeat(&self, clip: AnimationNodeIndex) -> AccessResult {
        self.0.set(move |player| {
            player.play(clip).repeat();
        })
    }

    /// Stop playing an animation.
    pub fn stop(&self, clip: AnimationNodeIndex) -> AccessResult {
        self.0.set(move |player| {
            player.stop(clip);
        })
    }

    /// Stop playing all animations.
    pub fn stop_all(&self) -> AccessResult {
        self.0.set(move |player| {
            player.stop_all();
        })
    }

    /// Stop playing an animation.
    pub fn is_playing_animation(&self, clip: AnimationNodeIndex) -> AccessResult<bool> {
        self.0.get(move |player| player.is_playing_animation(clip))
    }

    pub fn wait(&self, event: AnimationEvent) -> AccessResult<impl Future<Output = ()> + 'static> {
        let (send, recv) = async_oneshot::oneshot();
        AsyncWorld.run(|w| {
            let Some(mut entity) = w.get_entity_mut(self.0.entity) else {
                return Err(AccessError::EntityNotFound);
            };
            if let Some(mut reactors) = entity.get_mut::<AnimationReactor>() {
                reactors.push((event, send));
            } else {
                entity.insert(AnimationReactor(vec![(event, send)]));
            };
            Ok(())
        })?;
        Ok(recv.map(|_| ()))
    }
}

impl AsyncAnimationTransitions {
    pub fn play(&self, animation: AnimationNodeIndex, transition: impl AsSeconds) {
        AsyncWorld.run(|w| {
            let mut query =
                OwnedQueryState::<(&mut AnimationPlayer, &mut AnimationTransitions), ()>::new(w);
            if let Ok((mut player, mut transitions)) = query.get_mut(self.0.entity()) {
                transitions.play(&mut player, animation, transition.as_duration());
            }
        })
    }
}

#[derive(Debug)]
pub enum AnimationEvent {
    /// Yields when current animation is not the one specified.
    OnExit(AnimationNodeIndex),
    /// Yields when current animation is the one specified.
    OnEnter(AnimationNodeIndex),
    /// Yields when the last animation is the one specified
    /// and current animation is not the one specified.
    WaitUnitOnExit(AnimationNodeIndex),
    /// Yields when the specified animation passes a specific frame.
    OnFrame {
        animation: AnimationNodeIndex,
        frame: f32,
    },
    /// Yields when the specified animation passes a specific frame,
    /// or when the specified animation exits.
    OnFrameOrExit {
        animation: AnimationNodeIndex,
        frame: f32,
    },
}

#[derive(Component, Clone)]
pub struct PreviousAnimationPlayer(AnimationPlayer);

impl Deref for PreviousAnimationPlayer {
    type Target = AnimationPlayer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PreviousAnimationPlayer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Component)]
pub struct AnimationReactor(Vec<(AnimationEvent, async_oneshot::Sender<()>)>);

impl Deref for AnimationReactor {
    type Target = Vec<(AnimationEvent, async_oneshot::Sender<()>)>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AnimationReactor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// /// Reactor to [`AnimationClip`] in [`AnimationPlayer`] changed as [`AnimationChange`].
pub fn react_to_animation(
    mut query: Query<
        (
            &AnimationPlayer,
            &mut PreviousAnimationPlayer,
            &mut AnimationReactor,
        ),
        Changed<AnimationPlayer>,
    >,
) {
    for (player, mut prev, mut reactors) in query.iter_mut() {
        reactors.0.retain_mut(|(event, channel)| {
            let yields = match event {
                AnimationEvent::OnExit(idx) => !player.is_playing_animation(*idx),
                AnimationEvent::OnEnter(idx) => player.is_playing_animation(*idx),
                AnimationEvent::WaitUnitOnExit(idx) => {
                    player.is_playing_animation(*idx) && !prev.0.is_playing_animation(*idx)
                }
                AnimationEvent::OnFrame { animation, frame } => {
                    if let Some(prev) = prev.0.animation(*animation) {
                        if let Some(curr) = player.animation(*animation) {
                            curr.seek_time() >= *frame && *frame > prev.seek_time()
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                AnimationEvent::OnFrameOrExit { animation, frame } => {
                    if let Some(prev) = prev.0.animation(*animation) {
                        if let Some(curr) = player.animation(*animation) {
                            curr.seek_time() >= *frame && *frame > prev.seek_time()
                        } else {
                            true
                        }
                    } else {
                        false
                    }
                }
            };
            if yields {
                let _ = channel.send(());
                false
            } else {
                true
            }
        });
        prev.0.clone_from(player);
    }
}

// // /// Reactor to [`AnimationClip`] in [`AnimationPlayer`] changed as [`AnimationChange`].
// pub fn react_to_animation_main(
//     mut cache: Local<EntityHashMap<AnimationNodeIndex>>,
//     query: Query<
//         (
//             Entity,
//             &AnimationTransitions,
//             SignalSender<Change<AnimationNodeIndex>>
//         ),
//         (With<Signals>, Changed<AnimationPlayer>),
//     >,
// ) {
//     for (entity, transition, sender) in query.iter() {
//         let prev = cache.get(&entity).copied();
//         if !prev == transition.main()
//     }
//     if let Some(entity) = query
// }