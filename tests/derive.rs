use bevy::MinimalPlugins;
use bevy_app::App;
use bevy_defer::{access::{deref::AsyncResourceDeref, AsyncResource}, async_access, world, AsyncExtension, AsyncPlugin};
use bevy_ecs::system::Resource;
use ref_cast::RefCast;


#[derive(Debug, Resource)]
pub struct Unit {
    name: String,
}

#[derive(Debug, RefCast)]
#[repr(transparent)]
pub struct AsyncUnit(pub AsyncResource<Unit>);

impl AsyncResourceDeref for Unit {
    type Target = AsyncUnit;

    fn async_deref(this: &AsyncResource<Self>) -> &Self::Target {
        AsyncUnit::ref_cast(this)
    }
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

fn test() {
    let mut app = App::new();
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async {
        world().resource::<Unit>().set_name("Name")?;
        Ok(())
    });
}