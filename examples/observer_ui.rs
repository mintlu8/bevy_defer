#![allow(clippy::type_complexity)]
use bevy::prelude::*;
use bevy::tasks::futures_lite::StreamExt;
use bevy::ui::RelativeCursorPosition;
use bevy_defer::observer::{AsyncTrigger, AsyncTriggerExt};
use bevy_defer::AsyncPlugin;
use bevy_defer::{fetch, AsyncAccess, AsyncEntityCommandsExtension};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AsyncPlugin::default_settings())
        .add_systems(Startup, setup)
        .add_systems(Update, button_system)
        .run();
}

const NORMAL_BUTTON: Color = Color::srgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.25, 0.25, 0.25);
const PRESSED_BUTTON: Color = Color::srgb(0.35, 0.75, 0.35);

/// from the original
fn button_system(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, mut color, mut border_color) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                *color = PRESSED_BUTTON.into();
                border_color.0 = Color::srgb(1., 0., 0.);
            }
            Interaction::Hovered => {
                *color = HOVERED_BUTTON.into();
                border_color.0 = Color::WHITE;
            }
            Interaction::None => {
                *color = NORMAL_BUTTON.into();
                border_color.0 = Color::BLACK;
            }
        }
    }
}

fn setup(mut commands: Commands) {
    // ui camera
    commands.spawn(Camera2d);
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|parent| {
            let btn_entity = parent
                .spawn((
                    Node {
                        width: Val::Px(150.0),
                        height: Val::Px(45.0),
                        border: UiRect::all(Val::Px(5.0)),
                        // horizontally center child text
                        justify_content: JustifyContent::Center,
                        // vertically center child text
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    Button,
                    BorderColor(Color::BLACK),
                    ImageNode {
                        color: NORMAL_BUTTON,
                        ..Default::default()
                    },
                    RelativeCursorPosition::default(),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Button"),
                        TextFont {
                            font: Default::default(),
                            font_size: 20.0,
                            ..Default::default()
                        },
                        Pickable {
                            should_block_lower: false,
                            is_hoverable: false,
                        },
                    ));
                })
                .id();
            parent
                .spawn((
                    Text::new("Receiving"),
                    TextFont {
                        font: Default::default(),
                        font_size: 20.0,
                        ..Default::default()
                    },
                    TextColor(Color::srgb(0.9, 0.9, 0.9)),
                ))
                .spawn_task(move |btn| async move {
                    loop {
                        let (click, _) = Trigger::<Pointer<Click>>::entity(btn_entity).await;
                        let s =
                            format!("Clicked at {}", click.hit.position.unwrap_or_default().xz());
                        fetch!(btn, Text).get_mut(move |text| text.0 = s).unwrap();
                    }
                })
                .spawn_task(move |btn| async move {
                    loop {
                        let (pressed, _) = Trigger::<Pointer<Pressed>>::entity(btn_entity).await;
                        let s = format!(
                            "Mouse down at {}",
                            pressed.hit.position.unwrap_or_default().xz()
                        );
                        fetch!(btn, Text).get_mut(move |text| text.0 = s).unwrap();
                    }
                })
                .spawn_task(move |btn| async move {
                    loop {
                        let (over, _) = Trigger::<Pointer<Over>>::entity(btn_entity).await;
                        let s = format!(
                            "Hover entered at {}",
                            over.hit.position.unwrap_or_default().xz()
                        );
                        fetch!(btn, Text).get_mut(move |text| text.0 = s).unwrap();
                    }
                })
                .spawn_task(move |btn| async move {
                    loop {
                        let (out, _) = Trigger::<Pointer<Out>>::entity(btn_entity).await;
                        let s = format!(
                            "Hover exited at {}",
                            out.hit.position.unwrap_or_default().xz()
                        );
                        fetch!(btn, Text).get_mut(move |text| text.0 = s).unwrap();
                    }
                });
        });
}
