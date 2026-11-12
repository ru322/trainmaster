//! `assets/atlas.toml`(アトラスのページと帯の割り当て)の読み込み。

use crate::error::SpriteError;
use serde::Deserialize;

/// スプライトの帯の位置。
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

/// ページの幅と高さの上限(VideoCore IV のテクスチャの上限)。
pub const MAX_PAGE: u32 = 2048;

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

fn err(message: impl Into<String>) -> SpriteError {
    SpriteError::Atlas {
        message: message.into(),
    }
}

/// `atlas.toml` から `owner = "sprites"` の帯を探す。ページは `MAX_PAGE` 以下に収まることを確かめる。
pub fn parse(text: &str) -> Result<Strip, SpriteError> {
    let file: AtlasFile = toml::from_str(text).map_err(|e| err(e.to_string()))?;
    if file.width == 0 || file.width > MAX_PAGE {
        return Err(err(format!("width は 1〜{MAX_PAGE}")));
    }
    let render_scale = file.render_scale.unwrap_or(1);
    if !(1..=4).contains(&render_scale) {
        return Err(err("render_scale は 1〜4"));
    }
    let mut found = None;
    for (page, p) in file.page.iter().enumerate() {
        let mut y = 0;
        for (index, s) in p.strips.iter().enumerate() {
            if s.owner == "sprites" {
                if found.is_some() {
                    return Err(err("owner = \"sprites\" の帯が複数ある"));
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
        if y > MAX_PAGE {
            return Err(err(format!(
                "page {page} の帯の高さの合計 {y} が {MAX_PAGE} を超える"
            )));
        }
    }
    found.ok_or_else(|| err("owner = \"sprites\" の帯がない"))
}
