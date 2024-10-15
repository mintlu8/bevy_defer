//! Reactors for `bevy_ui`.

use crate::reactors::Change;
use crate::signals::Signals;
use bevy::ui::{Interaction, RelativeCursorPosition};
use bevy::ecs::{
    entity::Entity,
    query::{Changed, With},
    system::{Local, Query},
};
use bevy::math::Vec2;
use rustc_hash::FxHashMap;

/// State machine [`Interaction`] changed to a different value.
pub type UIInteractionChange = Change<Interaction>;

/// System that provides reactivity for [`bevy_ui`].
///
/// This also acts as `react_to_component_change` for [`Interaction`].
pub fn react_to_ui(
    mut prev: Local<FxHashMap<Entity, Interaction>>,
    query: Query<
        (
            Entity,
            &Signals,
            &Interaction,
            Option<&RelativeCursorPosition>,
        ),
        (Changed<Interaction>, With<Signals>),
    >,
) {
    use super::{ClickCancelled, Clicked, LostFocus, ObtainedFocus, Pressed};

    for (entity, signals, interaction, relative) in query.iter() {
        let previous = prev.insert(entity, *interaction);
        if Some(*interaction) == previous {
            continue;
        }
        let position = relative.and_then(|x| x.normalized).unwrap_or(Vec2::ZERO);
        signals.send::<UIInteractionChange>(Change {
            from: previous,
            to: *interaction,
        });
        if interaction == &Interaction::Pressed {
            signals.send::<Pressed>(position);
        }
        let previous = previous.unwrap_or_default();
        match (previous, interaction) {
            (Interaction::Pressed, Interaction::Hovered) => signals.send::<Clicked>(position),
            (Interaction::Pressed, Interaction::None) => signals.send::<ClickCancelled>(position),
            _ => false,
        };
        if previous == Interaction::None {
            signals.send::<ObtainedFocus>(position);
        }
        if interaction == &Interaction::None {
            signals.send::<LostFocus>(position);
        }
    }
}
