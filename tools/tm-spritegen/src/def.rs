//! スプライト定義(`assets/sprites/*.ron`)の型。

use serde::Deserialize;

/// 1 つの定義ファイル。`tile` は手続き的なスプライトのマスの大きさ(偶数)。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename = "Sprites", deny_unknown_fields)]
pub struct SpriteFile {
    /// マスの一辺(px)。
    pub tile: u32,
    /// スプライトの定義。
    pub sprites: Vec<SpriteDef>,
}

/// スプライト 1 件の定義。`source` か `frames` のどちらか一方を持つ。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpriteDef {
    /// 名前。`frames` があれば `_f0`、`_f1` … を付けた名前になる。
    pub name: String,
    /// 基準点(左上からの位置)。省略は `(0, 0)`。`Derive` は反転に合わせた元の基準点。
    #[serde(default)]
    pub pivot: Option<(i32, i32)>,
    /// tint 前提か。省略は false。`Derive` は元の値。
    #[serde(default)]
    pub tint: Option<bool>,
    /// アニメーション(2 フレーム以上のとき必須)。
    #[serde(default)]
    pub anim: Option<AnimDef>,
    /// 単一のスプライトの作り方。
    #[serde(default)]
    pub source: Option<Source>,
    /// アニメーションのフレーム。
    #[serde(default)]
    pub frames: Vec<Source>,
}

/// アニメーションの設定。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimDef {
    /// 1 フレームの時間(ms)。
    pub frame_ms: u16,
    /// 繰り返すか。
    #[serde(default)]
    pub looping: bool,
}

/// 絵の作り方。
#[derive(Debug, Clone, Deserialize)]
pub enum Source {
    /// パレットの記号で書いたドット絵(1 文字 = 1 画素、1 要素 = 1 行)。
    Pixels(Vec<String>),
    /// ほかのスプライトからの派生。
    Derive {
        /// 元の定義の名前。
        from: String,
        /// 操作。
        op: Op,
    },
    /// 組み込みの手続き。
    Proc(Proc),
}

/// 派生の操作。
#[derive(Debug, Clone, Deserialize)]
pub enum Op {
    /// 左右反転。
    FlipX,
    /// 上下反転。
    FlipY,
    /// 180 度回転。
    Rot180,
    /// 色の置き換え((元の記号, 先の記号) の組)。
    Recolor(Vec<(char, char)>),
}

/// 8 方向(10 §3.5 の `Dir8`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Dir {
    /// 東。
    E,
    /// 北東。
    NE,
    /// 北。
    N,
    /// 北西。
    NW,
    /// 西。
    W,
    /// 南西。
    SW,
    /// 南。
    S,
    /// 南東。
    SE,
}

/// 線路と帯の部品(15 §5.8)。帯の `conv` は「集める」版(`lane` が 1 のときは平行と同じ)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Part {
    /// 線路の縁。
    RailOutline,
    /// 線路の中身。
    RailFill,
    /// 帯の縁。
    BandOutline {
        /// レーン(0〜2)。
        lane: u8,
        /// 集める版か。
        conv: bool,
    },
    /// 帯の中身。
    BandFill {
        /// レーン(0〜2)。
        lane: u8,
        /// 集める版か。
        conv: bool,
    },
}

/// 組み込みの手続き(15 §5.7)。
#[derive(Debug, Clone, Deserialize)]
pub enum Proc {
    /// マスの中心から 1 方向の半区間(外接矩形で切り出す。基準点は計算する)。
    TrackHalf {
        /// 向き。
        dir: Dir,
        /// 部品。
        part: Part,
    },
    /// 9 分割の枠。名前に `_tl _t _tr _l _c _r _bl _b _br` が付く 9 枚になる。
    NineSlice {
        /// 角の大きさ。
        corner: u32,
        /// 縁の太さ(省略は 1、0 なら縁を描かない)。
        #[serde(default = "one")]
        border_w: u32,
        /// 中の色の名前。
        fill: String,
        /// 縁の色の名前。
        border: String,
        /// 外側の 1px の色の名前。
        outline: String,
    },
    /// マップのカーソルの枠((tile + 6) 四方)。
    CornerFrame,
    /// 1 色の矩形。
    Fill {
        /// 幅。
        w: u32,
        /// 高さ。
        h: u32,
        /// 色の名前。
        color: String,
    },
}

fn one() -> u32 {
    1
}
