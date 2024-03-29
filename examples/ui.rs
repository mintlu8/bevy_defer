use bevy::prelude::*;
use bevy_defer::signals::Signal;
use bevy_defer::{async_system, signals::Signals, world, AsyncCommandsExtension, async_systems::AsyncSystems, AsyncPlugin};
use bevy_defer::picking::{ui_reactor, AsyncUIButton, ClickCancelled, Click, UIInteractionChange, LoseFocus, ObtainFocus, Pressed};
use bevy_ui::RelativeCursorPosition;
use futures::FutureExt;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AsyncPlugin::default_settings())
        .add_systems(Startup, setup)
        .add_systems(Update, button_system)
        .add_systems(Update, ui_reactor)
        .run();
}

const NORMAL_BUTTON: Color = Color::rgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::rgb(0.25, 0.25, 0.25);
const PRESSED_BUTTON: Color = Color::rgb(0.35, 0.75, 0.35);

/// from the original 
fn button_system(
    mut interaction_query: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, mut color, mut border_color) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                *color = PRESSED_BUTTON.into();
                border_color.0 = Color::RED;
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
                        background_color: NORMAL_BUTTON.into(),
                        ..default()
                    },
                    RelativeCursorPosition::default(),
                    Signals::new()
                        .with_sender::<Click>(click.clone())
                        .with_sender::<Pressed>(press.clone())
                        .with_sender::<ObtainFocus>(focus.clone())
                        .with_sender::<LoseFocus>(lose.clone())
                        .with_sender::<ClickCancelled>(cancel.clone())
                        .with_sender::<UIInteractionChange>(state.clone()),
                    AsyncSystems::from_single(async_system!(
                        |click: Sender<Click>, press: Sender<Pressed>, focus: Sender<ObtainFocus>, lose: Sender<LoseFocus>, cancel: Sender<ClickCancelled>| {
                            futures::select_biased! {
                                pos = click.fuse() => println!("Clicked at {pos}"),
                                pos = press.fuse() => println!("Pressed at {pos}"),
                                pos = cancel.fuse() => println!("Click cancelled at {pos}"),
                                pos = focus.fuse() => println!("Focus obtained at {pos}"),
                                pos = lose.fuse() => println!("Focus lost at {pos}"),
                            }
                        } 
                    ))
                ))
                .with_children(|parent| {
                    parent.spawn((
                        TextBundle::from_section(
                            "Button",
                            TextStyle {
                                font: Default::default(),
                                font_size: 40.0,
                                color: Color::rgb(0.9, 0.9, 0.9),
                            },
                        ),
                        Signals::from_receiver::<UIInteractionChange>(state),
                        AsyncSystems::from_single(async_system!(
                            |click: Receiver<UIInteractionChange>, this: AsyncComponent<Text>| {
                                let variant = format!("{:?}", click.await.1);
                                this.set(move |text| text.sections[0].value = variant).await.unwrap();
                            } 
                        ))
                    ));
                }).id();
            parent.spawn((
                TextBundle::from_section(
                    "Receiving",
                    TextStyle {
                        font: Default::default(),
                        font_size: 40.0,
                        color: Color::rgb(0.9, 0.9, 0.9),
                    },
                ),
                Signals::new()
                    .with_receiver::<Click>(click.clone())
                    .with_receiver::<Pressed>(press.clone())
                    .with_receiver::<ObtainFocus>(focus.clone())
                    .with_receiver::<LoseFocus>(lose.clone())
                    .with_receiver::<ClickCancelled>(cancel.clone()),
                AsyncSystems::from_single(async_system!(
                    |click: Receiver<Click>, press: Receiver<Pressed>, focus: Receiver<ObtainFocus>, lose: Receiver<LoseFocus>, cancel: Receiver<ClickCancelled>, this: AsyncComponent<Text>| {
                        futures::select_biased! {
                            pos = click.fuse() => {
                                let s = format!("Clicked at {pos}");
                                this.set(move |text| text.sections[0].value = s).await.unwrap();
                            },
                            pos = press.fuse() => {
                                let s = format!("Pressed at {pos}");
                                this.set(move |text| text.sections[0].value = s).await.unwrap();
                            },
                            pos = focus.fuse() => {
                                let s = format!("Obtained focus at {pos}");
                                this.set(move |text| text.sections[0].value = s).await.unwrap();
                            },
                            pos = lose.fuse() => {
                                let s = format!("Lose focus at {pos}");
                                this.set(move |text| text.sections[0].value = s).await.unwrap();
                            },
                            pos = cancel.fuse() => {
                                let s = format!("Click cancelled at {pos}");
                                this.set(move |text| text.sections[0].value = s).await.unwrap();
                            },
                        }
                    } 
                ))
            ));
        });

    commands.spawn_task(move || async move {
        let world = world();
        let entity = world.entity(btn_entity);
        let btn = entity.query::<AsyncUIButton>();
        loop {
            // The other methods can yield immediately so ignored here.
            futures::select_biased! {
                pos = btn.clicked().fuse() => println!("Task: Clicked at {pos}"),
                pos = btn.cancelled().fuse() => println!("Task: Click cancelled at {pos}"),
            }
        }
    });
}