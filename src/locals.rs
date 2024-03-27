use bevy_asset::AssetServer;
use bevy_ecs::{system::SystemParam, world::World};
use scoped_tls::scoped_thread_local;

/// Convert a resource into thread local storage accessible within the async runtime.
pub trait LocalResourceScope: 'static {
    type Resource: SystemParam + 'static;
    fn scoped<T>(this: &<Self::Resource as SystemParam>::Item::<'_, '_>, f: impl FnOnce() -> T) -> T;
    fn maybe_scoped<T>(this: Option<&<Self::Resource as SystemParam>::Item::<'_, '_>>, f: impl FnOnce() -> T) -> T {
        if let Some(item) = this {
            Self::scoped(item, f)
        } else {
            f()
        }
    }
}

impl LocalResourceScope for () {
    type Resource = ();
    fn scoped<T>(_: &(), f: impl FnOnce() -> T) -> T {
        f()
    }
}

scoped_thread_local!(pub(crate) static SYNC_WORLD: World);

/// Run a function on a readonly [`World`] in the async context.
/// 
/// Returns [`Err`] if world access is not enabled, 
/// add `with_world_access` on the plugin to enable this access.
pub fn with_sync_world<T: 'static, F: FnOnce(&World) -> T>(f: F) -> Result<T, F> {
    if SYNC_WORLD.is_set() {
        Ok(SYNC_WORLD.with(f))
    } else {
        Err(f)
    }
}

/// Run a function on [`AssetServer`] in the async context.
/// 
/// # Panics
/// 
/// If [`AssetServer`] does not exist.
pub fn with_asset_server<T: 'static, F: FnOnce(&AssetServer) -> T>(f: F) -> T {
    if ASSET_SERVER.is_set() {
        ASSET_SERVER.with(f)
    } else {
        panic!("Asset server does not exist.")
    }
}

// RefCell<&mut World> is possible but that would probably be deadlock city.
impl LocalResourceScope for World {
    type Resource = &'static World;

    fn scoped<T>(this: &<Self::Resource as SystemParam>::Item::<'_, '_>, f: impl FnOnce() -> T) -> T {
        SYNC_WORLD.set(this, f)
    }
}

crate::tls_resource!(pub ASSET_SERVER: AssetServer);

impl<A, B> LocalResourceScope for (A, B) where A: LocalResourceScope, B: LocalResourceScope {
    type Resource = (A::Resource, B::Resource);

    fn scoped<T>(this: &<Self::Resource as SystemParam>::Item::<'_, '_>, f: impl FnOnce() -> T) -> T {
        A::scoped(&this.0, ||B::scoped(&this.1, f))
    }
}

/// Convert a `NonSend` resource into thread local storage through the [`scoped_tls`] crate.
/// 
/// # Example
/// 
/// ```
/// tls_resource_local!(pub MY_LOCAL_RESOURCE: MyLocalResource);
/// ```
#[macro_export]
macro_rules! tls_resource_local {
    ($vis: vis $name: ident: $ty: ty) => {
        $crate::scoped_thread_local!($vis static $name: $ty);
        impl $crate::LocalResourceScope for $ty {
            type Resource = $crate::NonSend<'static, $ty>;

            fn scoped<T>(this: &<Self::Resource as $crate::SystemParam>::Item::<'_, '_>, f: impl FnOnce() -> T) -> T {
                $name.set(&*this, f)
            }
        }
    };
}


/// Convert a `Resource` into thread local storage through the [`scoped_tls`] crate.
/// 
/// # Example
/// 
/// ```
/// tls_resource!(pub MY_RESOURCE: MyResource);
/// ```
#[macro_export]
macro_rules! tls_resource {
    ($vis: vis $name: ident: $ty: ty) => {
        $crate::scoped_thread_local!($vis static $name: $ty);
        impl $crate::LocalResourceScope for $ty {
            type Resource = $crate::Res<'static, $ty>;

            fn scoped<T>(this: &<Self::Resource as $crate::SystemParam>::Item::<'_, '_>, f: impl FnOnce() -> T) -> T {
                $name.set(&*this, f)
            }
        }
    };
}

#[cfg(test)]
mod test {
    use bevy_ecs::system::Resource;

    pub struct NonSend;

    #[derive(Resource)]
    pub struct Res;
    tls_resource_local!(pub NON_SEND: NonSend);
    tls_resource!(pub MY_RESOURCE: Res);
}