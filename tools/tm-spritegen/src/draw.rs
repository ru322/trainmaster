//! 手続き的なスプライト(線路、帯、枠、カーソル)。整数だけで描く。15 §5.7。

use crate::def::{Dir, Part};
use crate::error::SpriteError;
use crate::image::{CLEAR, Image};
use crate::palette::Palette;
use std::collections::BTreeSet;

type Px = BTreeSet<(i32, i32)>;

/// 9 分割の部品の接尾辞(左上、上、右上、左、中、右、左下、下、右下)。
pub const NINE_SUFFIXES: [&str; 9] = ["tl", "t", "tr", "l", "c", "r", "bl", "b", "br"];

fn bad(msg: &str) -> SpriteError {
    SpriteError::Syntax {
        file: "Proc".into(),
        message: msg.into(),
    }
}

fn bresenham(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x, mut y) = a;
    let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
    let (sx, sy) = (
        (b.0 - a.0).signum().max(0) * 2 - 1,
        (b.1 - a.1).signum().max(0) * 2 - 1,
    );
    let mut err = dx + dy;
    let mut out = Vec::new();
    loop {
        out.push((x, y));
        if (x, y) == b {
            return out;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// 半区間の線の両端(中心を 1px 越えた所からマスの境目を 2px 越えた所まで。15 §5.8 の 4.)。
fn endpoints(tile: i32, dir: Dir) -> ((i32, i32), (i32, i32)) {
    let (c, t) = (tile / 2 - 1, tile);
    match dir {
        Dir::E => ((c - 1, c), (t + 1, c)),
        Dir::W => ((c + 2, c), (-2, c)),
        Dir::N => ((c, c + 2), (c, -2)),
        Dir::S => ((c, c - 1), (c, t + 1)),
        Dir::NE => ((c - 2, c + 2), (t, -2)),
        Dir::SW => ((c + 1, c - 1), (-3, t + 1)),
        Dir::NW => ((c + 2, c + 2), (-2, -2)),
        Dir::SE => ((c - 1, c - 1), (t + 1, t + 1)),
    }
}

/// レーンのずれ(15 §5.8 の 2.)。
fn lane_shift(dir: Dir, lane: i32) -> (i32, i32) {
    let k = lane - 1;
    match dir {
        Dir::E | Dir::W => (0, 3 * k),
        Dir::N | Dir::S => (3 * k, 0),
        Dir::NE | Dir::SW => (2 * k, 2 * k),
        Dir::NW | Dir::SE => (2 * k, -2 * k),
    }
}

/// 線路か帯の半区間を、外接矩形で切り出した画像と基準点(マスの左上の画素がスプライトのどこに来るか)で返す。
pub fn track_half(
    tile: u32,
    dir: Dir,
    part: Part,
    pal: &Palette,
    name: &str,
) -> Result<(Image, (i32, i32)), SpriteError> {
    let (brush, color, lane, conv) = match part {
        Part::RailOutline => (6, pal.by_name("outline", name)?, 1, false),
        Part::RailFill => (4, pal.by_name("rail", name)?, 1, false),
        Part::BandOutline { lane, conv } => (4, pal.by_name("outline", name)?, lane, conv),
        Part::BandFill { lane, conv } => (2, pal.by_name("white", name)?, lane, conv),
    };
    if lane > 2 {
        return Err(bad("lane は 0〜2"));
    }
    let (start, end) = endpoints(tile as i32, dir);
    let sh = lane_shift(dir, lane as i32);
    let shifted = |p: (i32, i32)| (p.0 + sh.0, p.1 + sh.1);
    let line = if conv {
        bresenham(shifted(end), start)
    } else {
        bresenham(shifted(start), shifted(end))
    };
    let mut ink = Px::new();
    for (x, y) in line {
        for dx in -(brush / 2 - 1)..=brush / 2 {
            for dy in -(brush / 2 - 1)..=brush / 2 {
                ink.insert((x + dx, y + dy));
            }
        }
    }
    let x0 = ink.iter().map(|p| p.0).min().unwrap_or(0);
    let y0 = ink.iter().map(|p| p.1).min().unwrap_or(0);
    let w = ink.iter().map(|p| p.0).max().unwrap_or(0) - x0 + 1;
    let h = ink.iter().map(|p| p.1).max().unwrap_or(0) - y0 + 1;
    let mut img = Image::new(w as u32, h as u32);
    for (x, y) in ink {
        img.set(x - x0, y - y0, color);
    }
    Ok((img, (-x0, -y0)))
}

/// 9 分割の部品を `NINE_SUFFIXES` の順に返す。
pub fn nine_slice(
    corner: u32,
    border_w: u32,
    colors: [[u8; 4]; 3],
    name: &str,
) -> Result<Vec<Image>, SpriteError> {
    if corner < border_w + 2 {
        return Err(SpriteError::Slice {
            sprite: name.into(),
            reason: format!("corner {corner} は border_w {border_w} + 2 以上でなければならない"),
        });
    }
    let [fill, border, outline] = colors;
    let ring = |r: u32| {
        if r == 0 {
            outline
        } else if r <= border_w {
            border
        } else {
            fill
        }
    };
    let mut tl = Image::new(corner, corner);
    for y in 0..corner {
        for x in 0..corner {
            tl.px[(y * corner + x) as usize] = if x == 0 && y == 0 {
                CLEAR
            } else {
                ring(x.min(y))
            };
        }
    }
    let mut top = Image::new(1, corner);
    let mut left = Image::new(corner, 1);
    for i in 0..corner {
        top.px[i as usize] = ring(i);
        left.px[i as usize] = ring(i);
    }
    Ok(vec![
        tl.clone(),
        top.clone(),
        tl.flip_x(),
        left.clone(),
        Image::filled(1, 1, fill),
        left.flip_x(),
        tl.flip_y(),
        top.flip_y(),
        tl.rot180(),
    ])
}

/// マップのカーソルの枠。外側 1px の縁取り、内側 2px の芯、中は透明。
pub fn corner_frame(tile: u32, pal: &Palette, name: &str) -> Result<Image, SpriteError> {
    let n = tile + 6;
    let outline = pal.by_name("outline", name)?;
    let white = pal.by_name("white", name)?;
    let mut img = Image::new(n, n);
    for y in 0..n {
        for x in 0..n {
            let r = x.min(y).min(n - 1 - x).min(n - 1 - y);
            let color = match r {
                0 => outline,
                1 | 2 => white,
                _ => CLEAR,
            };
            img.px[(y * n + x) as usize] = color;
        }
    }
    Ok(img)
}
