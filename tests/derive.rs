use bevy::prelude::*;
use bevy_defer::{
    access::{AsyncResource, AsyncWorld},
    async_access, AsyncExtension, AsyncPlugin,
};

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
    app.insert_resource(Unit {
        name: "".to_owned(),
    });
    app.spawn_task(async {
        AsyncWorld.resource::<Unit>().set_name("Name")?;
        assert_eq!(AsyncWorld.resource::<Unit>().name().unwrap(), "Name");
        AsyncWorld.quit();
        Ok(())
    });
    app.run();
}
