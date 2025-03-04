use bevy::{
    app::{App, Startup},
    input::keyboard::KeyboardInput,
    prelude::{Camera2d, KeyCode, World},
    text::Text2d,
    DefaultPlugins,
};
use bevy_defer::{AppReactorExtension, AsyncAccess, AsyncExtension, AsyncPlugin, AsyncWorld};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AsyncPlugin::default_settings())
        .add_systems(Startup, |w: &mut World| {
            w.spawn(Camera2d);
            w.spawn(Text2d::new("Press some keys!"));
        })
        .react_to_event::<KeyboardInput>()
        .spawn_task(async {
            let phrase = [
                KeyCode::ArrowUp,
                KeyCode::ArrowUp,
                KeyCode::ArrowDown,
                KeyCode::ArrowDown,
                KeyCode::ArrowLeft,
                KeyCode::ArrowRight,
                KeyCode::ArrowLeft,
                KeyCode::ArrowRight,
                KeyCode::KeyB,
                KeyCode::KeyA,
            ];
            let mut idx = 0;
            loop {
                let item = AsyncWorld.next_event::<KeyboardInput>().await;
                if phrase[idx] == item.key_code {
                    idx += 1;
                    if idx >= phrase.len() {
                        break;
                    }
                }
            }
            AsyncWorld
                .query_single::<&mut Text2d>()
                .get_mut(|mut x| x.0 = "You dirty cheater!".into())?;
            Ok(())
        })
        .run();
}
