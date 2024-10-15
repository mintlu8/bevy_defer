use bevy::prelude::*;
use bevy_defer::{AsyncExtension, AsyncPlugin, AsyncWorld};

#[test]
pub fn unblock_test() {
    let mut app = App::new();
    app.add_plugins(AsyncPlugin::default_settings());
    app.add_plugins(MinimalPlugins);
    app.spawn_task(async move {
        let a = AsyncWorld.unblock(|| "A".to_string());
        let b = AsyncWorld.unblock(|| "B".to_string());
        let c = AsyncWorld.unblock(|| "C".to_string());
        assert_eq!(a.await, "A");
        assert_eq!(b.await, "B");
        assert_eq!(c.await, "C");
        AsyncWorld.quit();
        Ok(())
    });
    app.run();
}
