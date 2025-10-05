use crate::AsyncWorld;
use bevy::asset::{Asset, AssetPath, Handle};
pub use default_constructor;
pub use default_constructor::InferInto;

/// Add an asset from its type, returns its [`Handle`].
pub fn add<T: Asset>(item: T) -> Handle<T> {
    AsyncWorld
        .add_asset::<T>(item)
        .expect("Asset not registered.")
}

/// Load an asset from its [`AssetPath`], returns its [`Handle`].
pub fn load<T: Asset>(item: AssetPath<'static>) -> Handle<T> {
    AsyncWorld.load_asset::<T>(item).try_into_handle().unwrap()
}

/// Spawn a bundle using `bevy_defer`'s [`AsyncWorld`].
///
/// See [`default_constructor`].
///
/// # Effects
///
/// * `add`: Turns `impl Into<T>` into `Handle<T>`.
/// * `load`: Turns `impl Into<AssetPath>` into `Handle<T>`.
///
/// # Panics
///
/// If used outside of a `bevy_defer` future.
#[cfg_attr(docsrs, doc(cfg(feature = "spawn_macro")))]
#[macro_export]
macro_rules! spawn {
    ($($tt: tt)*) => {
        {
            #[allow(unused)]
            use $crate::spawn_macro::default_constructor::effects::*;
            #[allow(unused)]
            use $crate::spawn_macro::{add, load};
            $crate::AsyncWorld.spawn_bundle(
                $crate::spawn_macro::default_constructor::meta_default_constructor! {
                    [$crate::spawn_macro::default_constructor::infer_into]
                    $($tt)*
                }
            )
        }
    };
}

#[cfg(test)]
mod test {
    use bevy::{color::Srgba, prelude::Mesh2d, sprite::Sprite};

    pub fn test() {
        spawn!(
            Sprite {
                image: @load "1.png",
                color: Srgba::RED,
            },
            Mesh2d(@load "Mesh.gltf#Scene0"),
        )
        .add_child(child);
    }
}
