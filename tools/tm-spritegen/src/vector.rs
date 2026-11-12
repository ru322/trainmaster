//! 設計の座標(`FX` 分の 1 px の固定小数点)の図形を、標本を数えるアンチエイリアスで描く。整数だけで計算する。

use crate::image::{CLEAR, Image};

/// 設計の 1px の分割数(図形の座標の単位は 1/`FX` px)。
pub const FX: i64 = 16;
/// 描画の 1 画素あたりの縦横の標本の数。
pub const SAMPLES: i64 = 8;

/// 図形。座標は 1/`FX` px。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// 角を半径 `r` で丸めた矩形 `[x0, x1) × [y0, y1)`。
    RoundRect {
        /// 左。
        x0: i64,
        /// 上。
        y0: i64,
        /// 右(含まない)。
        x1: i64,
        /// 下(含まない)。
        y1: i64,
        /// 角の半径。
        r: i64,
    },
    /// `a` から `b` への両端の丸い太い線。`r` は太さの半分。
    Capsule {
        /// 始点。
        a: (i64, i64),
        /// 終点。
        b: (i64, i64),
        /// 太さの半分。
        r: i64,
    },
    /// 多角形(偶奇規則)。
    Polygon(Vec<(i64, i64)>),
    /// 凸多角形。辺の上は含まない。
    Convex(Vec<(i64, i64)>),
}

/// 点の凸包(辺の上の点は除く)。
pub fn convex_hull(mut pts: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    pts.sort_unstable();
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: (i64, i64), a: (i64, i64), b: (i64, i64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut hull: Vec<(i64, i64)> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &(i64, i64)>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// 軸に平行な切り取り `[x0, x1) × [y0, y1)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clip {
    /// 左。
    pub x0: i64,
    /// 上。
    pub y0: i64,
    /// 右(含まない)。
    pub x1: i64,
    /// 下(含まない)。
    pub y1: i64,
}

impl Clip {
    /// 切り取らない。
    pub const ALL: Clip = Clip {
        x0: i64::MIN / 4,
        y0: i64::MIN / 4,
        x1: i64::MAX / 4,
        y1: i64::MAX / 4,
    };
}

/// 色を付けた図形 1 つ。後に描いたものが標本の色を置き換える(半透明の色は重ねる)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paint {
    /// 図形。
    pub shape: Shape,
    /// 色。
    pub color: [u8; 4],
    /// 切り取り。
    pub clip: Clip,
}

/// 図形で描く絵。`w × h`(設計の px)の範囲を、設計の座標の `origin`(1/`FX` px)から切り出す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vector {
    /// 幅(設計の px)。
    pub w: u32,
    /// 高さ(設計の px)。
    pub h: u32,
    /// 絵の左上の設計の座標。
    pub origin: (i64, i64),
    /// 図形(描く順)。
    pub paints: Vec<Paint>,
    /// 絵の中心のまわりの回転(45 度 × `rot`、画面で反時計回り)。
    pub rot: u8,
}

/// cos 45° の 16 ビットの固定小数点。
const COS45_Q16: i64 = 46_341;

/// 画面で反時計回りに 45° × `rot` 回した点を元に戻す(`d` は中心からのずれ)。
fn unrotate(d: (i64, i64), rot: u8) -> (i64, i64) {
    let (x, y) = d;
    let r90 = |(x, y): (i64, i64), n: u8| match n % 4 {
        0 => (x, y),
        1 => (-y, x),
        2 => (-x, -y),
        _ => (y, -x),
    };
    let rot = rot % 8;
    let (x, y) = r90((x, y), rot / 2);
    if rot % 2 == 0 {
        return (x, y);
    }
    let q = |v: i64| (v * COS45_Q16 + (1 << 15)) >> 16;
    (q(x - y), q(x + y))
}

/// 図形の外接矩形(1/`FX` px、端を含まない)。
fn bounds(s: &Shape) -> (i64, i64, i64, i64) {
    match s {
        Shape::RoundRect { x0, y0, x1, y1, .. } => (*x0, *y0, *x1, *y1),
        Shape::Capsule { a, b, r } => (
            a.0.min(b.0) - r,
            a.1.min(b.1) - r,
            a.0.max(b.0) + r,
            a.1.max(b.1) + r,
        ),
        Shape::Polygon(p) | Shape::Convex(p) => (
            p.iter().map(|q| q.0).min().unwrap_or(0),
            p.iter().map(|q| q.1).min().unwrap_or(0),
            p.iter().map(|q| q.0).max().unwrap_or(0) + 1,
            p.iter().map(|q| q.1).max().unwrap_or(0) + 1,
        ),
    }
}

