//! 描画コマンド [10 §8.2]。

use crate::geom::{Color, Rect};

/// `AssetBundle::sprites` の添字。`0..SPRITE_COUNT` はスプライト、以降はグリフ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpriteId(pub u16);

/// キャッシュ層の番号。使えるのは `0..=3`(14 §3.5.2)。0 と 1 は静的なマップ層(マップを縦に切った 2 枚)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayerId(pub u8);

/// 描画コマンド 1 つ。座標は論理ピクセル。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrawCmd {
    /// 全体を塗る。
    Clear(Color),
    /// 矩形をアルファ合成で塗る。
    FillRect {
        /// 塗る範囲。
        rect: Rect,
        /// 色。
        color: Color,
    },
    /// 太さ 1 の線。
    Line {
        /// 始点の x。
        x0: i32,
        /// 始点の y。
        y0: i32,
        /// 終点の x。
        x1: i32,
        /// 終点の y。
        y1: i32,
        /// 色。
        color: Color,
    },
    /// スプライトを `dst` の大きさに最近傍で拡大縮小して描き、`tint` を乗算する。
    Sprite {
        /// スプライト。
        id: SpriteId,
        /// 描く範囲。
        dst: Rect,
        /// 乗算する色。
        tint: Color,
    },
    /// キャッシュ層の始まり。中身は層の座標(原点は層の左上)で書く(14 §3.5)。
    BeginCachedLayer {
        /// 層。
        layer: LayerId,
        /// 前回と同じ値で大きさも同じなら描き直さない。
        revision: u64,
        /// 層を画面に置く x。
        x: i32,
        /// 層を画面に置く y。
        y: i32,
        /// 層の幅。
        w: u32,
        /// 層の高さ。
        h: u32,
    },
    /// キャッシュ層の終わり。
    EndCachedLayer,
    /// 同じフレームの `BeginCachedLayer` の位置と大きさに層を描く。
    DrawCachedLayer {
        /// 層。
        layer: LayerId,
    },
}

/// 1 フレーム分の描画コマンド。上限は描画要素 2000 個(キャッシュ層は 1 個)[02 §9.2]。
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    /// コマンドの列。
    pub cmds: Vec<DrawCmd>,
}
