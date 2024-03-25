//! Reactors for `bevy_mod_picking`.

use crate::signal_ids;
use bevy_math::Vec2;
use crate::signals::Signals;
use bevy_ecs::{entity::Entity, system::{Query, Local}, query::Changed};
use rustc_hash::FxHashMap;
use bevy_mod_picking::{focus::PickingInteraction, pointer::PointerLocation};

signal_ids! {
    /// [`Interaction`](bevy_ui::Interaction) changed in general.
    /// 
    /// Sends previous and current interaction.
    pub PickingInteractionChange: (PickingInteraction, PickingInteraction)
}

/// System that provides reactivity for [`bevy_ui`], must be added manually.
pub fn picking_reactor(
    mut prev: Local<FxHashMap<Entity, PickingInteraction>>,
    query: Query<(Entity, &Signals, &PickingInteraction, Option<&PointerLocation>), Changed<bevy_ui::Interaction>>
) {
    use crate::picking::{Click, ClickCancelled, LoseFocus, ObtainFocus, Pressed};

    for (entity, signals, interaction, relative) in query.iter() {
        let previous = prev.insert(entity, *interaction).unwrap_or(PickingInteraction::None);
        let position = relative.and_then(|x| x.location().map(|x| x.position)).unwrap_or(Vec2::ZERO);
        signals.send::<PickingInteractionChange>((previous, *interaction));
        if interaction == &PickingInteraction::Pressed {
            signals.send::<Pressed>(position);
        }
        match (previous, interaction) {
            (PickingInteraction::Pressed, PickingInteraction::Hovered) => signals.send::<Click>(position),
            (PickingInteraction::Pressed, PickingInteraction::None) => signals.send::<ClickCancelled>(position),
            _ => (),
        }
        if previous == PickingInteraction::None {
            signals.send::<ObtainFocus>(position);
        }
        if interaction == &PickingInteraction::None {
            signals.send::<LoseFocus>(position);
        }
    }
}

mod sealed {
    use bevy_math::Vec2;
    use bevy_ecs::query::{QueryData, QueryFilter};
    use bevy_mod_picking::focus::PickingInteraction;
    use bevy_mod_picking::pointer::PointerLocation;
    use ref_cast::RefCast;

    use crate::access::AsyncEntityQuery;
    use crate::extensions::AsyncEntityQueryDeref;
    /// [`QueryData`] for asynchronously accessing a UI button's state.
    #[derive(Debug, QueryData)]
    pub struct AsyncPicking {
        interaction: &'static PickingInteraction,
        cursor: Option<&'static PointerLocation>,
    }

    impl AsyncPickingItem<'_> {
        fn get_cursor(&self) -> Vec2 {
            self.cursor.and_then(|x| x.location().map(|x| x.position)).unwrap_or(Vec2::ZERO)
        }
    }

    #[derive(RefCast)]
    #[repr(transparent)]
    pub struct AsyncPickingExt<F: QueryFilter>(AsyncEntityQuery<AsyncPicking, F>);

    impl<F: QueryFilter + 'static> AsyncPickingExt<F> {
        /// returns when `pressed` -> `hovered`.
        pub async fn clicked(&self) -> Vec2 {
            let mut last = PickingInteraction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction == &PickingInteraction::Hovered && last == PickingInteraction::Pressed;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }

        /// returns when `pressed` -> `none`.
        pub async fn cancelled(&self) -> Vec2 {
            let mut last = PickingInteraction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction == &PickingInteraction::None && last == PickingInteraction::Pressed;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }

        /// returns when `!pressed` -> `pressed`.
        pub async fn pressed(&self) -> Vec2 {
            let mut last = PickingInteraction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction == &PickingInteraction::Pressed && last != PickingInteraction::Pressed;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }

        /// returns when `!none`.
        pub async fn focused(&self) -> Vec2 {
            let mut last = PickingInteraction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction != &PickingInteraction::None && last == PickingInteraction::None;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }

        /// returns when `none`.
        pub async fn lose_focus(&self) -> Vec2 {
            let mut last = PickingInteraction::Hovered;
            self.0.watch(move |ui| {
                let result = ui.interaction == &PickingInteraction::None && last != PickingInteraction::None;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await
        }
    }

    impl AsyncEntityQueryDeref for AsyncPicking {
        type Target<F: QueryFilter> = AsyncPickingExt<F>;
    
        fn async_deref<F: bevy_ecs::query::QueryFilter>(this: &AsyncEntityQuery<Self, F>) -> &Self::Target<F> {
            AsyncPickingExt::ref_cast(this)
        }
    }
}

#[cfg(feature = "bevy_ui")]
pub use sealed::AsyncPicking;