//! パレット(`palette.ron`)の読み込みと検査。

use crate::error::SpriteError;
use serde::Deserialize;
use std::collections::BTreeSet;

/// パレットの色数の上限。
pub const MAX_COLORS: usize = 32;
/// 半透明を許す唯一の色の名前とアルファ値。
pub const SHADOW: (&str, u8) = ("shadow", 96);

/// パレットの 1 色。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteColor {
    /// ドット絵で使う 1 文字の記号。
    pub symbol: char,
    /// 色の名前(`PAL_` + 大文字の定数名になる)。
    pub name: String,
    /// RGBA。
    pub rgba: [u8; 4],
}

/// 名前付きの色の一覧。定義の順を保つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// 色の一覧。
    pub colors: Vec<PaletteColor>,
}

#[derive(Deserialize)]
#[serde(rename = "Palette", deny_unknown_fields)]
struct PaletteFile {
    colors: Vec<(char, String, u32)>,
}

impl Palette {
    /// RON の文字列から読み、SG-E01・E02・E03・E08・E09 を検査する。
    pub fn parse(text: &str) -> Result<Self, SpriteError> {
        let file: PaletteFile =
            crate::ron_options()
                .from_str(text)
                .map_err(|e| SpriteError::Syntax {
                    file: "palette.ron".into(),
                    message: e.to_string(),
                })?;
        if file.colors.len() > MAX_COLORS {
            return Err(SpriteError::TooManyColors {
                count: file.colors.len(),
            });
        }
        let mut symbols = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut colors = Vec::new();
        for (symbol, name, rgba) in file.colors {
            crate::check_name(&name)?;
            if !names.insert(name.clone()) {
                return Err(SpriteError::Name {
                    name,
                    reason: "パレットの名前が重複している".into(),
                });
            }
            if !symbols.insert(symbol) {
                return Err(SpriteError::UnknownSymbol {
                    sprite: "palette".into(),
                    symbol: symbol.to_string(),
                });
            }
            let rgba = rgba.to_be_bytes();
            let alpha = rgba[3];
            let allowed = alpha == 0 || alpha == 255 || (name == SHADOW.0 && alpha == SHADOW.1);
            if !allowed {
                return Err(SpriteError::Translucent { color: name, alpha });
            }
            colors.push(PaletteColor { symbol, name, rgba });
        }
        Ok(Self { colors })
    }

    /// 記号から色を引く。
    pub fn by_symbol(&self, symbol: char, sprite: &str) -> Result<[u8; 4], SpriteError> {
        self.colors
            .iter()
            .find(|c| c.symbol == symbol)
            .map(|c| c.rgba)
            .ok_or_else(|| SpriteError::UnknownSymbol {
                sprite: sprite.into(),
                symbol: symbol.to_string(),
            })
    }

    /// 名前から色を引く。
    pub fn by_name(&self, name: &str, sprite: &str) -> Result<[u8; 4], SpriteError> {
        self.colors
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.rgba)
            .ok_or_else(|| SpriteError::UnknownSymbol {
                sprite: sprite.into(),
                symbol: name.into(),
            })
    }
}
