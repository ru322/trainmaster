//! 手続き的なスプライト(線路、帯、枠、カーソル)を図形で作る。整数だけで計算する。15 §5.7、§5.8。

use crate::def::{Dir, Part};
use crate::error::SpriteError;
use crate::image::CLEAR;
use crate::palette::Palette;
use crate::vector::{Clip, FX, Paint, Shape, Vector, convex_hull};

/// 9 分割の部品の接尾辞(左上、上、右上、左、中、右、左下、下、右下)。
pub const NINE_SUFFIXES: [&str; 9] = ["tl", "t", "tr", "l", "c", "r", "bl", "b", "br"];

fn bad(msg: &str) -> SpriteError {
    SpriteError::Syntax {
        file: "Proc".into(),
        message: msg.into(),
    }
}

fn step(dir: Dir) -> (i64, i64) {
    match dir {
        Dir::E => (1, 0),
        Dir::NE => (1, -1),
        Dir::N => (0, -1),
        Dir::NW => (-1, -1),
        Dir::W => (-1, 0),
        Dir::SW => (-1, 1),
        Dir::S => (0, 1),
        Dir::SE => (1, 1),
    }
}

/// 線路と帯の四角いペンの太さ(設計の px。線路の縁、線路の中身、帯の縁、帯の中身。15 §5.8)。`sprites.rs` に出し、UI もこれを使う。
pub const TRACK_PENS: [i64; 4] = [6, 4, 4, 2];
/// 帯のレーンのずれ(設計の px。縦横の辺、斜めの辺の x と y。15 §5.8 の 2.)。
pub const TRACK_LANES: [i64; 2] = [3, 2];
const _: () = assert!(
    TRACK_PENS[0] % 2 == 0
        && TRACK_PENS[1] % 2 == 0
        && TRACK_PENS[2] % 2 == 0
        && TRACK_PENS[3] % 2 == 0,
    "ペンは中心の両側に半分ずつなので偶数"
);

/// レーンのずれ(設計の px)。
fn lane_shift(dir: Dir, lane: i64) -> (i64, i64) {
    let k = lane - 1;
    let (s, d) = (TRACK_LANES[0] * k, TRACK_LANES[1] * k);
    match dir {
        Dir::E | Dir::W => (0, s),
        Dir::N | Dir::S => (s, 0),
        Dir::NE | Dir::SW => (d, d),
        Dir::NW | Dir::SE => (d, -d),
    }
}

/// 線路か帯の半区間: マスの中心から辺の中点(斜めは角)までの線分を、四角いペン(軸にそろった正方形)でなぞった形(15 §5.8 の 4.)。
/// 帯はレーンのずれを足し、集める版は中心の側をずらさない。外接矩形(線分 ± ペンの半分)をそのまま絵の大きさにし、
/// 基準点(マスの左上がスプライトのどこに来るか)と返す。隣の半区間とは端の正方形だけが重なる(斜めの縁は重ならない)。
pub fn track_half(
    tile: u32,
    dir: Dir,
    part: Part,
    pal: &Palette,
    name: &str,
) -> Result<(Vector, (i32, i32)), SpriteError> {
    let (brush, color, lane, conv) = match part {
        Part::RailOutline => (TRACK_PENS[0], pal.by_name("outline", name)?, 1, false),
        Part::RailFill => (TRACK_PENS[1], pal.by_name("rail", name)?, 1, false),
        Part::BandOutline { lane, conv } => {
            (TRACK_PENS[2], pal.by_name("outline", name)?, lane, conv)
        }
        Part::BandFill { lane, conv } => (TRACK_PENS[3], pal.by_name("white", name)?, lane, conv),
    };
    if lane > 2 {
        return Err(bad("lane は 0〜2"));
    }
    let half = i64::from(tile) / 2;
    let (sx, sy) = step(dir);
    let sh = lane_shift(dir, i64::from(lane));
    let from = if conv {
        (half, half)
    } else {
        (half + sh.0, half + sh.1)
    };
    let to = (half + sx * half + sh.0, half + sy * half + sh.1);
    let p = brush / 2;
    let corners: Vec<(i64, i64)> = [from, to]
        .iter()
        .flat_map(|&(x, y)| {
            [(-p, -p), (p, -p), (p, p), (-p, p)].map(|(dx, dy)| ((x + dx) * FX, (y + dy) * FX))
        })
        .collect();
    let (x0, y0) = (from.0.min(to.0) - p, from.1.min(to.1) - p);
    let (x1, y1) = (from.0.max(to.0) + p, from.1.max(to.1) + p);
    let v = Vector {
        w: (x1 - x0) as u32,
        h: (y1 - y0) as u32,
        origin: (x0 * FX, y0 * FX),
        paints: vec![Paint {
            shape: Shape::Convex(if part == Part::RailFill && sx != 0 && sy != 0 {
                shave_long_edges(convex_hull(corners), sx * sy, RAIL_FILL_SHAVE)
            } else {
                convex_hull(corners)
            }),
            color,
            clip: Clip::ALL,
        }],
        rot: 0,
    };
    Ok((v, (-x0 as i32, -y0 as i32)))
}

