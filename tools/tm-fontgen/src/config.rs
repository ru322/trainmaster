//! `assets/fonts/fontgen.toml` と `assets/atlas.toml` の読み込み(15 §4.2.2、§3.2)。

use crate::error::FontgenError;
use serde::Deserialize;
use std::collections::BTreeSet;

/// フォントの大きさ 1 つ分の設定。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceDef {
    /// `FontId` の名前(`Body12` など)。
    pub id: String,
    /// 大きさ(px)。
    pub size_px: u8,
    /// 行送り(px)。
    pub line_height: u8,
    /// 被覆率の補正(0 = そのまま。15 §4.4)。
    pub contrast: i32,
    /// 2 倍の大きさで描く文言のキー。その字に 2 倍の大きさの字形(大きい版)を作る。
    #[serde(default)]
    pub alt_keys: Vec<String>,
}

/// `[charset]`: 漢字以外の必ず入れる字。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharsetDef {
    /// `["U+0020", "U+007E"]` の形の閉区間。
    pub ranges: Vec<[String; 2]>,
    /// 個別の字。
    pub chars: String,
}

/// `[text]`。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextDef {
    /// `ja.toml` のパス。
    pub ja: String,
    /// `allowed_kanji.txt` のパス。
    pub allowed_kanji: String,
    /// 1 行の字数の警告の閾値。
    pub warn_line_chars: usize,
    /// フォントの帯の使用率(%)の警告の閾値。省略すると 80(15 §3.2)。
    #[serde(default = "default_warn_strip_percent")]
    pub warn_strip_percent: u32,
}

fn default_warn_strip_percent() -> u32 {
    80
}

/// `fontgen.toml` の中身。パスはリポジトリのルートからの相対パス。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// TTF のパス。
    pub font: String,
    /// TTF の SHA-256(16 進の小文字)。
    pub font_sha256: String,
    /// `atlas.toml` のパス。
    pub atlas: String,
    /// `atlas.toml` の帯の `owner`。
    pub strip_owner: String,
    /// グリフの間の透明な余白(px)。
    pub padding: u32,
    /// フォントの大きさ。並びが採番の順。
    pub face: Vec<FaceDef>,
    /// 漢字以外の字。
    pub charset: CharsetDef,
    /// 文言のファイル。
    pub text: TextDef,
}

fn bad(path: &str, message: impl Into<String>) -> FontgenError {
    FontgenError::Read {
        path: path.into(),
        message: message.into(),
    }
}

fn parse_code_point(path: &str, s: &str) -> Result<char, FontgenError> {
    s.strip_prefix("U+")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .and_then(char::from_u32)
        .ok_or_else(|| bad(path, format!("`{s}` は U+XXXX の形の字ではない")))
}

impl Config {
    /// `fontgen.toml` を読んで値を検査する(FG-E01)。`path` はメッセージに使う。
    pub fn parse(text: &str, path: &str) -> Result<Config, FontgenError> {
        let c: Config = toml::from_str(text).map_err(|e| bad(path, e.to_string()))?;
        if c.face.is_empty() {
            return Err(bad(path, "[[face]] がない"));
        }
        let mut ids = BTreeSet::new();
        for f in &c.face {
            let mut it = f.id.chars();
            let ok = it.next().is_some_and(|ch| ch.is_ascii_uppercase())
                && it.all(|ch| ch.is_ascii_alphanumeric());
            if !ok {
                return Err(bad(
                    path,
                    format!("face の id `{}` は ^[A-Z][A-Za-z0-9]*$", f.id),
                ));
            }
            if !ids.insert(f.id.clone()) {
                return Err(bad(path, format!("face の id `{}` が重複", f.id)));
            }
            if f.size_px == 0 || f.line_height == 0 {
                return Err(bad(
                    path,
                    format!("face `{}` の size_px と line_height は 1 以上", f.id),
                ));
            }
            if !(-100..=10_000).contains(&f.contrast) {
                return Err(bad(
                    path,
                    format!("face `{}` の contrast は -100〜10000", f.id),
                ));
            }
        }
        if !(1..=100).contains(&c.text.warn_strip_percent) {
            return Err(bad(path, "warn_strip_percent は 1〜100"));
        }
        if c.font_sha256.len() != 64 || !c.font_sha256.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(bad(path, "font_sha256 は 64 桁の 16 進"));
        }
        c.charset_chars(path)?;
        Ok(c)
    }

    /// `[charset]` の字(区間と個別の字の和集合)。
    pub fn charset_chars(&self, path: &str) -> Result<BTreeSet<char>, FontgenError> {
        let mut set = BTreeSet::new();
        for [lo, hi] in &self.charset.ranges {
            let (lo, hi) = (parse_code_point(path, lo)?, parse_code_point(path, hi)?);
            if lo > hi {
                return Err(bad(
                    path,
                    format!("区間 U+{:04X}〜U+{:04X} の向きが逆", lo as u32, hi as u32),
                ));
            }
            set.extend(lo..=hi);
        }
        set.extend(self.charset.chars.chars());
        Ok(set)
    }
}

