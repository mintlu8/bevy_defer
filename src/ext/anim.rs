use bevy_animation::{AnimationClip, AnimationPlayer, RepeatAnimation};
use bevy_asset::Handle;
use bevy_ecs::{entity::Entity, query::{Changed, With}, system::{Local, Query}};
use ref_cast::RefCast;
use rustc_hash::FxHashMap;

use crate::{access::{deref::AsyncComponentDeref, AsyncComponent}, reactors::Change, tween::AsSeconds, AsyncAccess, AsyncResult};
use crate::signals::{SignalSender, Signals};

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

impl AsyncAnimationPlayer {
    /// Start playing an animation, resetting state of the player, unless the requested animation is already playing.
    pub async fn play(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        self.0.set(move |player| {player.play(clip);}).await
    }

    /// Start playing an animation, and set repeat mode to [`RepeatAnimation::Never`].
    pub async fn play_once(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        self.0.set(move |player| {
            player.play(clip);
            player.set_repeat(RepeatAnimation::Never);
        }).await
    }

    /// Start playing an animation, and set repeat mode to [`RepeatAnimation::Forever`].
    pub async fn play_repeat(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        self.0.set(move |player| {
            player.play(clip);
            player.repeat();
        }).await
    }

    /// Start playing an animation with smooth linear transition.
    pub async fn play_with_transition(&self, clip: Handle<AnimationClip>, duration: impl AsSeconds) -> AsyncResult {
        let duration = duration.as_duration();
        self.0.set(move |player| {player.play_with_transition(clip, duration);}).await
    }

    /// Start playing an animation with smooth linear transition and set repeat mode to [`RepeatAnimation::Never`].
    pub async fn play_once_with_transition(&self, clip: Handle<AnimationClip>, duration: impl AsSeconds) -> AsyncResult {
        let duration = duration.as_duration();
        self.0.set(move |player| {
            player.play_with_transition(clip, duration);
            player.set_repeat(RepeatAnimation::Never);
        }).await
    }

    /// Start playing an animation with smooth linear transition and set repeat mode to [`RepeatAnimation::Forever`].
    pub async fn play_repeat_with_transition(&self, clip: Handle<AnimationClip>, duration: impl AsSeconds) -> AsyncResult {
        let duration = duration.as_duration();
        self.0.set(move |player| {
            player.play_with_transition(clip, duration);
            player.repeat();
        }).await
    }

    /// Start playing an animation once and wait for it to complete.
    pub async fn animate(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        futures::future::try_join(
            self.play_once(clip.clone()),
            self.when_exit(clip)
        ).await.map(|_| ())
    }

    /// Start playing an animation once with smooth linear transition and wait for it to complete.
    pub async fn animate_with_transition(&self, clip: Handle<AnimationClip>, duration: impl AsSeconds) -> AsyncResult {
        futures::future::try_join(
            self.play_once_with_transition(clip.clone(), duration),
            self.when_exit(clip)
        ).await.map(|_| ())
    }

    /// Set the repetition behaviour of the animation.
    pub async fn set_repeat(&self, f: impl FnOnce(RepeatAnimation) -> RepeatAnimation + Send + 'static) -> AsyncResult {
        self.0.set(move |player| {
            player.set_repeat(f(player.repeat_mode()));
        }).await
    }

    /// Set the speed of the animation playback
    pub async fn set_speed(&self, f: impl FnOnce(f32) -> f32 + Send + 'static) -> AsyncResult {
        self.0.set(move |player| {
            player.set_speed(f(player.speed()));
        }).await
    }

    /// Seek to a specific time in the animation.
    pub async fn seek_to(&self, f: impl FnOnce(f32) -> f32 + Send + 'static) -> AsyncResult {
        self.0.set(move |player| {
            player.seek_to(f(player.seek_time()));
        }).await
    }

    /// Pause the animation
    pub async fn pause(&self) -> AsyncResult {
        self.0.set(move |player| {player.pause();}).await
    }

    /// Unpause the animation
    pub async fn resume(&self) -> AsyncResult {
        self.0.set(move |player| {player.resume();}).await
    }

    /// Wait for an [`AnimationClip`] to exit.
    pub async fn when_exit(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        self.0.watch(move |player| {
            (player.animation_clip() != &clip || player.is_finished())
                    .then_some(())
        }).await?;
        Ok(())
    }

    /// Wait for an [`AnimationClip`] to be entered.
    pub async fn when_enter(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        self.0.watch(move |player| {
            (player.animation_clip() == &clip).then_some(())
        }).await?;
        Ok(())
    }
}

/// [`SignalId`] and content for playing [`AnimationClip`] changed.
pub type AnimationChange = Change<Handle<AnimationClip>>;


/// Reactor to [`AnimationClip`] in [`AnimationPlayer`] changed as [`AnimationChange`]. 
/// 
/// Currently unused by [`AsyncAnimationPlayer`].
pub fn react_to_animation(
    mut previous: Local<FxHashMap<Entity, Handle<AnimationClip>>>,
    query: Query<(Entity, &AnimationPlayer, SignalSender<AnimationChange>), (Changed<AnimationPlayer>, With<Signals>)>
) {
    for (entity, player, sender) in query.iter(){
        let last = previous.get(&entity);
        if last != Some(player.animation_clip()){ 
            let change = AnimationChange {
                from: last.map(|x| x.clone_weak()).unwrap_or_default(),
                to: player.animation_clip().clone_weak(),
            };
            previous.insert(entity, player.animation_clip().clone_weak());
            sender.send(change);
        }
    }
}