/// 斜めの線路の中身を、長い辺に沿って内側へ削る量(1/`FX` px、`x ± y` の値で)。帯の縁(同じ太さの黒)が
/// 中身のアンチエイリアスの縁を覆いきるように、垂直に約 0.27 px(マップで 1.6 画素)。端の正方形は削らない。
const RAIL_FILL_SHAVE: i64 = 6;

/// 斜め(`slope` が −1 なら北東・南西、1 なら北西・南東)の六角形の長い辺(`x + y` か `x − y` が一定)だけを `d` 内側へ寄せる。
fn shave_long_edges(poly: Vec<(i64, i64)>, slope: i64, d: i64) -> Vec<(i64, i64)> {
    let k = |p: (i64, i64)| p.0 - slope * p.1;
    let lo = poly.iter().map(|&p| k(p)).min().unwrap_or(0) + d;
    let hi = poly.iter().map(|&p| k(p)).max().unwrap_or(0) - d;
    let cut = |poly: Vec<(i64, i64)>, keep: &dyn Fn(i64) -> bool, at: i64| {
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (ka, kb) = (k(a), k(b));
            if keep(ka) {
                out.push(a);
            }
            if keep(ka) != keep(kb) {
                let t = |u: i64, v: i64| {
                    let (mut n, mut dd) = ((v - u) * (at - ka), kb - ka);
                    if dd < 0 {
                        (n, dd) = (-n, -dd);
                    }
                    u + (2 * n + dd).div_euclid(2 * dd)
                };
                out.push((t(a.0, b.0), t(a.1, b.1)));
            }
        }
        out
    };
    let poly = cut(poly, &|v| v >= lo, lo);
    cut(poly, &|v| v <= hi, hi)
}

fn rect(x0: i64, y0: i64, x1: i64, y1: i64, r: i64, color: [u8; 4]) -> Paint {
    Paint {
        shape: Shape::RoundRect { x0, y0, x1, y1, r },
        color,
        clip: Clip::ALL,
    }
}

