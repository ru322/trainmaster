//! RGBA の画像と、反転などの整数の操作。

/// 8 ビット RGBA の画像。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// 幅(px)。
    pub w: u32,
    /// 高さ(px)。
    pub h: u32,
    /// 行優先の画素。
    pub px: Vec<[u8; 4]>,
}

/// 透明の画素。
pub const CLEAR: [u8; 4] = [0, 0, 0, 0];

impl Image {
    /// 透明で埋めた画像。
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![CLEAR; (w * h) as usize],
        }
    }

    /// 1 色で埋めた画像。
    pub fn filled(w: u32, h: u32, color: [u8; 4]) -> Self {
        Self {
            w,
            h,
            px: vec![color; (w * h) as usize],
        }
    }

    /// 画素を返す。範囲外は透明。
    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            CLEAR
        } else {
            self.px[(y as u32 * self.w + x as u32) as usize]
        }
    }

    /// 画素を書く。範囲外は無視する。
    pub fn set(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x >= 0 && y >= 0 && x < self.w as i32 && y < self.h as i32 {
            self.px[(y as u32 * self.w + x as u32) as usize] = color;
        }
    }

    /// 左右反転。
    pub fn flip_x(&self) -> Self {
        self.remap(|x, y, w, _| (w - 1 - x, y))
    }

    /// 上下反転。
    pub fn flip_y(&self) -> Self {
        self.remap(|x, y, _, h| (x, h - 1 - y))
    }

    /// 180 度回転。
    pub fn rot180(&self) -> Self {
        self.remap(|x, y, w, h| (w - 1 - x, h - 1 - y))
    }

    /// 色の置き換え(`from` と等しい画素を `to` にする)。
    pub fn recolor(&self, map: &[([u8; 4], [u8; 4])]) -> Self {
        let mut out = self.clone();
        for p in &mut out.px {
            if let Some((_, to)) = map.iter().find(|(from, _)| from == p) {
                *p = *to;
            }
        }
        out
    }

    fn remap(&self, f: impl Fn(u32, u32, u32, u32) -> (u32, u32)) -> Self {
        let mut out = Self::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                let (nx, ny) = f(x, y, self.w, self.h);
                out.px[(ny * self.w + nx) as usize] = self.px[(y * self.w + x) as usize];
            }
        }
        out
    }
}
