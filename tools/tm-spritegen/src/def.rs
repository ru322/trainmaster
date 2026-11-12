//! スプライト定義(`assets/sprites/*.ron`)の型。

use serde::Deserialize;

/// 1 つの定義ファイル。`tile` は手続き的なスプライトのマスの大きさ(偶数)。
/// 寸法と基準点は設計の px(UI が `scale` 倍の論理 px で描く)。絵は描画の倍率 × `scale` の画素で作る。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename = "Sprites", deny_unknown_fields)]
pub struct SpriteFile {
    /// マスの一辺(設計の px)。
    pub tile: u32,
    /// UI が描く倍率(設計の 1px が論理の何 px か)。省略は 1。マップの絵は 2。
    #[serde(default)]
    pub scale: Option<u32>,
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
    /// UI が描く倍率。省略はファイルの `scale`。
    #[serde(default)]
    pub scale: Option<u32>,
    /// `scale` の 2 倍でも描くか(マップの上でも使う UI の絵)。2 倍の画素の大きい版をアトラスに足す。
    #[serde(default)]
    pub alt: Option<bool>,
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
    /// 図形の絵。`w × h`(設計の px)に、描く順の図形を並べる。
    Shapes {
        /// 幅。
        w: u32,
        /// 高さ。
        h: u32,
        /// 図形。
        paint: Vec<ShapeDef>,
    },
}

/// 設計の px の数。1/16 px の倍数でなければならない(小数で書ける)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Num(pub i64);

impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Num, D::Error> {
        let v = f64::deserialize(d)?;
        let q = v * crate::vector::FX as f64;
        if !q.is_finite() || q.fract() != 0.0 || q.abs() > 1e6 {
            return Err(serde::de::Error::custom(format!(
                "{v} は 1/{} px の倍数でない",
                crate::vector::FX
            )));
        }
        Ok(Num(q as i64))
    }
}

/// 図形 1 つ。色はパレットの名前。
#[derive(Debug, Clone, Deserialize)]
pub enum ShapeDef {
    /// 角を `r` で丸めた矩形。
    Rect {
        /// 左。
        x: Num,
        /// 上。
        y: Num,
        /// 幅。
        w: Num,
        /// 高さ。
        h: Num,
        /// 角の半径(省略は 0)。
        #[serde(default = "zero")]
        r: Num,
        /// 色。
        color: String,
    },
    /// 両端の丸い線。
    Line {
        /// 始点。
        from: (Num, Num),
        /// 終点。
        to: (Num, Num),
        /// 太さ。
        width: Num,
        /// 色。
        color: String,
    },
    /// 円。
    Circle {
        /// 中心の x。
        x: Num,
        /// 中心の y。
        y: Num,
        /// 半径。
        r: Num,
        /// 色。
        color: String,
    },
    /// 多角形(偶奇規則)。
    Poly {
        /// 頂点。
        pts: Vec<(Num, Num)>,
        /// 色。
        color: String,
    },
}

fn zero() -> Num {
    Num(0)
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
    /// 絵の中心のまわりに 45 度 × n だけ反時計回り(画面で)に回す。ドット絵は 90 度の倍数だけ。
    Rotate(u8),
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
