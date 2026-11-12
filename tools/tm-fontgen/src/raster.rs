//! `fontdue` でのラスタライズと被覆率の補正(15 §4.4)。

use crate::charset::Source;
use crate::config::FaceDef;
use crate::error::FontgenError;
use fontdue::{Font, FontSettings};
use std::collections::{BTreeMap, BTreeSet};

/// 1 字のビットマップ(描画の画素。インクの外接矩形を、論理 1px = `Face::scale` 画素の格子にそろえて広げたもの)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    /// 幅(描画の画素。`scale` の倍数)。
    pub w: u32,
    /// 高さ(描画の画素。`scale` の倍数)。
    pub h: u32,
    /// ペンの位置から左端まで(描画の画素。`scale` の倍数)。
    pub x_off: i32,
    /// ベースラインから上端まで(上が負。描画の画素。`scale` の倍数)。
    pub y_off: i32,
    /// 補正後のアルファ(行優先、`w × h`)。
    pub alpha: Vec<u8>,
}

/// 1 字分のグリフ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    /// 字。
    pub ch: char,
    /// 送り幅(論理 px)。
    pub advance: u32,
    /// 字形の画素。空白など描くものがなければ `None`。
    pub bitmap: Option<Bitmap>,
    /// 2 倍に描くときの字形(`bitmap` の枠をちょうど 2 倍にした画素)。`alt_keys` の字だけ。
    pub alt: Option<Bitmap>,
}

/// 1 つの大きさのフォント。寸法は論理 px、ビットマップは描画の画素(論理 × `scale`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    /// 設定。
    pub def: FaceDef,
    /// 論理 1px の描画の画素数(`atlas.toml` の `render_scale`)。
    pub scale: u32,
    /// ベースラインから行の上端まで(論理 px)。
    pub ascent: u32,
    /// ベースラインから下端まで(論理 px)。
    pub descent: u32,
    /// 文字コードの昇順。
    pub glyphs: Vec<Glyph>,
}

impl Face {
    /// その字のグリフ。
    pub fn glyph(&self, ch: char) -> Option<&Glyph> {
        self.glyphs
            .binary_search_by_key(&ch, |g| g.ch)
            .ok()
            .map(|i| &self.glyphs[i])
    }
}

/// 被覆率 → アルファの表。`α = clamp((cov − 128) × (100 + contrast) / 100 + 128, 0, 255)`(整数)。
pub fn contrast_table(contrast: i32) -> [u8; 256] {
    let mut t = [0u8; 256];
    for (cov, a) in t.iter_mut().enumerate() {
        let v = (cov as i32 - 128) * (100 + contrast) / 100 + 128;
        *a = v.clamp(0, 255) as u8;
    }
    t
}

/// TTF を `size_px`(描画の画素)の最適化の大きさで読む(FG-E01)。
pub fn load_font(ttf: &[u8], size_px: u32, path: &str) -> Result<Font, FontgenError> {
    let settings = FontSettings {
        scale: size_px as f32,
        ..FontSettings::default()
    };
    Font::from_bytes(ttf, settings).map_err(|e| FontgenError::Read {
        path: path.into(),
        message: e.into(),
    })
}

/// すべての字に字形があるかを確かめる(FG-E10)。
pub fn check_glyphs(font: &Font, chars: &BTreeMap<char, Source>) -> Result<(), FontgenError> {
    for (&ch, &src) in chars {
        if font.lookup_glyph_index(ch) == 0 {
            return Err(FontgenError::MissingGlyph {
                ch,
                source: src.label(),
            });
        }
    }
    Ok(())
}

fn range_err(path: &str, def: &FaceDef, what: String) -> FontgenError {
    FontgenError::Read {
        path: path.into(),
        message: format!("face `{}`: {what}", def.id),
    }
}

