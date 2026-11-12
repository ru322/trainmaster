//! 人の目視用の画像(`preview` と `proof`。15 §4.1、AS-V-04)。

use crate::Generated;
use crate::emit::Image;
use crate::kanji::is_kanji;
use crate::raster::Face;
use crate::text::Entry;

/// パネルの地の色(13 §3.11)。画像の背景。
pub const PREVIEW_BG: [u8; 3] = [0x1E, 0x2A, 0x4A];
/// 文字の色(白)。
pub const TEXT_COLOR: [u8; 3] = [0xFF, 0xFF, 0xFF];
/// キーと見出しの色(使えない文字の色。13 §3.11)。
pub const LABEL_COLOR: [u8; 3] = [0xA6, 0xAE, 0xC0];
/// 漢字の一覧の枠の色。
pub const GRID_COLOR: [u8; 3] = [0x3A, 0x46, 0x6A];
/// `proof` の 1 頁の大きさ(論理解像度 640 × 360。画像は描画の倍率をかけた大きさ)。
pub const PAGE: (u32, u32) = (640, 360);
/// `proof` の頁の余白。
pub const MARGIN: u32 = 4;
/// 漢字の一覧の 1 行の字数。
pub const KANJI_COLUMNS: u32 = 20;

fn line_width(face: &Face, line: &str) -> u32 {
    let fallback = face.glyph('?');
    line.chars()
        .filter_map(|c| face.glyph(c).or(fallback))
        .map(|g| g.advance)
        .sum()
}

/// 1 行を 15 §4.10 の規則で描く(ペンは論理 px の整数、フォントにない字は「?」)。`x` と `baseline` は論理 px。
fn draw_line(img: &mut Image, face: &Face, x: i32, baseline: i32, line: &str, color: [u8; 3]) {
    let fallback = face.glyph('?');
    let k = face.scale as i32;
    let baseline = baseline * k;
    let mut pen = x * k;
    for c in line.chars() {
        let Some(gl) = face.glyph(c).or(fallback) else {
            continue;
        };
        if let Some(bm) = &gl.bitmap {
            for by in 0..bm.h {
                for bx in 0..bm.w {
                    let (px, py) = (pen + bm.x_off + bx as i32, baseline + bm.y_off + by as i32);
                    if px < 0 || py < 0 {
                        continue;
                    }
                    let a = u32::from(bm.alpha[(by * bm.w + bx) as usize]);
                    let mut p = img.get(px as u32, py as u32);
                    for (v, &c) in p.iter_mut().zip(&color) {
                        *v = ((u32::from(c) * a + u32::from(*v) * (255 - a)) / 255) as u8;
                    }
                    img.set(px as u32, py as u32, p);
                }
            }
        }
        pen += gl.advance as i32 * k;
    }
}

/// 論理 px の大きさ `w × h` の地(画像は描画の倍率 `k` 倍)。
fn background(w: u32, h: u32, k: u32) -> Image {
    let [r, g, b] = PREVIEW_BG;
    Image::new(w * k, h * k, [r, g, b, 255])
}

/// 最近傍で `scale` 倍にする。
pub fn upscale(img: &Image, scale: u32) -> Image {
    let mut out = Image::new(img.w * scale, img.h * scale, [0; 4]);
    for y in 0..out.h {
        for x in 0..out.w {
            out.set(x, y, img.get(x / scale, y / scale));
        }
    }
    out
}

/// 文字列を並べ、パネルの地に白で描いて `scale` 倍にする(1 で描画の解像度のまま)。
pub fn preview(face: &Face, text: &str, scale: u32) -> Image {
    let lines: Vec<&str> = text.split('\n').collect();
    let width = lines
        .iter()
        .map(|l| line_width(face, l))
        .max()
        .unwrap_or(0)
        .max(1);
    let lh = u32::from(face.def.line_height);
    let mut img = background(width, lh * lines.len() as u32, face.scale);
    for (i, line) in lines.iter().enumerate() {
        let baseline = (i as u32 * lh + face.ascent) as i32;
        draw_line(&mut img, face, 0, baseline, line, TEXT_COLOR);
    }
    upscale(&img, scale)
}

