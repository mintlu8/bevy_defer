use bevy::MinimalPlugins;
use bevy_app::App;
use bevy_defer::{
    access::{AsyncResource, AsyncWorld},
    async_access, AsyncExtension, AsyncPlugin,
};
use bevy_ecs::system::Resource;

#[derive(Debug, Resource, AsyncResource)]
pub struct Unit {
    name: String,
}

#[async_access]
impl Unit {
    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into()
    }
}

#[test]
fn test() {
    let mut app = App::new();
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        AsyncWorld.resource::<Unit>().set_name("Name")?;
        Ok(())
    });
}