/// `paints` の外接矩形(1/`FX` px)。切り取りも考える。何もなければ `None`。
pub fn ink_bounds(paints: &[Paint]) -> Option<(i64, i64, i64, i64)> {
    paints
        .iter()
        .filter(|p| p.color[3] != 0)
        .map(|p| {
            let b = bounds(&p.shape);
            (
                b.0.max(p.clip.x0),
                b.1.max(p.clip.y0),
                b.2.min(p.clip.x1),
                b.3.min(p.clip.y1),
            )
        })
        .filter(|b| b.0 < b.2 && b.1 < b.3)
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

/// 標本の座標の単位(1/`FX`/`k` px)での図形の判定。
struct Scaled {
    shape: Shape,
    clip: Clip,
    color: [u8; 4],
    bb: (i64, i64, i64, i64),
}

fn scale_shape(s: &Shape, k: i64) -> Shape {
    let p = |q: (i64, i64)| (q.0 * k, q.1 * k);
    match s {
        Shape::RoundRect { x0, y0, x1, y1, r } => Shape::RoundRect {
            x0: x0 * k,
            y0: y0 * k,
            x1: x1 * k,
            y1: y1 * k,
            r: r * k,
        },
        Shape::Capsule { a, b, r } => Shape::Capsule {
            a: p(*a),
            b: p(*b),
            r: r * k,
        },
        Shape::Polygon(v) => Shape::Polygon(v.iter().map(|&q| p(q)).collect()),
        Shape::Convex(v) => Shape::Convex(v.iter().map(|&q| p(q)).collect()),
    }
}

fn contains(s: &Shape, x: i64, y: i64) -> bool {
    match *s {
        Shape::RoundRect { x0, y0, x1, y1, r } => {
            if x < x0 || x >= x1 || y < y0 || y >= y1 {
                return false;
            }
            let cx = x.clamp(x0 + r, x1 - r);
            let cy = y.clamp(y0 + r, y1 - r);
            let (dx, dy) = (i128::from(x - cx), i128::from(y - cy));
            dx * dx + dy * dy <= i128::from(r) * i128::from(r)
        }
        Shape::Capsule { a, b, r } => {
            let (dx, dy) = (i128::from(b.0 - a.0), i128::from(b.1 - a.1));
            let (px, py) = (i128::from(x - a.0), i128::from(y - a.1));
            let rr = i128::from(r) * i128::from(r);
            let len2 = dx * dx + dy * dy;
            let t = px * dx + py * dy;
            if len2 == 0 || t <= 0 {
                return px * px + py * py < rr;
            }
            if t >= len2 {
                let (qx, qy) = (i128::from(x - b.0), i128::from(y - b.1));
                return qx * qx + qy * qy < rr;
            }
            let cross = px * dy - py * dx;
            cross * cross < rr * len2
        }
        Shape::Convex(ref v) => {
            let mut sign = 0i128;
            for i in 0..v.len() {
                let (a, b) = (v[i], v[(i + 1) % v.len()]);
                let c = i128::from(b.0 - a.0) * i128::from(y - a.1)
                    - i128::from(b.1 - a.1) * i128::from(x - a.0);
                if c == 0 || (sign != 0 && (c > 0) != (sign > 0)) {
                    return false;
                }
                sign = c;
            }
            v.len() >= 3
        }
        Shape::Polygon(ref v) => {
            let mut inside = false;
            for i in 0..v.len() {
                let (a, b) = (v[i], v[(i + 1) % v.len()]);
                if (a.1 <= y) != (b.1 <= y) {
                    let lhs = i128::from(x - a.0) * i128::from(b.1 - a.1);
                    let rhs = i128::from(y - a.1) * i128::from(b.0 - a.0);
                    if (b.1 > a.1 && lhs < rhs) || (b.1 < a.1 && lhs > rhs) {
                        inside = !inside;
                    }
                }
            }
            inside
        }
    }
}

/// `over` を `under` の上に重ねる(ストレートアルファ)。
fn over(top: [u8; 4], under: [u8; 4]) -> [u8; 4] {
    let (ta, ua) = (u32::from(top[3]), u32::from(under[3]));
    let a = ta * 255 + ua * (255 - ta);
    if a == 0 {
        return CLEAR;
    }
    let mut out = [0u8; 4];
    for k in 0..3 {
        let v = u32::from(top[k]) * ta * 255 + u32::from(under[k]) * ua * (255 - ta);
        out[k] = ((v + a / 2) / a) as u8;
    }
    out[3] = ((a + 127) / 255) as u8;
    out
}

/// 標本の色の和(アルファを掛けた RGB とアルファ)から 1 画素の色。
pub(crate) fn resolve_samples(sum: [u64; 4], n: u64) -> [u8; 4] {
    if sum[3] == 0 {
        return CLEAR;
    }
    let a = sum[3];
    let c = |v: u64| ((v + a / 2) / a) as u8;
    [c(sum[0]), c(sum[1]), c(sum[2]), ((a + n / 2) / n) as u8]
}

pub(crate) fn add_sample(sum: &mut [u64; 4], c: [u8; 4]) {
    let a = u64::from(c[3]);
    for k in 0..3 {
        sum[k] += u64::from(c[k]) * a;
    }
    sum[3] += a;
}

impl Vector {
    /// 設計の 1px を `r` 画素にして描く。大きさは `(w·r) × (h·r)`。
    pub fn render(&self, r: u32) -> Image {
        self.render_samples(r, SAMPLES)
    }

    /// `render` の標本の数を `samples`²(1 なら画素の中心だけ)にしたもの。
    pub fn render_samples(&self, r: u32, samples: i64) -> Image {
        let r = i64::from(r);
        let k = 2 * samples * r;
        let scaled: Vec<Scaled> = self
            .paints
            .iter()
            .map(|p| {
                let b = bounds(&p.shape);
                Scaled {
                    shape: scale_shape(&p.shape, k),
                    clip: Clip {
                        x0: p.clip.x0.saturating_mul(k),
                        y0: p.clip.y0.saturating_mul(k),
                        x1: p.clip.x1.saturating_mul(k),
                        y1: p.clip.y1.saturating_mul(k),
                    },
                    color: p.color,
                    bb: (
                        b.0.max(p.clip.x0) * k,
                        b.1.max(p.clip.y0) * k,
                        b.2.min(p.clip.x1) * k,
                        b.3.min(p.clip.y1) * k,
                    ),
                }
            })
            .collect();
        let (w, h) = (self.w as i64 * r, self.h as i64 * r);
        let mut img = Image::new(w as u32, h as u32);
        let center = (
            self.origin.0 * k + self.w as i64 * FX * k / 2,
            self.origin.1 * k + self.h as i64 * FX * k / 2,
        );
        let n = (samples * samples) as u64;
        for py in 0..h {
            for px in 0..w {
                let x_lo = self.origin.0 * k + px * samples * 2 * FX;
                let y_lo = self.origin.1 * k + py * samples * 2 * FX;
                let x_hi = x_lo + samples * 2 * FX;
                let y_hi = y_lo + samples * 2 * FX;
                let near: Vec<&Scaled> = if self.rot % 8 == 0 {
                    scaled
                        .iter()
                        .filter(|s| {
                            s.bb.0 < x_hi && x_lo < s.bb.2 && s.bb.1 < y_hi && y_lo < s.bb.3
                        })
                        .collect()
                } else {
                    scaled.iter().collect()
                };
                if near.is_empty() {
                    continue;
                }
                let mut sum = [0u64; 4];
                for j in 0..samples {
                    let y = y_lo + (2 * j + 1) * FX;
                    for i in 0..samples {
                        let x = x_lo + (2 * i + 1) * FX;
                        let (x, y) = if self.rot % 8 == 0 {
                            (x, y)
                        } else {
                            let d = unrotate((x - center.0, y - center.1), self.rot);
                            (center.0 + d.0, center.1 + d.1)
                        };
                        let mut c = CLEAR;
                        for s in &near {
                            let inside = x >= s.clip.x0
                                && x < s.clip.x1
                                && y >= s.clip.y0
                                && y < s.clip.y1
                                && contains(&s.shape, x, y);
                            if inside {
                                c = if s.color[3] == 255 || s.color[3] == 0 {
                                    s.color
                                } else {
                                    over(s.color, c)
                                };
                            }
                        }
                        add_sample(&mut sum, c);
                    }
                }
                img.set(px as i32, py as i32, resolve_samples(sum, n));
            }
        }
        img
    }

    /// 色の置き換え。
    pub fn recolor(&self, map: &[([u8; 4], [u8; 4])]) -> Vector {
        let mut out = self.clone();
        for p in &mut out.paints {
            if let Some((_, to)) = map.iter().find(|(from, _)| *from == p.color) {
                p.color = *to;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 4] = [255, 0, 0, 255];

    fn one(shape: Shape, w: u32, h: u32) -> Vector {
        Vector {
            w,
            h,
            origin: (0, 0),
            paints: vec![Paint {
                shape,
                color: RED,
                clip: Clip::ALL,
            }],
            rot: 0,
        }
    }

    #[test]
    fn rotation_by_90_is_exact_and_by_45_keeps_the_center() {
        let bar = one(
            Shape::RoundRect {
                x0: 8 * FX,
                y0: 7 * FX,
                x1: 15 * FX,
                y1: 9 * FX,
                r: 0,
            },
            16,
            16,
        );
        let e = bar.render(3);
        let n = Vector {
            rot: 2,
            ..bar.clone()
        }
        .render(3);
        for y in 0..48 {
            for x in 0..48 {
                assert_eq!(n.get(x, y), e.get(47 - y, x), "90 度 ({x}, {y})");
            }
        }
        let ne = Vector { rot: 1, ..bar }.render(3);
        assert_eq!(ne.get(33, 14)[3], 255, "北東へ伸びる");
        assert_eq!(ne.get(33, 33)[3], 0);
    }

    #[test]
    fn pixel_aligned_rect_has_hard_edges() {
        let v = one(
            Shape::RoundRect {
                x0: FX,
                y0: FX,
                x1: 3 * FX,
                y1: 2 * FX,
                r: 0,
            },
            4,
            3,
        );
        let img = v.render(3);
        assert_eq!((img.w, img.h), (12, 9));
        for y in 0..9 {
            for x in 0..12 {
                let inside = (3..9).contains(&x) && (3..6).contains(&y);
                assert_eq!(
                    img.get(x, y),
                    if inside { RED } else { CLEAR },
                    "({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn diagonal_edges_are_antialiased_and_symmetric() {
        let v = one(
            Shape::Capsule {
                a: (2 * FX, 2 * FX),
                b: (14 * FX, 14 * FX),
                r: FX,
            },
            16,
            16,
        );
        let img = v.render(6);
        let partial = img.px.iter().filter(|p| p[3] != 0 && p[3] != 255).count();
        assert!(partial > 100, "{partial}");
        for y in 0..img.h as i32 {
            for x in 0..img.w as i32 {
                assert_eq!(img.get(x, y), img.get(y, x), "対角で対称 ({x}, {y})");
            }
        }
        assert_eq!(img.get(48, 48), RED);
    }

    #[test]
    fn clip_cuts_on_pixel_boundaries_and_later_paints_replace() {
        let mut v = one(
            Shape::RoundRect {
                x0: 0,
                y0: 0,
                x1: 4 * FX,
                y1: 4 * FX,
                r: 0,
            },
            4,
            4,
        );
        v.paints[0].clip = Clip {
            x1: 2 * FX,
            ..Clip::ALL
        };
        v.paints.push(Paint {
            shape: Shape::Polygon(vec![(0, 0), (FX, 0), (FX, FX), (0, FX)]),
            color: CLEAR,
            clip: Clip::ALL,
        });
        let img = v.render(3);
        assert_eq!(img.get(5, 5), RED);
        assert_eq!(img.get(6, 5), CLEAR, "切り取りの境目は画素の境目");
        assert_eq!(img.get(1, 1), CLEAR, "後の透明で置き換える");
        assert_eq!(ink_bounds(&v.paints), Some((0, 0, 2 * FX, 4 * FX)));
    }
}
