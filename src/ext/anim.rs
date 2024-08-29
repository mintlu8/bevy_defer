//! # Getting Started
//!
//! Add [`PreviousAnimationPlayer`] and [`AnimationReactor`] (optional) next to [`AnimationPlayer`].
//!
//! [`AnimationReactor`] can react to [`AnimationEvent`]s happening.
//!
//! Additionally [`MainAnimationChange`] can react to [`AnimationTransitions`]'s main animation being changed.

use crate::reactors::Change;
use crate::signals::{SignalSender, Signals};
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

    pub fn wait(
        &self,
        event: AnimationEvent,
    ) -> AccessResult<impl Future<Output = bool> + 'static> {
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
        Ok(recv.map(|v| v.unwrap_or(false)))
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
    /// Waits for an animation to exit,
    /// immediately returns if the specified animation is not playing.
    ///
    /// Yields when the current animation is not the one specified.
    OnExit(AnimationNodeIndex),
    /// Waits for an animation to be entered,
    /// immediately returns if the specified animation is playing.
    ///
    /// Yields when current animation is the one specified.
    OnEnter(AnimationNodeIndex),
    /// Wait for an animation to be entered then exited.
    ///
    /// Yields when the last animation is the one specified
    /// and current animation is not the one specified.
    WaitUnitOnExit(AnimationNodeIndex),
    /// Wait for a specific frame to pass or the animation exits.
    ///
    /// Yields when the specified animation passes a specific frame,
    /// returns false if animation exits and the frame is not entered.
    ///
    /// # Note
    ///
    /// Frame is in seconds, if referencing keyframe `15` exported at `60` fps,
    /// the value should be `0.25`.
    OnFrame {
        animation: AnimationNodeIndex,
        frame: f32,
    },
}

#[derive(Component, Clone, Default)]
pub struct PreviousAnimationPlayer(AnimationPlayer);

impl Deref for PreviousAnimationPlayer {
    type Target = AnimationPlayer;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Component)]
pub struct AnimationReactor(Vec<(AnimationEvent, async_oneshot::Sender<bool>)>);

#[derive(RefCast)]
#[repr(transparent)]
pub struct AsyncAnimationReactor(AsyncComponent<AnimationReactor>);

impl AsyncComponentDeref for AnimationReactor {
    type Target = AsyncAnimationReactor;

    fn async_deref(this: &AsyncComponent<Self>) -> &Self::Target {
        AsyncAnimationReactor::ref_cast(this)
    }
}

impl AnimationReactor {
    /// Wait for an [`AnimationEvent`] to happen.
    pub fn react_to(&mut self, event: AnimationEvent) -> impl Future<Output = bool> + 'static {
        let (send, recv) = async_oneshot::oneshot();
        self.push((event, send));
        recv.map(|v| v.unwrap_or(false))
    }
}

impl AsyncAnimationReactor {
    /// Wait for an [`AnimationEvent`] to happen.
    pub async fn react_to(&self, event: AnimationEvent) -> AccessResult<bool> {
        Ok(self.0.set(|x| x.react_to(event))?.await)
    }
}

impl Deref for AnimationReactor {
    type Target = Vec<(AnimationEvent, async_oneshot::Sender<bool>)>;

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
            Option<&mut AnimationReactor>,
        ),
        Changed<AnimationPlayer>,
    >,
) {
    for (player, mut prev, reactors) in query.iter_mut() {
        if let Some(mut reactors) = reactors {
            reactors.0.retain_mut(|(event, channel)| {
                let yields = match event {
                    AnimationEvent::OnExit(idx) => {
                        (!player.is_playing_animation(*idx)).then_some(true)
                    }
                    AnimationEvent::OnEnter(idx) => {
                        player.is_playing_animation(*idx).then_some(true)
                    }
                    AnimationEvent::WaitUnitOnExit(idx) => (player.is_playing_animation(*idx)
                        && !prev.0.is_playing_animation(*idx))
                    .then_some(true),
                    AnimationEvent::OnFrame { animation, frame } => {
                        if let Some(prev) = prev.0.animation(*animation) {
                            if let Some(curr) = player.animation(*animation) {
                                if curr.seek_time() >= *frame && *frame > prev.seek_time() {
                                    Some(true)
                                } else {
                                    None
                                }
                            } else {
                                Some(false)
                            }
                        } else {
                            None
                        }
                    }
                };
                match yields {
                    Some(v) => {
                        let _ = channel.send(v);
                        false
                    }
                    None => true,
                }
            });
        }
        prev.0.clone_from(player);
    }
}

/// Changes to [`AnimationTransitions`]'s main animation.
pub type MainAnimationChange = Change<Option<AnimationNodeIndex>>;

// /// Reactor to [`AnimationClip`] in [`AnimationPlayer`] changed as [`AnimationChange`].
pub fn react_to_main_animation_change(
    mut cache: Local<EntityHashMap<Option<AnimationNodeIndex>>>,
    query: Query<
        (
            Entity,
            &AnimationTransitions,
            SignalSender<MainAnimationChange>,
        ),
        (With<Signals>, Changed<AnimationTransitions>),
    >,
) {
    for (entity, transition, sender) in query.iter() {
        let prev = cache.get(&entity).copied();
        if prev.flatten() != transition.get_main_animation() {
            cache.insert(entity, transition.get_main_animation());
            sender.send(Change {
                from: prev,
                to: transition.get_main_animation(),
            });
        }
    }
}
