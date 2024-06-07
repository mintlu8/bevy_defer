use async_oneshot::Sender;
use bevy_animation::prelude::{AnimationNodeIndex, AnimationTransitions};
use bevy_animation::{AnimationClip, AnimationPlayer, RepeatAnimation};
use bevy_asset::Handle;
use bevy_ecs::query;
use bevy_ecs::system::{Res, ResMut, Resource};
use bevy_ecs::{
    entity::Entity,
    query::{Changed, With},
    system::{Local, Query},
};
use futures::{Future, FutureExt};
use ref_cast::RefCast;
use rustc_hash::FxHashMap;

use crate::signals::{SignalSender, Signals};
use crate::tween::AsSeconds;
use crate::{AccessError, AsyncWorld, OwnedQueryState};
use crate::{
    access::{deref::AsyncComponentDeref, AsyncComponent},
    reactors::Change,
    AccessResult, AsyncAccess,
};

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
        self.0.get(move |player| {
            player.is_playing_animation(clip)
        })
    }

    pub fn wait(&self, event: AnimationEvent) -> impl Future<Output = ()> + 'static {
        let (send, recv) = async_oneshot::oneshot();
        let _ = AsyncWorld.resource::<AnimationReactors>()
            .set(|x| x.reactors.entry(self.0.entity()).or_default().push((event, send)));
        recv.map(|_| ())
    }
}

impl AsyncAnimationTransitions {
    pub fn play(&self, animation: AnimationNodeIndex, transition: impl AsSeconds) {
        AsyncWorld.run(|w| {
            let mut query = OwnedQueryState::<(&mut AnimationPlayer, &mut AnimationTransitions), ()>::new(w);
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
    /// Yields when last animation is the one specified 
    /// and current animation is not the one specified.
    WaitUnitOnExit(AnimationNodeIndex),
}

#[derive(Debug, Resource)]
pub struct AnimationReactors {
    reactors: FxHashMap<Entity, Vec<(AnimationEvent, Sender<()>)>>
}


/// `SignalId` and content for playing [`AnimationClip`] changed.
pub type AnimationChange = Change<Handle<AnimationClip>>;

fn map_op<A, B>(item: Option<&(A, B)>) -> (Option<&A>, Option<&B>) {
    match item {
        Some((a, b)) => (Some(&a), Some(&b)),
        None => (None, None),
    }
}

// /// Reactor to [`AnimationClip`] in [`AnimationPlayer`] changed as [`AnimationChange`].
// pub fn react_to_animation(
//     reactors: Option<ResMut<AnimationReactors>>,
//     prev: Local<FxHashMap<(Entity, AnimationNodeIndex), (AnimationNodeIndex, f32)>>,
//     query: Query<(Entity, &AnimationPlayer, &AnimationTransitions)>
// ) {
//     let Some(mut reactors) = reactors else {return};
//     for (entity, player, transition) in query.iter() {
//         let (last_node, last_frame) = map_op(prev.get(&entity));
//         if let Some(reactors) = reactors.reactors.get_mut(&entity) {
//             reactors.retain_mut(|(event, sender)| {
//                 let yield_this = match event {
//                     AnimationEvent::OnExit(idx) => {
//                         !player.is_playing_animation(*idx)
//                     },
//                     AnimationEvent::OnEnter(idx) => {
//                         player.is_playing_animation(*idx)
//                     },
//                     AnimationEvent::WaitUnitOnExit(idx) => {
//                         last_node == Some(idx) && 
//                             !player.is_playing_animation(*idx)
//                     },
//                 };
//                 if yield_this {
//                     let _  = sender.send(());
//                 }
//                 !yield_this
//             })
//         } else {
//             continue;
//         }
//         prev.insert(entity, ())
//     }
// }