/// 全文言を 640 × 360 の頁に流し込む(キーは灰色、文は白。はみ出した行は切れる)。`scale` 倍。
pub fn text_pages(face: &Face, entries: &[Entry], scale: u32) -> Vec<Image> {
    let lh = u32::from(face.def.line_height);
    let per_page = ((PAGE.1 - 2 * MARGIN) / lh).max(1) as usize;
    let mut pages: Vec<Vec<(&str, [u8; 3])>> = vec![Vec::new()];
    for e in entries {
        let mut lines = vec![(e.key.as_str(), LABEL_COLOR)];
        lines.extend(e.value.split('\n').map(|l| (l, TEXT_COLOR)));
        let used = pages.last().map_or(0, Vec::len);
        if used > 0 && used + lines.len() > per_page {
            pages.push(Vec::new());
        }
        for line in lines {
            if pages.last().is_some_and(|p| p.len() == per_page) {
                pages.push(Vec::new());
            }
            if let Some(p) = pages.last_mut() {
                p.push(line);
            }
        }
    }
    pages
        .iter()
        .map(|lines| {
            let mut img = background(PAGE.0, PAGE.1, face.scale);
            for (i, &(line, color)) in lines.iter().enumerate() {
                let baseline = (MARGIN + i as u32 * lh + face.ascent) as i32;
                draw_line(&mut img, face, MARGIN as i32, baseline, line, color);
            }
            upscale(&img, scale)
        })
        .collect()
}

/// 漢字を 1 字ずつ枠に入れて並べた一覧(フォントごとに見出しと格子)。`scale` 倍。
pub fn kanji_sheet(faces: &[Face], kanji: &[char], scale: u32) -> Image {
    let rows = (kanji.len() as u32).div_ceil(KANJI_COLUMNS).max(1);
    let cell = |f: &Face| {
        (
            u32::from(f.def.size_px) + 4,
            u32::from(f.def.line_height) + 4,
        )
    };
    let width = faces
        .iter()
        .map(|f| KANJI_COLUMNS * cell(f).0 + 1)
        .max()
        .unwrap_or(1)
        + 2 * MARGIN;
    let height: u32 = faces
        .iter()
        .map(|f| u32::from(f.def.line_height) + rows * cell(f).1 + 1 + MARGIN)
        .sum::<u32>()
        + MARGIN;
    let k = faces.first().map_or(1, |f| f.scale);
    let mut img = background(width, height, k);
    let [r, g, b] = GRID_COLOR;
    let mut top = MARGIN;
    for f in faces {
        let title = format!("{} contrast {}", f.def.id, f.def.contrast);
        draw_line(
            &mut img,
            f,
            MARGIN as i32,
            (top + f.ascent) as i32,
            &title,
            LABEL_COLOR,
        );
        top += u32::from(f.def.line_height);
        let (cw, ch) = cell(f);
        for y in 0..=rows * ch * k {
            for x in 0..=KANJI_COLUMNS * cw * k {
                if x % (cw * k) == 0 || y % (ch * k) == 0 {
                    img.set(MARGIN * k + x, top * k + y, [r, g, b, 255]);
                }
            }
        }
        for (i, &k) in kanji.iter().enumerate() {
            let (col, row) = (i as u32 % KANJI_COLUMNS, i as u32 / KANJI_COLUMNS);
            let x = MARGIN + col * cw + 2;
            let baseline = top + row * ch + 2 + f.ascent;
            draw_line(
                &mut img,
                f,
                x as i32,
                baseline as i32,
                &k.to_string(),
                TEXT_COLOR,
            );
        }
        top += rows * ch + 1 + MARGIN;
    }
    upscale(&img, scale)
}

/// `proof` の画像とファイル名: フォントごとの全文言の頁(`text_<id>_NN.png`)と、漢字の一覧(`kanji.png`)。
pub fn proof(g: &Generated, scale: u32) -> Vec<(String, Image)> {
    let mut out = Vec::new();
    for f in &g.faces {
        for (i, img) in text_pages(f, &g.checked.texts.entries, scale)
            .into_iter()
            .enumerate()
        {
            out.push((format!("text_{}_{:02}.png", f.def.id, i + 1), img));
        }
    }
    let kanji: Vec<char> = g
        .checked
        .chars
        .keys()
        .copied()
        .filter(|&c| is_kanji(c))
        .collect();
    out.push(("kanji.png".into(), kanji_sheet(&g.faces, &kanji, scale)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upscale_is_nearest_neighbour() {
        let mut img = Image::new(2, 3, [0; 4]);
        for y in 0..3 {
            for x in 0..2 {
                img.set(x, y, [x as u8, y as u8, 7, 255]);
            }
        }
        let big = upscale(&img, 3);
        assert_eq!((big.w, big.h), (6, 9));
        for y in 0..9 {
            for x in 0..6 {
                assert_eq!(big.get(x, y), [(x / 3) as u8, (y / 3) as u8, 7, 255]);
            }
        }
    }
}
