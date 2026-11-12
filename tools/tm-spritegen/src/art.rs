//! 倍率を決める前の絵(ドット絵か図形)と、反転。描くときに倍率を決める。

use crate::image::Image;
use crate::upscale;
use crate::vector::Vector;

/// 絵の元。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Art {
    /// パレットの色のドット絵(設計の 1px = 1 画素)。拡大するときは滑らかにする。
    Grid(Image),
    /// 図形。
    Vector(Vector),
}

/// 反転(左右、上下)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flip {
    /// 左右反転。
    pub x: bool,
    /// 上下反転。
    pub y: bool,
}

/// 反転つきの絵。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawing {
    /// 元。
    pub art: Art,
    /// 描いた後の反転。
    pub flip: Flip,
}

impl Drawing {
    /// 反転なしの絵。
    pub fn new(art: Art) -> Drawing {
        Drawing {
            art,
            flip: Flip::default(),
        }
    }

    /// 設計の大きさ(px)。
    pub fn size(&self) -> (u32, u32) {
        match &self.art {
            Art::Grid(img) => (img.w, img.h),
            Art::Vector(v) => (v.w, v.h),
        }
    }

    /// 設計の 1px を `r` 画素にして描く。等倍(`r` = 1)はドット絵と同じく画素の中心だけで描く(アンチエイリアスなし)。
    pub fn render(&self, r: u32) -> Image {
        let img = match &self.art {
            Art::Grid(img) if r == 1 => img.clone(),
            Art::Grid(img) => upscale::smooth(img, r),
            Art::Vector(v) if r == 1 => v.render_samples(1, 1),
            Art::Vector(v) => v.render(r),
        };
        match (self.flip.x, self.flip.y) {
            (false, false) => img,
            (true, false) => img.flip_x(),
            (false, true) => img.flip_y(),
            (true, true) => img.rot180(),
        }
    }

    /// 反転を重ねる。
    pub fn flipped(&self, x: bool, y: bool) -> Drawing {
        Drawing {
            art: self.art.clone(),
            flip: Flip {
                x: self.flip.x ^ x,
                y: self.flip.y ^ y,
            },
        }
    }

    /// 画面で反時計回りに 45 度 × `n` 回す。反転した絵は回せない。ドット絵は 90 度の倍数だけ。
    pub fn rotated(&self, n: u8) -> Result<Drawing, &'static str> {
        if self.flip != Flip::default() {
            return Err("反転した絵は回せない");
        }
        let art = match &self.art {
            Art::Grid(img) => {
                if n % 2 != 0 {
                    return Err("ドット絵は 90 度の倍数だけ回せる");
                }
                let mut out = img.clone();
                for _ in 0..(n / 2) % 4 {
                    out = out.rot90();
                }
                Art::Grid(out)
            }
            Art::Vector(v) => Art::Vector(crate::vector::Vector {
                rot: (v.rot + n) % 8,
                ..v.clone()
            }),
        };
        Ok(Drawing::new(art))
    }

    /// 色の置き換え(反転とは独立)。
    pub fn recolor(&self, map: &[([u8; 4], [u8; 4])]) -> Drawing {
        let art = match &self.art {
            Art::Grid(img) => Art::Grid(img.recolor(map)),
            Art::Vector(v) => Art::Vector(v.recolor(map)),
        };
        Drawing {
            art,
            flip: self.flip,
        }
    }
}
