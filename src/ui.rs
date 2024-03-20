use crate::signal_ids;
use bevy_math::Vec2;
use crate::signals::Signals;
use bevy_ecs::{entity::Entity, system::{Query, Local}, query::Changed};
use rustc_hash::FxHashMap;

signal_ids! {
    /// [`Interaction`](bevy_ui::Interaction) from any to `Pressed`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition).
    pub UIPressed: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Pressed` to `Hovered`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition).
    pub UIClick: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `None` to `Hovered|Pressed`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition).
    pub UIObtainFocus: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Hovered|Pressed` to `None`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition).
    pub UILoseFocus: Vec2,
    /// [`Interaction`](bevy_ui::Interaction) from `Pressed` to `None`.
    /// 
    /// Sends [`RelativeCursorPosition`](bevy_ui::RelativeCursorPosition).
    pub UIClickCancelled: Vec2,
}

#[cfg(feature = "bevy_ui")]
signal_ids! {
    /// [`Interaction`](bevy_ui::Interaction) changed in general.
    /// 
    /// Sends previous and current interaction.
    pub UIInteractionChange: (bevy_ui::Interaction, bevy_ui::Interaction)
}
/// System that provides reactivity for [`bevy_ui`], must be added manually.
#[cfg(feature = "bevy_ui")]
pub fn ui_reactor(
    mut prev: Local<FxHashMap<Entity, bevy_ui::Interaction>>,
    query: Query<(Entity, &Signals, &bevy_ui::Interaction, Option<&bevy_ui::RelativeCursorPosition>), Changed<bevy_ui::Interaction>>
) {
    for (entity, signals, interaction, relative) in query.iter() {
        let previous = prev.insert(entity, *interaction).unwrap_or(bevy_ui::Interaction::None);
        let position = relative.and_then(|x| x.normalized).unwrap_or(Vec2::ZERO);
        signals.send::<UIInteractionChange>((previous, *interaction));
        use bevy_ui::Interaction;
        if interaction == &Interaction::Pressed {
            signals.send::<UIPressed>(position);
        }
        match (previous, interaction) {
            (Interaction::Pressed, Interaction::Hovered) => signals.send::<UIClick>(position),
            (Interaction::Pressed, Interaction::None) => signals.send::<UIClickCancelled>(position),
            _ => (),
        }
        if previous == Interaction::None {
            signals.send::<UIObtainFocus>(position);
        }
        if interaction == &Interaction::None {
            signals.send::<UILoseFocus>(position);
        }
    }
}

#[cfg(feature = "bevy_ui")]
mod sealed {
    use bevy_math::Vec2;
    use bevy_ecs::query::{QueryData, QueryFilter};
    use bevy_ui::{Interaction, RelativeCursorPosition};
    use ref_cast::RefCast;

    use crate::{AsyncEntityQuery, AsyncEntityQueryDeref};

    /// [`QueryData`] for asynchronously accessing a UI button's state.
    #[derive(Debug, QueryData)]
    pub struct AsyncUIButton {
        interaction: &'static Interaction,
        cursor: Option<&'static RelativeCursorPosition>,
    }

    impl AsyncUIButtonItem<'_> {
        fn get_cursor(&self) -> Vec2 {
            self.cursor.and_then(|x| x.normalized).unwrap_or(Vec2::ZERO)
        }
    }

    #[derive(RefCast)]
    #[repr(transparent)]
    pub struct AsyncUIExt<F: QueryFilter>(AsyncEntityQuery<AsyncUIButton, F>);

    impl<F: QueryFilter + 'static> AsyncUIExt<F> {
        /// returns when `pressed` -> `hovered`.
        pub async fn clicked(&self) -> Vec2 {
            let mut last = Interaction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction == &Interaction::Hovered && last == Interaction::Pressed;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }

        /// returns when `pressed` -> `none`.
        pub async fn cancelled(&self) -> Vec2 {
            let mut last = Interaction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction == &Interaction::None && last == Interaction::Pressed;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }

        /// returns when `!pressed` -> `pressed`.
        pub async fn pressed(&self) -> Vec2 {
            let mut last = Interaction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction == &Interaction::Pressed && last != Interaction::Pressed;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }

        /// returns when `!none`.
        pub async fn focused(&self) -> Vec2 {
            let mut last = Interaction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction != &Interaction::None && last == Interaction::None;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }

        /// returns when `none`.
        pub async fn lose_focus(&self) -> Vec2 {
            let mut last = Interaction::Hovered;
            self.0.watch(move |ui| {
                let result = ui.interaction == &Interaction::None && last != Interaction::None;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }
    }

    impl AsyncEntityQueryDeref for AsyncUIButton {
        type Target<F: QueryFilter> = AsyncUIExt<F>;
    
        fn async_deref<F: bevy_ecs::query::QueryFilter>(this: &AsyncEntityQuery<Self, F>) -> &Self::Target<F> {
            AsyncUIExt::ref_cast(this)
        }
    }
}

#[cfg(feature = "bevy_ui")]
pub use sealed::AsyncUIButton;