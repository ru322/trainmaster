//! スプライトの `SpriteId`、寸法、アニメーション、9 分割。生成物。生成元は `tm-spritegen`(`assets/sprites`)。

use tm_platform_api::SpriteId;

/// スプライトごとの描画用の情報。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteMeta {
    /// 幅。
    pub w: u16,
    /// 高さ。
    pub h: u16,
    /// 基準点のスプライト左上からの位置。`dst = 基準の位置 - pivot`。
    pub pivot_x: i16,
    /// 基準点のスプライト左上からの位置。
    pub pivot_y: i16,
    /// 白〜灰色で描いてあり、tint で色を付ける前提か。
    pub tintable: bool,
}

/// アニメーション。フレームの `SpriteId` は連続している。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anim {
    /// 先頭のフレーム。
    pub first: SpriteId,
    /// フレーム数。
    pub frames: u8,
    /// 1 フレームの長さ。
    pub frame_ms: u16,
    /// 繰り返すか。
    pub looping: bool,
}

impl Anim {
    /// 経過時間のフレーム。繰り返さないときは最後のフレームで止まる。
    pub fn frame_at(&self, elapsed_ms: u64) -> SpriteId {
        let last = u64::from(self.frames.max(1)) - 1;
        let step = elapsed_ms / u64::from(self.frame_ms.max(1));
        let index = if self.looping {
            step % (last + 1)
        } else {
            step.min(last)
        };
        SpriteId(self.first.0 + u16::try_from(index).expect("フレーム数は u8"))
    }
}

/// 9 分割の枠。角は等倍、辺と中央は描画先の大きさに伸ばす。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NineSlice {
    /// 左上、上、右上、左、中、右、左下、下、右下。
    pub parts: [SpriteId; 9],
    /// 角の大きさ。
    pub corner: u8,
}

#[allow(missing_docs)]
mod generated {
    use super::{Anim, NineSlice, SpriteMeta};
    use crate::atlas::StripInfo;
    use tm_platform_api::{SpriteId, SpriteRect};

    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/sprites.rs"));
}

pub use generated::*;

/// スプライトの寸法と基準点。グリフの ID を渡すと panic する(グリフは `font::glyph`)。
pub fn meta(id: SpriteId) -> &'static SpriteMeta {
    &SPRITE_META[usize::from(id.0)]
}

/// スプライトの名前。グリフの ID を渡すと panic する。
pub fn name(id: SpriteId) -> &'static str {
    SPRITE_NAMES[usize::from(id.0)]
}
