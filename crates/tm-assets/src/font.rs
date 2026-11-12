//! ビットマップフォントの寸法とグリフの検索。生成物。生成元は `tm-fontgen`(`assets/fonts/fontgen.toml`)。

use tm_platform_api::SpriteId;

use crate::sprite::SPRITE_COUNT;

const NO_BITMAP: u16 = u16::MAX;

/// フォントの種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontId {
    /// 本文用(12px)。
    Body12,
    /// 見出し用(16px)。
    Head16,
}

#[derive(Clone, Copy, Debug)]
struct GlyphEntry {
    ch: char,
    advance: u8,
    index: u16,
    w: u8,
    h: u8,
    x_off: i8,
    y_off: i8,
}

/// 1 つの大きさのフォント。
#[derive(Debug)]
pub struct FontFace {
    /// 種類。
    pub id: FontId,
    /// 大きさ(論理 px。字形は描画の倍率をかけた画素で作ってある)。
    pub size_px: u8,
    /// ベースラインから行の上端まで。
    pub ascent: u8,
    /// ベースラインから下端まで。
    pub descent: u8,
    /// 行送り。
    pub line_height: u8,
    /// 全角の送り幅。
    pub full_advance: u8,
    glyphs: &'static [GlyphEntry],
    ascii: &'static [u16; 95],
}

/// 1 字のグリフ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    /// 字。
    pub ch: char,
    /// 送り幅(px)。
    pub advance: u8,
    /// 字形。空白は `None`。
    pub bitmap: Option<GlyphBitmap>,
}

/// グリフの字形。ペンの位置とベースラインからのずれで置く。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphBitmap {
    /// アトラス上のスプライト。
    pub sprite: SpriteId,
    /// 幅。
    pub w: u8,
    /// 高さ。
    pub h: u8,
    /// ペンの位置から左端まで。
    pub x_off: i8,
    /// ベースラインから上端まで。上が負。
    pub y_off: i8,
}

#[allow(missing_docs)]
mod generated {
    use super::{FontFace, FontId, GlyphEntry, NO_BITMAP};
    use crate::atlas::StripInfo;
    use tm_platform_api::SpriteRect;

    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/glyphs.rs"));
}

pub use generated::{
    FONT_SHA256, FONT_STRIP, GLYPH_ALT, GLYPH_ALT_COUNT, GLYPH_COUNT, GLYPH_RECTS,
    GLYPH_RENDER_SCALE,
};

impl FontFace {
    fn entry(&self, ch: char) -> Option<&'static GlyphEntry> {
        let glyphs: &'static [GlyphEntry] = self.glyphs;
        if (' '..='~').contains(&ch) {
            let index = self.ascii[ch as usize - ' ' as usize];
            return (index != u16::MAX).then(|| &glyphs[usize::from(index)]);
        }
        glyphs
            .binary_search_by_key(&ch, |e| e.ch)
            .ok()
            .map(|i| &glyphs[i])
    }
}

fn to_glyph(e: &GlyphEntry) -> Glyph {
    let bitmap = (e.index != NO_BITMAP).then(|| GlyphBitmap {
        sprite: SpriteId(
            u16::try_from(SPRITE_COUNT + usize::from(e.index))
                .expect("SPRITE_TOTAL は u16 に収まる"),
        ),
        w: e.w,
        h: e.h,
        x_off: e.x_off,
        y_off: e.y_off,
    });
    Glyph {
        ch: e.ch,
        advance: e.advance,
        bitmap,
    }
}

/// フォントの寸法。
pub fn face(id: FontId) -> &'static FontFace {
    match id {
        FontId::Body12 => &generated::FACE_BODY12,
        FontId::Head16 => &generated::FACE_HEAD16,
    }
}

/// その字のグリフ。フォントにない字は `None`。
pub fn glyph(id: FontId, ch: char) -> Option<Glyph> {
    face(id).entry(ch).map(to_glyph)
}

/// フォントにない字の代わりに描く「?」のグリフ。
pub fn fallback_glyph(id: FontId) -> Glyph {
    glyph(id, '?').expect("「?」は ASCII なのでフォントにある")
}

/// 折り返しなしの幅(px)。改行があれば最長の行の幅。
pub fn measure(id: FontId, s: &str) -> u32 {
    let fallback = fallback_glyph(id);
    s.split('\n')
        .map(|line| {
            line.chars()
                .map(|ch| u32::from(glyph(id, ch).unwrap_or(fallback).advance))
                .sum()
        })
        .max()
        .unwrap_or(0)
}
