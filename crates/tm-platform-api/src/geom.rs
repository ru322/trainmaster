//! 論理解像度の座標と色 [10 §8.1]。

/// 論理解像度の幅。
pub const LOGICAL_WIDTH: u32 = 640;
/// 論理解像度の高さ。
pub const LOGICAL_HEIGHT: u32 = 360;
/// 論理 1px あたりの描画の画素数。プラットフォームは論理座標をこの倍率で描く。
pub const RENDER_SCALE: u32 = 3;
/// 描画の解像度の幅(1920)。
pub const RENDER_WIDTH: u32 = LOGICAL_WIDTH * RENDER_SCALE;
/// 描画の解像度の高さ(1080)。
pub const RENDER_HEIGHT: u32 = LOGICAL_HEIGHT * RENDER_SCALE;
/// 細かい座標の単位の数(論理 1px = `SUBPX`)。1 単位は描画の 1 画素。
pub const SUBPX: i32 = RENDER_SCALE as i32;

/// ストレートアルファの色。`a` = 255 で不透明。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    /// 赤。
    pub r: u8,
    /// 緑。
    pub g: u8,
    /// 青。
    pub b: u8,
    /// 不透明度。
    pub a: u8,
}

/// 論理ピクセルの矩形。`w` または `h` が 0 なら何も描かない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// 左端。
    pub x: i32,
    /// 上端。
    pub y: i32,
    /// 幅。
    pub w: u32,
    /// 高さ。
    pub h: u32,
}
