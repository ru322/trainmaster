//! 論理解像度の座標と色 [10 §8.1]。

/// 論理解像度の幅。
pub const LOGICAL_WIDTH: u32 = 640;
/// 論理解像度の高さ。
pub const LOGICAL_HEIGHT: u32 = 360;

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
