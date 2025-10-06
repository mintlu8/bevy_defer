use std::{any::Any, sync::Arc};

use bevy::prelude::*;
use bevy_defer::{
    access::{AsyncResource, AsyncWorld},
    async_access, async_dyn, AsyncExtension, AsyncPlugin,
};

#[derive(Debug, Resource, AsyncResource)]
pub struct Unit {
    name: String,
    id: i32,
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
        id: 0,
    });
    app.spawn_task(async {
        AsyncWorld.resource::<Unit>().set_name("Name")?;
        assert_eq!(AsyncWorld.resource::<Unit>().name().unwrap(), "Name");
        AsyncWorld.quit();
        Ok(())
    });
    app.run();
}

struct Ref<'t>(&'t ());

trait AsyncTrait {
    #[async_dyn]
    async fn a(&self) -> i32;
    #[async_dyn]
    async fn b(self: Arc<Self>) -> i32;
    #[async_dyn]
    async fn c(&self, slice: &str) -> i32;
    #[async_dyn]
    async fn d(self: Arc<Self>, slice: &str) -> i32;
    #[async_dyn]
    async fn e(&self, slice: &str) -> &i32;
    #[async_dyn]
    async fn f<'t>(&'t self, slice: &'t str) -> &'t i32;
    #[async_dyn]
    async fn g(&self, slice: &str) -> Ref<'_>;
    #[async_dyn]
    async fn h(&self, slice: &str) -> Box<dyn Any>;
}

// Dyn compatible.
const _: Option<Box<dyn AsyncTrait>> = None;

impl AsyncTrait for Unit {
    #[async_dyn]
    async fn a(&self) -> i32 {
        self.id
    }

    #[async_dyn]
    async fn b(self: Arc<Self>) -> i32 {
        self.id
    }

    #[async_dyn]
    async fn c(&self, _: &str) -> i32 {
        self.id
    }

    #[async_dyn]
    async fn d(self: Arc<Self>, _: &str) -> i32 {
        self.id
    }

    #[async_dyn]
    async fn e(&self, _: &str) -> &i32 {
        &self.id
    }

    #[async_dyn]
    async fn f<'t>(&'t self, _: &'t str) -> &'t i32 {
        &self.id
    }

    #[async_dyn]
    async fn g(&self, _: &str) -> Ref<'_> {
        Ref(&())
    }

    #[async_dyn]
    async fn h(&self, _: &str) -> Box<dyn Any> {
        Box::new(self.id)
    }
}
