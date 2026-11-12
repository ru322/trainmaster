#![allow(dead_code)]

use tm_platform_api::{AssetBundle, AtlasImage, SoundData, SoundKind, SpriteId, SpriteRect};

pub fn leak<T>(items: Vec<T>) -> &'static [T] {
    Box::leak(items.into_boxed_slice())
}

pub fn atlas(width: u32, height: u32) -> AtlasImage {
    AtlasImage {
        width,
        height,
        rgba: leak(vec![0xFF; width as usize * height as usize * 4]),
    }
}

pub fn rect(atlas: u8, x: u16, y: u16, w: u16, h: u16) -> SpriteRect {
    SpriteRect { atlas, x, y, w, h }
}

pub fn sound(
    name: &'static str,
    kind: SoundKind,
    len: usize,
    loop_start: Option<u32>,
) -> SoundData {
    SoundData {
        name,
        kind,
        samples: leak(vec![0; len]),
        loop_start,
    }
}

pub fn bundle(
    atlases: Vec<AtlasImage>,
    sprites: Vec<tm_platform_api::SpriteRect>,
    sounds: Vec<SoundData>,
) -> AssetBundle {
    AssetBundle {
        atlases: leak(atlases),
        sprites: leak(sprites),
        alt: &[],
        sounds: leak(sounds),
        white: SpriteId(0),
    }
}

pub fn valid_bundle() -> AssetBundle {
    bundle(
        vec![atlas(16, 16)],
        vec![rect(0, 0, 0, 2, 2), rect(0, 2, 0, 4, 4)],
        vec![
            sound("bgm_a", SoundKind::Bgm, 100, Some(10)),
            sound("jingle_a", SoundKind::Jingle, 100, None),
            sound("se_a", SoundKind::Se, 100, None),
        ],
    )
}
