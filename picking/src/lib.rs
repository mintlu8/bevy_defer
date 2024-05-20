#![allow(clippy::type_complexity)]
//! [`bevy_defer`] reactors for [`bevy_mod_picking`].

use bevy_defer::ext::picking::{ClickCancelled, Clicked, LostFocus, ObtainedFocus, Pressed};
use bevy_defer::signals::Signals;
use bevy_defer::{reactors::Change, signal_ids};
use bevy_ecs::{
    entity::Entity,
    query::{Changed, With},
    system::{Local, Query},
};
use bevy_math::Vec2;
use bevy_mod_picking::{
    focus::PickingInteraction, pointer::PointerLocation, selection::PickSelection,
};
use rustc_hash::FxHashMap;

signal_ids! {
    /// [`PickSelection`] changed.
    pub PickingSelected: bool,
}

/// State machine [`PickingInteraction`] changed to a different value.
pub type PickingInteractionChange = Change<PickingInteraction>;

/// System that provides reactivity for [`bevy_mod_picking`], must be added manually.
///
/// This also acts as `react_to_component_change` for [`PickingInteraction`].
pub fn react_to_picking(
    mut prev: Local<FxHashMap<Entity, PickingInteraction>>,
    mut prev_select: Local<FxHashMap<Entity, bool>>,
    interactions: Query<
        (
            Entity,
            &Signals,
            &PickingInteraction,
            Option<&PointerLocation>,
        ),
        (Changed<PickingInteraction>, With<Signals>),
    >,
    selections: Query<(Entity, &Signals, &PickSelection), (Changed<PickSelection>, With<Signals>)>,
) {
    for (entity, signals, interaction, relative) in interactions.iter() {
        let previous = prev.insert(entity, *interaction);
        if Some(*interaction) == previous {
            continue;
        }
        let position = relative
            .and_then(|x| x.location().map(|x| x.position))
            .unwrap_or(Vec2::ZERO);
        signals.send::<PickingInteractionChange>(Change {
            from: previous,
            to: *interaction,
        });
        if interaction == &PickingInteraction::Pressed {
            signals.send::<Pressed>(position);
        }
        let previous = previous.unwrap_or(PickingInteraction::None);
        match (previous, interaction) {
            (PickingInteraction::Pressed, PickingInteraction::Hovered) => {
                signals.send::<Clicked>(position)
            }
            (PickingInteraction::Pressed, PickingInteraction::None) => {
                signals.send::<ClickCancelled>(position)
            }
            _ => false,
        };
        if previous == PickingInteraction::None {
            signals.send::<ObtainedFocus>(position);
        }
        if interaction == &PickingInteraction::None {
            signals.send::<LostFocus>(position);
        }
    }

    for (entity, signals, selection) in selections.iter() {
        let selection = selection.is_selected;
        let previous = prev_select.insert(entity, selection).unwrap_or(false);
        if selection == previous {
            continue;
        }
        signals.send::<PickingSelected>(selection);
    }
}
