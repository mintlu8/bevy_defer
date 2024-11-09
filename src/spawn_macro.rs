use crate::AsyncWorld;
use bevy::{
    asset::{Asset, AssetPath, Handle},
    ecs::component::{ComponentHooks, StorageType},
    prelude::{BuildChildren, Bundle, Component},
};
pub use default_constructor;
use std::ops::DerefMut;

/// Add an asset from its type, returns its [`Handle`].
pub fn add<T: Asset>(item: T) -> Handle<T> {
    AsyncWorld
        .add_asset::<T>(item)
        .expect("Asset not registered.")
}

/// Load an asset from its [`AssetPath`], returns its [`Handle`].
pub fn load<T: Asset>(item: AssetPath<'static>) -> Handle<T> {
    AsyncWorld.load_asset::<T>(item).into_handle()
}

#[derive(Debug, Default)]
pub enum SpawnChild<B: Bundle> {
    Child(B),
    #[default]
    None,
}

impl<B: Bundle> Component for SpawnChild<B> {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        hooks.on_add(|mut world, entity, _| {
            if let Some(mut spawn) = world.entity_mut(entity).get_mut::<SpawnChild<B>>() {
                match std::mem::take(spawn.deref_mut()) {
                    SpawnChild::Child(bundle) => {
                        world.commands().entity(entity).with_child(bundle);
                    }
                    SpawnChild::None => todo!(),
                };
            }
        });
    }
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
        );
    }
}
