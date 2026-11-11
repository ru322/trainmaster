//! エラー型。コードは 15 §4.3 の FG-E01〜E15。

use std::fmt;

/// 文言の中の位置(ファイル、キー、1 始まりの何文字目か)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPos {
    /// `ja.toml` のパス。
    pub file: String,
    /// キー(`a.b.c`)。
    pub key: String,
    /// 何文字目か(1 始まり)。
    pub index: usize,
}

impl fmt::Display for TextPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} の {} 文字目", self.file, self.key, self.index)
    }
}

/// 生成の失敗。どれも終了コード 1。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontgenError {
    /// FG-E01: ファイルが読めない、構文や設定の誤り。
    Read {
        /// 対象のパス。
        path: String,
        /// 誤りの内容。
        message: String,
    },
    /// FG-E02: フォントの SHA-256 が `font_sha256` と違う。
    FontHash {
        /// `fontgen.toml` の値。
        expected: String,
        /// 実際の値。
        actual: String,
    },
    /// FG-E03: `allowed_kanji.txt` の書式の誤り。
    KanjiList {
        /// 行番号(1 始まり。0 はファイル全体の誤り)。
        line: usize,
        /// 該当の字(行全体の誤りなら `None`)。
        ch: Option<char>,
        /// 誤りの内容。
        reason: String,
    },
    /// FG-E04: `ja.toml` のキーの誤り。
    Key {
        /// キー。
        key: String,
        /// 誤りの内容。
        reason: String,
    },
    /// FG-E05: 字の集合にない漢字以外の字。
    Char {
        /// 位置。
        pos: TextPos,
        /// 該当の字。
        ch: char,
    },
    /// FG-E06: `[joyo_2010]` にも `[number_units]` にもない漢字。
    Kanji {
        /// 位置。
        pos: TextPos,
        /// 該当の字。
        ch: char,
    },
    /// FG-E07: 常用漢字でない単位の字を単位の語の外で使った。
    UnitChar {
        /// 位置。
        pos: TextPos,
        /// 該当の字。
        ch: char,
    },
    /// FG-E08: `tm-numeric` の漢字が `allowed_kanji.txt` にない。
    NumericKanji {
        /// 該当の字。
        ch: char,
    },
    /// FG-E09: `tm-numeric` の漢字以外の字が `[charset]` にない。
    NumericChar {
        /// 該当の字。
        ch: char,
    },
    /// FG-E10: フォントに字形がない。
    MissingGlyph {
        /// 該当の字。
        ch: char,
        /// 字の出どころ。
        source: &'static str,
    },
    /// FG-E11: グリフが帯に収まらない。
    Overflow {
        /// 必要な行数(収まらなかった時点)。
        needed: u32,
        /// 帯の高さ。
        available: u32,
    },
    /// FG-E12: プレースホルダの誤り。
    Placeholder {
        /// 位置。
        pos: TextPos,
        /// 誤りの内容。
        reason: String,
    },
    /// FG-E13: `{money}` の直後に「円」がある。
    YenAfterMoney {
        /// 位置(「円」の位置)。
        pos: TextPos,
    },
    /// FG-E14: 空白の誤り。
    Space {
        /// 位置。
        pos: TextPos,
        /// 誤りの内容。
        reason: &'static str,
    },
    /// FG-E15: 文言の数が `TextId(u16)` の範囲を超える。
    TooManyTexts {
        /// 文言の数。
        count: usize,
    },
}

impl FontgenError {
    /// `FG-Exx` のコード。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Read { .. } => "FG-E01",
            Self::FontHash { .. } => "FG-E02",
            Self::KanjiList { .. } => "FG-E03",
            Self::Key { .. } => "FG-E04",
            Self::Char { .. } => "FG-E05",
            Self::Kanji { .. } => "FG-E06",
            Self::UnitChar { .. } => "FG-E07",
            Self::NumericKanji { .. } => "FG-E08",
            Self::NumericChar { .. } => "FG-E09",
            Self::MissingGlyph { .. } => "FG-E10",
            Self::Overflow { .. } => "FG-E11",
            Self::Placeholder { .. } => "FG-E12",
            Self::YenAfterMoney { .. } => "FG-E13",
            Self::Space { .. } => "FG-E14",
            Self::TooManyTexts { .. } => "FG-E15",
        }
    }
}

/// 字と `U+XXXX` を並べた表記。
pub fn show_char(ch: char) -> String {
    format!("「{}」(U+{:04X})", ch.escape_debug(), ch as u32)
}

impl fmt::Display for FontgenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ", self.code())?;
        match self {
            Self::Read { path, message } => write!(f, "{path}: {message}"),
            Self::FontHash { expected, actual } => write!(
                f,
                "フォントの SHA-256 が違う(fontgen.toml: {expected}、実際: {actual})"
            ),
            Self::KanjiList { line, ch, reason } => {
                match line {
                    0 => write!(f, "allowed_kanji.txt(ファイル全体)")?,
                    n => write!(f, "allowed_kanji.txt: {n} 行目")?,
                }
                if let Some(c) = ch {
                    write!(f, " {}", show_char(*c))?;
                }
                write!(f, ": {reason}")
            }
            Self::Key { key, reason } => write!(f, "ja.toml: キー `{key}`: {reason}"),
            Self::Char { pos, ch } => write!(
                f,
                "{pos} {}: 使えない字。! ? ( ) と空白は半角で書く。新しい記号は fontgen.toml の [charset] に足す",
                show_char(*ch)
            ),
            Self::Kanji { pos, ch } => write!(
                f,
                "{pos} {}: 常用漢字ではありません。かなで書くか、常用漢字の語に言い換えてください",
                show_char(*ch)
            ),
            Self::UnitChar { pos, ch } => write!(
                f,
                "{pos} {}: 常用漢字でない数の単位の字は、単位の語(垓、秭、穣、澗、阿僧祇)の中でだけ使える",
                show_char(*ch)
            ),
            Self::NumericKanji { ch } => write!(
                f,
                "tm-numeric の {} が allowed_kanji.txt のどの区分にもない",
                show_char(*ch)
            ),
            Self::NumericChar { ch } => write!(
                f,
                "tm-numeric の {} が fontgen.toml の [charset] にない",
                show_char(*ch)
            ),
            Self::MissingGlyph { ch, source } => write!(
                f,
                "{} の字形がフォントにない(出どころ: {source})",
                show_char(*ch)
            ),
            Self::Overflow { needed, available } => write!(
                f,
                "グリフが帯に収まらない({needed} 行が要る、帯は {available} 行)"
            ),
            Self::Placeholder { pos, reason } => write!(f, "{pos}: {reason}"),
            Self::YenAfterMoney { pos } => write!(
                f,
                "{pos}: {{money}} の後ろに「円」を書かない(数の表示が「円」を付ける)"
            ),
            Self::Space { pos, reason } => write!(f, "{pos}: {reason}"),
            Self::TooManyTexts { count } => {
                write!(f, "文言が {count} 個あり、TextId(u16) に収まらない")
            }
        }
    }
}

impl std::error::Error for FontgenError {}
