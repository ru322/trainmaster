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
}

#[derive(Deserialize)]
struct AtlasFile {
    width: u32,
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

/// `atlas.toml` から `owner = "sprites"` の帯を探す。ページは 1024 以下に収まることを確かめる。
pub fn parse(text: &str) -> Result<Strip, SpriteError> {
    let file: AtlasFile = toml::from_str(text).map_err(|e| err(e.to_string()))?;
    if file.width == 0 || file.width > 1024 {
        return Err(err("width は 1〜1024"));
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
                });
            }
            y += s.height;
        }
        if y > 1024 {
            return Err(err(format!(
                "page {page} の帯の高さの合計 {y} が 1024 を超える"
            )));
        }
    }
    found.ok_or_else(|| err("owner = \"sprites\" の帯がない"))
}