/// 1 つの大きさで全字を、論理の `size_px` の `scale` 倍の画素でラスタライズする。`path` はフォントのパス(メッセージ用)。
/// 送り幅は論理 px の整数でなければならない。字形の矩形は、論理の格子(`scale` 画素)にそろうまで透明の余白で広げる。
pub fn rasterize_face(
    font: &Font,
    def: &FaceDef,
    chars: &BTreeMap<char, Source>,
    path: &str,
    scale: u32,
) -> Result<Face, FontgenError> {
    check_glyphs(font, chars)?;
    let k = scale.max(1) as i32;
    let px = f32::from(def.size_px) * k as f32;
    let lm = font
        .horizontal_line_metrics(px)
        .ok_or_else(|| range_err(path, def, "行の寸法がない".into()))?;
    let ascent = (lm.ascent / k as f32).ceil() as u32;
    let descent = (-lm.descent / k as f32).ceil() as u32;
    if ascent + descent > u32::from(def.line_height) {
        return Err(range_err(
            path,
            def,
            format!(
                "ascent {ascent} + descent {descent} が line_height {} を超える",
                def.line_height
            ),
        ));
    }
    let table = contrast_table(def.contrast);
    let mut glyphs = Vec::with_capacity(chars.len());
    for &ch in chars.keys() {
        let (m, cov) = font.rasterize(ch, px);
        let advance_px = m.advance_width.round() as i32;
        if advance_px % k != 0 {
            return Err(range_err(
                path,
                def,
                format!("{ch:?} の送り幅 {advance_px} が {k} の倍数でない"),
            ));
        }
        let advance = (advance_px / k) as u32;
        let (iw, ih) = (m.width as i32, m.height as i32);
        let (ix, iy) = (m.xmin, -(m.ymin + ih));
        let (x0, y0) = (ix.div_euclid(k) * k, iy.div_euclid(k) * k);
        let (x1, y1) = (
            (ix + iw + k - 1).div_euclid(k) * k,
            (iy + ih + k - 1).div_euclid(k) * k,
        );
        let fits = advance <= 255
            && (x1 - x0) / k <= 255
            && (y1 - y0) / k <= 255
            && i8::try_from(x0 / k).is_ok()
            && i8::try_from(y0 / k).is_ok();
        if !fits {
            return Err(range_err(
                path,
                def,
                format!("{ch:?} の寸法が u8 / i8 に収まらない"),
            ));
        }
        let bitmap = (iw > 0 && ih > 0).then(|| {
            let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
            let mut alpha = vec![0u8; (w * h) as usize];
            for y in 0..ih {
                for x in 0..iw {
                    let dst = ((y + iy - y0) as u32 * w + (x + ix - x0) as u32) as usize;
                    alpha[dst] = table[cov[(y * iw + x) as usize] as usize];
                }
            }
            Bitmap {
                w,
                h,
                x_off: x0,
                y_off: y0,
                alpha,
            }
        });
        glyphs.push(Glyph {
            ch,
            advance,
            bitmap,
            alt: None,
        });
    }
    Ok(Face {
        def: def.clone(),
        scale: k as u32,
        ascent,
        descent,
        glyphs,
    })
}

/// `chars` の字に、2 倍の大きさでラスタライズした大きい版を付ける。`font2` は 2 倍の大きさで読んだフォント。
/// 大きい版は元の字形の枠をちょうど 2 倍にした画素に置く(はみ出すインクは FG-E01)。
pub fn add_alt(
    face: &mut Face,
    font2: &Font,
    chars: &BTreeSet<char>,
    path: &str,
) -> Result<(), FontgenError> {
    let k = face.scale as i32;
    let px = f32::from(face.def.size_px) * (2 * k) as f32;
    let table = contrast_table(face.def.contrast);
    for &ch in chars {
        let def = face.def.clone();
        let Some(g) = face.glyphs.iter_mut().find(|g| g.ch == ch) else {
            return Err(range_err(path, &def, format!("{ch:?} がフォントにない")));
        };
        let Some(b) = &g.bitmap else { continue };
        let (m, cov) = font2.rasterize(ch, px);
        let (iw, ih) = (m.width as i32, m.height as i32);
        let (ix, iy) = (m.xmin, -(m.ymin + ih));
        let (w, h) = (2 * b.w, 2 * b.h);
        let (x0, y0) = (2 * b.x_off, 2 * b.y_off);
        let mut alpha = vec![0u8; (w * h) as usize];
        for y in 0..ih {
            for x in 0..iw {
                let a = table[cov[(y * iw + x) as usize] as usize];
                let (tx, ty) = (x + ix - x0, y + iy - y0);
                if tx < 0 || ty < 0 || tx >= w as i32 || ty >= h as i32 {
                    if a > 32 {
                        return Err(range_err(
                            path,
                            &def,
                            format!("{ch:?} の 2 倍の字形が枠をはみ出す"),
                        ));
                    }
                    continue;
                }
                alpha[(ty as u32 * w + tx as u32) as usize] = a;
            }
        }
        g.alt = Some(Bitmap {
            w,
            h,
            x_off: x0,
            y_off: y0,
            alpha,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_zero_is_identity() {
        let t = contrast_table(0);
        assert!(t.iter().enumerate().all(|(i, &a)| a as usize == i));
    }

    #[test]
    fn contrast_steepens_and_clamps() {
        let t = contrast_table(100);
        assert_eq!((t[0], t[64], t[128], t[192], t[255]), (0, 0, 128, 255, 255));
        assert_eq!(t[100], 72);
        let t = contrast_table(30);
        assert_eq!(
            (
                t[0], t[38], t[39], t[64], t[128], t[200], t[226], t[227], t[255]
            ),
            (0, 11, 13, 45, 128, 221, 255, 255, 255)
        );
        let t = contrast_table(10_000);
        assert_eq!((t[127], t[128]), (27, 128));
    }
}
