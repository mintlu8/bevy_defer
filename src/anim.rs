use std::time::Duration;

use bevy_animation::{AnimationClip, AnimationPlayer, RepeatAnimation};
use bevy_asset::Handle;
use ref_cast::RefCast;

use crate::{AsyncComponent, AsyncComponentDeref, AsyncResult};

#[derive(RefCast)]
#[repr(transparent)]
pub struct AsyncAnimationPlayer<'t>(AsyncComponent<'t, AnimationPlayer>);

impl AsyncComponentDeref for AnimationPlayer {
    type Target<'t> = AsyncAnimationPlayer<'t>;

    fn async_deref<'a, 'b>(this: &'b crate::AsyncComponent<'a, Self>) -> &'b Self::Target<'a> {
        AsyncAnimationPlayer::ref_cast(this)
    }
}

impl AsyncAnimationPlayer<'_> {
    pub async fn play(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        self.0.set(move |player| {player.play(clip);}).await
    }

    pub async fn play_with_transition(&self, clip: Handle<AnimationClip>, duration: Duration) -> AsyncResult {
        self.0.set(move |player| {player.play_with_transition(clip, duration);}).await
    }

    pub async fn animate(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        futures::try_join!(
            self.play(clip.clone()),
            self.when_exit(clip)
        ).map(|_|())
    }


    pub async fn animate_with_transition(&self, clip: Handle<AnimationClip>, duration: Duration) -> AsyncResult {
        futures::try_join!(
            self.play_with_transition(clip.clone(), duration),
            self.when_exit(clip)
        ).map(|_|())
    }

    pub async fn set_repeat(&self, f: impl FnOnce(RepeatAnimation) -> RepeatAnimation + Send + Sync + 'static) -> AsyncResult {
        self.0.set(move |player| {
            player.set_repeat(f(player.repeat_mode()));
        }).await
    }

    pub async fn set_speed(&self, f: impl FnOnce(f32) -> f32 + Send + Sync + 'static) -> AsyncResult {
        self.0.set(move |player| {
            player.set_speed(f(player.speed()));
        }).await
    }

    pub async fn seek_to(&self, f: impl FnOnce(f32) -> f32 + Send + Sync + 'static) -> AsyncResult {
        self.0.set(move |player| {
            player.seek_to(f(player.seek_time()));
        }).await
    }

    pub async fn pause(&self) -> AsyncResult {
        self.0.set(move |player| {player.pause();}).await
    }

    pub async fn resume(&self) -> AsyncResult {
        self.0.set(move |player| {player.resume();}).await
    }

    pub async fn when_exit(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        self.0.watch(move |player| {
            (player.animation_clip() != &clip || player.is_finished())
                    .then_some(())
        }).await?;
        Ok(())
    }

    pub async fn when_enter(&self, clip: Handle<AnimationClip>) -> AsyncResult {
        self.0.watch(move |player| {
            (player.animation_clip() == &clip).then_some(())
        }).await?;
        Ok(())
    }
}