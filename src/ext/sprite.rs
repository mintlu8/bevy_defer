use bevy_sprite::{TextureAtlas, TextureAtlasLayout};

use crate::access::{traits::WorldDeref, AsyncAsset};

impl WorldDeref for TextureAtlas {
    type Target = AsyncAsset<TextureAtlasLayout>;

    fn deref_to(&self) -> Self::Target {
        AsyncAsset(self.layout.clone_weak())
    }
}
