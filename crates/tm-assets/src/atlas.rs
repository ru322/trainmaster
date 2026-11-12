//! アトラスのページと、スプライト・グリフ共通の矩形の表 [15 §3.2、§3.3]。

use tm_platform_api::{AtlasImage, SpriteAlt, SpriteId, SpriteRect};

use crate::font::GLYPH_COUNT;
use crate::sprite::SPRITE_COUNT;
use crate::{font, sprite};

/// ページの中で 1 つのツールが受け持つ帯の位置。`y` と `height` はページの画素の行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StripInfo {
    /// ページの番号。
    pub page: u8,
    /// 帯の上端。
    pub y: u32,
    /// 帯の高さ。
    pub height: u32,
}

include!(concat!(env!("OUT_DIR"), "/pages.rs"));

/// スプライトとグリフを合わせた `SpriteId` の数。
pub const SPRITE_TOTAL: usize = SPRITE_COUNT + GLYPH_COUNT;
const _: () = assert!(SPRITE_TOTAL <= u16::MAX as usize);
const _: () = assert!(sprite::SPRITE_RENDER_SCALE == tm_platform_api::RENDER_SCALE);
const _: () = assert!(font::GLYPH_RENDER_SCALE == tm_platform_api::RENDER_SCALE);

const fn concat_rects<const A: usize, const B: usize, const C: usize>(
    a: &[SpriteRect; A],
    b: &[SpriteRect; B],
) -> [SpriteRect; C] {
    assert!(A + B == C);
    let mut out = [SpriteRect::ZERO; C];
    let mut i = 0;
    while i < A {
        out[i] = a[i];
        i += 1;
    }
    let mut j = 0;
    while j < B {
        out[A + j] = b[j];
        j += 1;
    }
    out
}

/// `SpriteId` の番号順の矩形。スプライト、続いてグリフ。
pub static ALL_RECTS: [SpriteRect; SPRITE_TOTAL] =
    concat_rects(&sprite::SPRITE_RECTS, &font::GLYPH_RECTS);

/// 大きい版の数(スプライトとグリフ)。
pub const ALT_TOTAL: usize = sprite::SPRITE_ALT_COUNT + font::GLYPH_ALT_COUNT;

const fn concat_alt<const A: usize, const B: usize, const C: usize>(
    a: &[SpriteAlt; A],
    b: &[(u16, SpriteRect); B],
) -> [SpriteAlt; C] {
    assert!(A + B == C);
    let mut out = [SpriteAlt {
        id: SpriteId(0),
        rect: SpriteRect::ZERO,
    }; C];
    let mut i = 0;
    while i < A {
        out[i] = a[i];
        i += 1;
    }
    let mut j = 0;
    while j < B {
        out[A + j] = SpriteAlt {
            id: SpriteId((SPRITE_COUNT + b[j].0 as usize) as u16),
            rect: b[j].1,
        };
        j += 1;
    }
    out
}

/// `SpriteId` の昇順の大きい版。スプライト、続いてグリフ。
pub static ALL_ALT: [SpriteAlt; ALT_TOTAL] = concat_alt(&sprite::SPRITE_ALT, &font::GLYPH_ALT);

/// プラットフォームに渡すアトラスのページ。
pub static ATLASES: [AtlasImage; PAGE_COUNT] = {
    let mut pages = [AtlasImage {
        width: 0,
        height: 0,
        rgba: &[],
    }; PAGE_COUNT];
    let mut i = 0;
    while i < PAGE_COUNT {
        pages[i] = AtlasImage {
            width: PAGE_DIMS[i].0,
            height: PAGE_DIMS[i].1,
            rgba: PAGE_RGBA[i],
        };
        i += 1;
    }
    pages
};