/// フォントの帯の位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strip {
    /// ページの幅(px)。
    pub width: u32,
    /// ページの番号。
    pub page: u32,
    /// ページの中での帯の番号。
    pub index: u32,
    /// 帯の上端の y(ページの絶対座標)。
    pub y: u32,
    /// 帯の高さ(行)。
    pub height: u32,
    /// 論理 1px の描画の画素数(`atlas.toml` の `render_scale`。省略は 1)。
    pub render_scale: u32,
}

#[derive(Deserialize)]
struct AtlasFile {
    width: u32,
    #[serde(default)]
    render_scale: Option<u32>,
    page: Vec<PageDef>,
}

#[derive(Deserialize)]
struct PageDef {
    strips: Vec<StripDef>,
}

#[derive(Deserialize)]
struct StripDef {
    owner: String,
    height: u32,
}

/// `atlas.toml` から `owner` の帯を 1 つ探す(FG-E01)。
pub fn parse_strip(text: &str, owner: &str, path: &str) -> Result<Strip, FontgenError> {
    let file: AtlasFile = toml::from_str(text).map_err(|e| bad(path, e.to_string()))?;
    if file.width == 0 || file.width > 2048 {
        return Err(bad(path, "width は 1〜2048"));
    }
    let render_scale = file.render_scale.unwrap_or(1);
    if !(1..=4).contains(&render_scale) {
        return Err(bad(path, "render_scale は 1〜4"));
    }
    let mut found = None;
    for (page, p) in file.page.iter().enumerate() {
        let mut y = 0;
        for (index, s) in p.strips.iter().enumerate() {
            if s.owner == owner {
                if found.is_some() {
                    return Err(bad(path, format!("owner = \"{owner}\" の帯が複数ある")));
                }
                found = Some(Strip {
                    width: file.width,
                    page: page as u32,
                    index: index as u32,
                    y,
                    height: s.height,
                    render_scale,
                });
            }
            y += s.height;
        }
        if y > 2048 {
            return Err(bad(
                path,
                format!("page {page} の帯の高さの合計 {y} が 2048 を超える"),
            ));
        }
    }
    found.ok_or_else(|| bad(path, format!("owner = \"{owner}\" の帯がない")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATLAS: &str = "width = 1024\n[[page]]\nstrips = [ { owner = \"font\", height = 768 }, { owner = \"sprites\", height = 256 } ]\n";

    #[test]
    fn strip_of_font() {
        let s = parse_strip(ATLAS, "font", "atlas.toml").unwrap();
        assert_eq!(
            (s.page, s.index, s.y, s.height, s.width),
            (0, 0, 0, 768, 1024)
        );
        let s = parse_strip(ATLAS, "sprites", "atlas.toml").unwrap();
        assert_eq!((s.index, s.y), (1, 768));
    }

    #[test]
    fn strip_errors_are_e01() {
        for (text, owner) in [
            (ATLAS, "nothing"),
            ("width = 4096\npage = []", "font"),
            ("width = ", "font"),
        ] {
            assert_eq!(parse_strip(text, owner, "a").unwrap_err().code(), "FG-E01");
        }
    }

    const CONFIG: &str = "font = \"f.ttf\"\nfont_sha256 = \"7d2b48d84ef4e65c9f85bcf65fb1fb41c92b134be641fab6a7141d597c9a92b0\"\natlas = \"a.toml\"\nstrip_owner = \"font\"\npadding = 1\n[[face]]\nid = \"Body12\"\nsize_px = 12\nline_height = 16\ncontrast = 30\n[[face]]\nid = \"Head16\"\nsize_px = 16\nline_height = 20\ncontrast = 0\n[charset]\nranges = []\nchars = \"あ\"\n[text]\nja = \"ja.toml\"\nallowed_kanji = \"k.txt\"\nwarn_line_chars = 20\n";

    #[test]
    fn contrast_per_face_and_strip_percent() {
        let c = Config::parse(CONFIG, "f").unwrap();
        let contrasts: Vec<(&str, i32)> =
            c.face.iter().map(|f| (f.id.as_str(), f.contrast)).collect();
        assert_eq!(contrasts, [("Body12", 30), ("Head16", 0)]);
        assert_eq!(c.text.warn_strip_percent, 80);
        let c = Config::parse(&format!("{CONFIG}warn_strip_percent = 50\n"), "f").unwrap();
        assert_eq!(c.text.warn_strip_percent, 50);
        for bad in ["warn_strip_percent = 0\n", "warn_strip_percent = 101\n"] {
            let e = Config::parse(&format!("{CONFIG}{bad}"), "f").unwrap_err();
            assert_eq!(e.code(), "FG-E01");
        }
    }

    #[test]
    fn code_points() {
        assert_eq!(parse_code_point("p", "U+79ED").unwrap(), '秭');
        assert!(parse_code_point("p", "79ED").is_err());
        assert!(parse_code_point("p", "U+D800").is_err());
    }
}
