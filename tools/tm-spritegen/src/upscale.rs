//! ドット絵を滑らかに拡大する。Scale2x(EPX)で斜めの段をつないでから、標本を数えて縮め、境目にアンチエイリアスを付ける。

use crate::image::Image;
use crate::vector::{SAMPLES, add_sample, resolve_samples};

/// 範囲外は透明として、Scale2x で 2 倍にする。
pub fn scale2x(src: &Image) -> Image {
    let (w, h) = (src.w as i32, src.h as i32);
    let mut out = Image::new(src.w * 2, src.h * 2);
    for y in 0..h {
        for x in 0..w {
            let p = src.get(x, y);
            let a = src.get(x, y - 1);
            let b = src.get(x + 1, y);
            let c = src.get(x - 1, y);
            let d = src.get(x, y + 1);
            let e0 = if c == a && c != d && a != b { a } else { p };
            let e1 = if a == b && a != c && b != d { b } else { p };
            let e2 = if d == c && d != b && c != a { c } else { p };
            let e3 = if b == d && b != a && d != c { d } else { p };
            out.set(2 * x, 2 * y, e0);
            out.set(2 * x + 1, 2 * y, e1);
            out.set(2 * x, 2 * y + 1, e2);
            out.set(2 * x + 1, 2 * y + 1, e3);
        }
    }
    out
}

/// ドット絵の 1px を `r` 画素にする。`r` の 4/3 倍以上になるまで Scale2x を重ね、1 画素あたり `SAMPLES`² の標本で縮める。
pub fn smooth(src: &Image, r: u32) -> Image {
    let mut g = 1u32;
    let mut grid = src.clone();
    while 3 * g < 4 * r {
        grid = scale2x(&grid);
        g *= 2;
    }
    let (w, h) = (src.w * r, src.h * r);
    let mut out = Image::new(w, h);
    let (s, r64, g64) = (SAMPLES, i64::from(r), i64::from(g));
    let n = (s * s) as u64;
    for py in 0..i64::from(h) {
        for px in 0..i64::from(w) {
            let mut sum = [0u64; 4];
            for j in 0..s {
                let gy = ((2 * (py * s + j) + 1) * g64) / (2 * s * r64);
                for i in 0..s {
                    let gx = ((2 * (px * s + i) + 1) * g64) / (2 * s * r64);
                    add_sample(&mut sum, grid.get(gx as i32, gy as i32));
                }
            }
            out.set(px as i32, py as i32, resolve_samples(sum, n));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: [u8; 4] = [0, 0, 0, 255];
    const W: [u8; 4] = [255, 255, 255, 255];

    fn grid(rows: &[&str]) -> Image {
        let mut img = Image::new(rows[0].len() as u32, rows.len() as u32);
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                img.set(x as i32, y as i32, if ch == 'K' { K } else { W });
            }
        }
        img
    }

    #[test]
    fn flat_areas_and_straight_edges_stay_sharp() {
        let img = smooth(&grid(&["KKKWWW"; 4]), 3);
        assert_eq!((img.w, img.h), (18, 12));
        for y in 0..12 {
            for x in 0..18 {
                let corner = !(3..=14).contains(&x) && !(3..=8).contains(&y);
                if !corner {
                    assert_eq!(img.get(x, y), if x < 9 { K } else { W }, "({x}, {y})");
                }
            }
        }
        assert!(img.get(0, 0)[3] < 255, "絵の外は透明なので角は丸まる");
    }

    #[test]
    fn diagonal_steps_are_joined_and_softened() {
        let src = grid(&["KWWW", "WKWW", "WWKW", "WWWK"]);
        let img = smooth(&src, 6);
        let gray = img.px.iter().filter(|p| p[0] != 0 && p[0] != 255).count();
        assert!(gray > 0, "斜めの境目に中間の色");
        for k in 6..18 {
            assert_eq!(img.get(k, k), K, "斜めの線がつながる k={k}");
        }
        assert!(img.get(0, 0)[3] < 255, "絵の外は透明なので端は丸まる");
        for y in 0..24 {
            for x in 0..24 {
                assert_eq!(img.get(x, y), img.get(y, x));
            }
        }
    }

    #[test]
    fn isolated_pixels_survive() {
        let img = smooth(&grid(&["WWW", "WKW", "WWW"]), 3);
        assert_eq!(img.get(4, 4), K);
    }
}