/// 9 分割の部品を `NINE_SUFFIXES` の順に返す。外側から `outline` 1px、`border` を `border_w` px、残りを `fill`。角は丸める。
pub fn nine_slice(
    corner: u32,
    border_w: u32,
    colors: [[u8; 4]; 3],
    name: &str,
) -> Result<Vec<Vector>, SpriteError> {
    if corner < border_w + 2 {
        return Err(SpriteError::Slice {
            sprite: name.into(),
            reason: format!("corner {corner} は border_w {border_w} + 2 以上でなければならない"),
        });
    }
    let [fill, border, outline] = colors;
    let (c, bw) = (i64::from(corner), i64::from(border_w));
    let n = 3 * c * FX;
    let round = 2 * FX;
    let mut paints = vec![rect(0, 0, n, n, round, outline)];
    if bw > 0 {
        paints.push(rect(FX, FX, n - FX, n - FX, (round - FX).max(0), border));
    }
    let inset = (1 + bw) * FX;
    paints.push(rect(
        inset,
        inset,
        n - inset,
        n - inset,
        (round - inset).max(0),
        fill,
    ));
    let cols = [(0, c), (c, 1), (2 * c, c)];
    let mut out = Vec::with_capacity(9);
    for &(y, h) in &cols {
        for &(x, w) in &cols {
            out.push(Vector {
                w: w as u32,
                h: h as u32,
                origin: (x * FX, y * FX),
                paints: paints.clone(),
                rot: 0,
            });
        }
    }
    Ok(out)
}

/// マップのカーソルの枠((tile + 6) 四方)。外側 1px の縁取り、内側 2px の芯、中は透明。角は丸める。
pub fn corner_frame(tile: u32, pal: &Palette, name: &str) -> Result<Vector, SpriteError> {
    let n = i64::from(tile + 6);
    let outline = pal.by_name("outline", name)?;
    let white = pal.by_name("white", name)?;
    let f = n * FX;
    Ok(Vector {
        w: n as u32,
        h: n as u32,
        origin: (0, 0),
        paints: vec![
            rect(0, 0, f, f, 2 * FX, outline),
            rect(FX, FX, f - FX, f - FX, FX, white),
            rect(3 * FX, 3 * FX, f - 3 * FX, f - 3 * FX, 0, CLEAR),
        ],
        rot: 0,
    })
}

/// 1 色の矩形。
pub fn fill(w: u32, h: u32, color: [u8; 4]) -> Vector {
    Vector {
        w,
        h,
        origin: (0, 0),
        paints: vec![rect(0, 0, i64::from(w) * FX, i64::from(h) * FX, 0, color)],
        rot: 0,
    }
}

/// 定義の図形を、パレットの色の図形にする。
pub fn shapes(
    w: u32,
    h: u32,
    defs: &[crate::def::ShapeDef],
    pal: &Palette,
    name: &str,
) -> Result<Vector, SpriteError> {
    use crate::def::ShapeDef;
    let mut paints = Vec::with_capacity(defs.len());
    for d in defs {
        let (shape, color) = match d {
            ShapeDef::Rect {
                x,
                y,
                w,
                h,
                r,
                color,
            } => (
                Shape::RoundRect {
                    x0: x.0,
                    y0: y.0,
                    x1: x.0 + w.0,
                    y1: y.0 + h.0,
                    r: r.0,
                },
                color,
            ),
            ShapeDef::Line {
                from,
                to,
                width,
                color,
            } => (
                Shape::Capsule {
                    a: (from.0.0, from.1.0),
                    b: (to.0.0, to.1.0),
                    r: width.0 / 2,
                },
                color,
            ),
            ShapeDef::Circle { x, y, r, color } => (
                Shape::Capsule {
                    a: (x.0, y.0),
                    b: (x.0, y.0),
                    r: r.0,
                },
                color,
            ),
            ShapeDef::Poly { pts, color } => {
                if pts.len() < 3 {
                    return Err(bad("Poly は 3 点以上"));
                }
                (
                    Shape::Polygon(pts.iter().map(|p| (p.0.0, p.1.0)).collect()),
                    color,
                )
            }
        };
        paints.push(Paint {
            shape,
            color: pal.by_name(color, name)?,
            clip: Clip::ALL,
        });
    }
    if w == 0 || h == 0 {
        return Err(bad("Shapes の寸法が 0"));
    }
    Ok(Vector {
        w,
        h,
        origin: (0, 0),
        paints,
        rot: 0,
    })
}
