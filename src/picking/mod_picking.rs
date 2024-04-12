//! Reactors for `bevy_mod_picking`.

use crate::{reactors::Change, signal_ids};
use bevy_math::Vec2;
use crate::signals::Signals;
use bevy_ecs::{entity::Entity, query::{Changed, With}, system::{Local, Query}};
use rustc_hash::FxHashMap;
use bevy_mod_picking::{focus::PickingInteraction, pointer::PointerLocation, selection::PickSelection};
use crate::picking::{Click, ClickCancelled, LoseFocus, ObtainFocus, Pressed};

signal_ids! {
    /// [`PickSelection`] changed.
    pub PickingSelected: bool,
}

/// State machine [`PickingInteraction`] changed to a different value.
pub type PickingInteractionChange = Change<PickingInteraction>;

/// System that provides reactivity for [`bevy_mod_picking`], must be added manually.
/// 
/// This also acts as `react_to_state_machine` for [`PickingInteraction`].
pub fn picking_reactor(
    mut prev: Local<FxHashMap<Entity, PickingInteraction>>,
    mut prev_select: Local<FxHashMap<Entity, bool>>,
    interactions: Query<(Entity, &Signals, &PickingInteraction, Option<&PointerLocation>), (Changed<PickingInteraction>, With<Signals>)>,
    selections: Query<(Entity, &Signals, &PickSelection), (Changed<PickSelection>, With<Signals>)>
) {
    for (entity, signals, interaction, relative) in interactions.iter() {
        let previous = prev.insert(entity, *interaction).unwrap_or(PickingInteraction::None);
        if interaction == &previous { continue; }
        let position = relative.and_then(|x| x.location().map(|x| x.position)).unwrap_or(Vec2::ZERO);
        signals.send::<PickingInteractionChange>(Change {
            from: previous, 
            to: *interaction
        });
        if interaction == &PickingInteraction::Pressed {
            signals.send::<Pressed>(position);
        }
        match (previous, interaction) {
            (PickingInteraction::Pressed, PickingInteraction::Hovered) => signals.send::<Click>(position),
            (PickingInteraction::Pressed, PickingInteraction::None) => signals.send::<ClickCancelled>(position),
            _ => false,
        };
        if previous == PickingInteraction::None {
            signals.send::<ObtainFocus>(position);
        }
        if interaction == &PickingInteraction::None {
            signals.send::<LoseFocus>(position);
        }
    }

    for (entity, signals, selection) in selections.iter() {
        let selection = selection.is_selected;
        let previous = prev_select.insert(entity, selection).unwrap_or(false);
        if selection == previous { continue; }
        signals.send::<PickingSelected>(selection);
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
    use crate::AsyncAccess;
    /// [`QueryData`] for asynchronously accessing a `bevy_mod_picking` pickable's state.
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
            }).await.unwrap_or_default()
        }

        /// returns when `pressed` -> `none`.
        pub async fn cancelled(&self) -> Vec2 {
            let mut last = PickingInteraction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction == &PickingInteraction::None && last == PickingInteraction::Pressed;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await.unwrap_or_default()
        }

        /// returns when `!pressed` -> `pressed`.
        pub async fn pressed(&self) -> Vec2 {
            let mut last = PickingInteraction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction == &PickingInteraction::Pressed && last != PickingInteraction::Pressed;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await.unwrap_or_default()
        }

        /// returns when `!none`.
        pub async fn focused(&self) -> Vec2 {
            let mut last = PickingInteraction::None;
            self.0.watch(move |ui| {
                let result = ui.interaction != &PickingInteraction::None && last == PickingInteraction::None;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await.unwrap_or_default()
        }

        /// returns when `none`.
        pub async fn lose_focus(&self) -> Vec2 {
            let mut last = PickingInteraction::Hovered;
            self.0.watch(move |ui| {
                let result = ui.interaction == &PickingInteraction::None && last != PickingInteraction::None;
                last = *ui.interaction;
                result.then_some(ui.get_cursor())
            }).await.unwrap_or_default()
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