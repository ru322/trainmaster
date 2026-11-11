//! エラー型。コードは 15 §5.7 の SG-E01〜E09。

use std::fmt;

/// 生成の失敗。`SG-Exx` は 15 §5.7 の番号で、`Io` と `Atlas` は表にない入出力と `atlas.toml` の誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpriteError {
    /// ファイルの読み書きに失敗した。
    Io {
        /// 対象のパス。
        path: String,
        /// OS のメッセージ。
        message: String,
    },
    /// `atlas.toml` の構文または内容の誤り。
    Atlas {
        /// 誤りの内容。
        message: String,
    },
    /// SG-E01: RON の構文と定義の構造(未知の項目、`source` と `frames` の矛盾など)の誤り。
    Syntax {
        /// 定義ファイル名(または対象の名前)。
        file: String,
        /// 誤りの内容。
        message: String,
    },
    /// SG-E02: 名前の規則違反、重複、表を成す名前の欠け。
    Name {
        /// 対象の名前。
        name: String,
        /// 誤りの内容。
        reason: String,
    },
    /// SG-E03: パレットにない記号や色の名前、パレット内の記号の重複。
    UnknownSymbol {
        /// 対象のスプライト(パレットの誤りなら `palette`)。
        sprite: String,
        /// 該当の記号または名前。
        symbol: String,
    },
    /// SG-E04: 行の長さがそろわない、フレームの寸法が違う、フレームが連続しない。
    Shape {
        /// 対象のスプライト。
        sprite: String,
        /// 誤りの内容。
        reason: String,
    },
    /// SG-E05: `Derive` の元がない、循環、元が不正。
    Derive {
        /// 対象のスプライト。
        sprite: String,
        /// 誤りの内容。
        reason: String,
    },
    /// SG-E06: 基準点が絵の外にある。
    Pivot {
        /// 対象のスプライト。
        sprite: String,
        /// 指定の基準点。
        pivot: (i32, i32),
        /// 絵の寸法。
        size: (u32, u32),
    },
    /// SG-E06: 9 分割の寸法が規則に反する(`corner` が `border_w + 2` 未満)。
    Slice {
        /// 対象のスプライト。
        sprite: String,
        /// 誤りの内容。
        reason: String,
    },
    /// SG-E07: 帯にスプライトが入りきらない。
    Overflow {
        /// 必要な高さ(行)。
        needed: u32,
        /// 帯の高さ(行)。
        available: u32,
    },
    /// SG-E08: パレットが 32 色を超える。
    TooManyColors {
        /// 色数。
        count: usize,
    },
    /// SG-E09: 半透明の色の使い方が規則に反する(許すのは `shadow` の α = 96 だけ)。
    Translucent {
        /// 色の名前。
        color: String,
        /// アルファ値。
        alpha: u8,
    },
}

impl SpriteError {
    /// 設計書のエラー番号。表にない誤りは `None`。
    pub fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::Io { .. } | Self::Atlas { .. } => return None,
            Self::Syntax { .. } => "SG-E01",
            Self::Name { .. } => "SG-E02",
            Self::UnknownSymbol { .. } => "SG-E03",
            Self::Shape { .. } => "SG-E04",
            Self::Derive { .. } => "SG-E05",
            Self::Pivot { .. } | Self::Slice { .. } => "SG-E06",
            Self::Overflow { .. } => "SG-E07",
            Self::TooManyColors { .. } => "SG-E08",
            Self::Translucent { .. } => "SG-E09",
        })
    }
}

impl fmt::Display for SpriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(code) = self.code() {
            write!(f, "{code}: ")?;
        }
        match self {
            Self::Io { path, message } => write!(f, "{path}: {message}"),
            Self::Atlas { message } => write!(f, "atlas.toml: {message}"),
            Self::Syntax { file, message } => write!(f, "{file}: {message}"),
            Self::Name { name, reason } => write!(f, "名前 `{name}`: {reason}"),
            Self::UnknownSymbol { sprite, symbol } => {
                write!(
                    f,
                    "{sprite}: パレットに `{symbol}` がない(または重複している)"
                )
            }
            Self::Shape { sprite, reason } => write!(f, "{sprite}: {reason}"),
            Self::Derive { sprite, reason } => write!(f, "{sprite}: {reason}"),
            Self::Pivot {
                sprite,
                pivot,
                size,
            } => write!(
                f,
                "{sprite}: 基準点 {pivot:?} が寸法 {}×{} の外にある",
                size.0, size.1
            ),
            Self::Slice { sprite, reason } => write!(f, "{sprite}: {reason}"),
            Self::Overflow { needed, available } => {
                write!(f, "帯にあふれた(必要 {needed} 行、帯は {available} 行)")
            }
            Self::TooManyColors { count } => write!(f, "パレットが {count} 色で、32 色を超える"),
            Self::Translucent { color, alpha } => write!(
                f,
                "色 `{color}` のアルファ {alpha} は許されない(半透明は shadow の 96 だけ)"
            ),
        }
    }
}

impl std::error::Error for SpriteError {}
