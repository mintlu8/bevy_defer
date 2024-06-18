#![allow(clippy::type_complexity)]
use bevy::prelude::*;
use bevy_defer::ext::picking::{
    ClickCancelled, Clicked, LostFocus, ObtainedFocus, Pressed, UIInteractionChange,
};
use bevy_defer::reactors::StateMachine;
use bevy_defer::signals::Signal;
use bevy_defer::AsyncAccess;
use bevy_defer::{async_system, async_systems::AsyncSystems, signals::Signals, AsyncPlugin};
use bevy_tasks::futures_lite::StreamExt;
use bevy_ui::RelativeCursorPosition;

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
    commands.spawn(Camera2dBundle::default());
    let click = Signal::default();
    let press = Signal::default();
    let focus = Signal::default();
    let lose = Signal::default();
    let cancel = Signal::default();
    let state = Signal::default();
    let mut btn_entity = Entity::PLACEHOLDER;
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            btn_entity = parent
                .spawn((
                    ButtonBundle {
                        style: Style {
                            width: Val::Px(150.0),
                            height: Val::Px(65.0),
                            border: UiRect::all(Val::Px(5.0)),
                            // horizontally center child text
                            justify_content: JustifyContent::Center,
                            // vertically center child text
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        border_color: BorderColor(Color::BLACK),
                        image: UiImage {
                            color: NORMAL_BUTTON,
                            ..Default::default()
                        },
                        ..default()
                    },
                    RelativeCursorPosition::default(),
                    Signals::new()
                        .with_sender::<Clicked>(click.clone())
                        .with_sender::<Pressed>(press.clone())
                        .with_sender::<ObtainedFocus>(focus.clone())
                        .with_sender::<LostFocus>(lose.clone())
                        .with_sender::<ClickCancelled>(cancel.clone())
                        .with_sender::<UIInteractionChange>(state.clone()),
                    AsyncSystems::from_single(async_system!(
                        |click: Sender<Clicked>,
                         press: Sender<Pressed>,
                         focus: Sender<ObtainedFocus>,
                         lose: Sender<LostFocus>,
                         cancel: Sender<ClickCancelled>| {
                            futures::select_biased! {
                                pos = click.recv() => println!("Clicked at {pos}"),
                                pos = press.recv() => println!("Pressed at {pos}"),
                                pos = cancel.recv() => println!("Click cancelled at {pos}"),
                                pos = focus.recv() => println!("Focus obtained at {pos}"),
                                pos = lose.recv() => println!("Focus lost at {pos}"),
                            }
                        }
                    )),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        TextBundle::from_section(
                            "Button",
                            TextStyle {
                                font: Default::default(),
                                font_size: 40.0,
                                color: Color::srgb(0.9, 0.9, 0.9),
                            },
                        ),
                        Signals::from_receiver::<UIInteractionChange>(state),
                        AsyncSystems::from_single(async_system!(
                            |click: StateMachine<Interaction>, this: AsyncComponent<Text>| {
                                let mut stream = click.into_stream();
                                while let Some(item) = stream.next().await {
                                    let variant = format!("{:?}", item.to);
                                    this.set(move |text| text.sections[0].value = variant)
                                        .unwrap();
                                }
                            }
                        )),
                    ));
                })
                .id();
            parent.spawn((
                TextBundle::from_section(
                    "Receiving",
                    TextStyle {
                        font: Default::default(),
                        font_size: 40.0,
                        color: Color::srgb(0.9, 0.9, 0.9),
                    },
                ),
                Signals::new()
                    .with_receiver::<Clicked>(click.clone())
                    .with_receiver::<Pressed>(press.clone())
                    .with_receiver::<ObtainedFocus>(focus.clone())
                    .with_receiver::<LostFocus>(lose.clone())
                    .with_receiver::<ClickCancelled>(cancel.clone()),
                AsyncSystems::from_single(async_system!(
                    |click: Receiver<Clicked>,
                     press: Receiver<Pressed>,
                     focus: Receiver<ObtainedFocus>,
                     lose: Receiver<LostFocus>,
                     cancel: Receiver<ClickCancelled>,
                     this: AsyncComponent<Text>| {
                        futures::select_biased! {
                            pos = click.recv() => {
                                let s = format!("Clicked at {pos}");
                                this.set(move |text| text.sections[0].value = s).unwrap();
                            },
                            pos = press.recv() => {
                                let s = format!("Pressed at {pos}");
                                this.set(move |text| text.sections[0].value = s).unwrap();
                            },
                            pos = focus.recv() => {
                                let s = format!("Obtained focus at {pos}");
                                this.set(move |text| text.sections[0].value = s).unwrap();
                            },
                            pos = lose.recv() => {
                                let s = format!("Lose focus at {pos}");
                                this.set(move |text| text.sections[0].value = s).unwrap();
                            },
                            pos = cancel.recv() => {
                                let s = format!("Click cancelled at {pos}");
                                this.set(move |text| text.sections[0].value = s).unwrap();
                            },
                        }
                    }
                )),
            ));
        });
}
