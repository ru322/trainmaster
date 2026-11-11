//! `fontdue` でのラスタライズと被覆率の補正(15 §4.4)。

use crate::charset::Source;
use crate::config::FaceDef;
use crate::error::FontgenError;
use fontdue::{Font, FontSettings};
use std::collections::BTreeMap;

/// 1 字のビットマップ(インクの外接矩形)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    /// 幅(px)。
    pub w: u32,
    /// 高さ(px)。
    pub h: u32,
    /// ペンの位置から左端まで。
    pub x_off: i32,
    /// ベースラインから上端まで(上が負)。
    pub y_off: i32,
    /// 補正後のアルファ(行優先、`w × h`)。
    pub alpha: Vec<u8>,
}

/// 1 字分のグリフ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    /// 字。
    pub ch: char,
    /// 送り幅(整数 px)。
    pub advance: u32,
    /// 字形の画素。空白など描くものがなければ `None`。
    pub bitmap: Option<Bitmap>,
}

/// 1 つの大きさのフォント。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    /// 設定。
    pub def: FaceDef,
    /// ベースラインから行の上端まで(px)。
    pub ascent: u32,
    /// ベースラインから下端まで(px)。
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

/// TTF を `size_px` の最適化の大きさで読む(FG-E01)。
pub fn load_font(ttf: &[u8], size_px: u8, path: &str) -> Result<Font, FontgenError> {
    let settings = FontSettings {
        scale: f32::from(size_px),
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

/// 1 つの大きさで全字をラスタライズする。`path` はフォントのパス(メッセージ用)。
pub fn rasterize_face(
    font: &Font,
    def: &FaceDef,
    chars: &BTreeMap<char, Source>,
    path: &str,
) -> Result<Face, FontgenError> {
    check_glyphs(font, chars)?;
    let px = f32::from(def.size_px);
    let lm = font
        .horizontal_line_metrics(px)
        .ok_or_else(|| range_err(path, def, "行の寸法がない".into()))?;
    let ascent = lm.ascent.ceil() as u32;
    let descent = (-lm.descent).ceil() as u32;
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
        let advance = m.advance_width.round() as u32;
        let (w, h) = (m.width as u32, m.height as u32);
        let x_off = m.xmin;
        let y_off = -(m.ymin + m.height as i32);
        let fits = advance <= 255
            && w <= 255
            && h <= 255
            && i8::try_from(x_off).is_ok()
            && i8::try_from(y_off).is_ok();
        if !fits {
            return Err(range_err(
                path,
                def,
                format!("{ch:?} の寸法が u8 / i8 に収まらない"),
            ));
        }
        let bitmap = (w > 0 && h > 0).then(|| Bitmap {
            w,
            h,
            x_off,
            y_off,
            alpha: cov.iter().map(|&c| table[c as usize]).collect(),
        });
        glyphs.push(Glyph {
            ch,
            advance,
            bitmap,
        });
    }
    Ok(Face {
        def: def.clone(),
        ascent,
        descent,
        glyphs,
    })
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
