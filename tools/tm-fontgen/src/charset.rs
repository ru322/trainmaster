//! フォントに入れる字の集合(15 §4.3)と、`tm-numeric` の字の検査(FG-E08、FG-E09)。

use crate::error::FontgenError;
use crate::kanji::{AllowedKanji, is_kanji};
use std::collections::{BTreeMap, BTreeSet};

/// 字の出どころ(FG-E10 と `report` で使う)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// `fontgen.toml` の `[charset]`。
    Charset,
    /// `ja.toml` に出る漢字。
    Text,
    /// `tm-numeric` の `OUTPUT_CHARS` と `JA_UNITS`。
    Numeric,
    /// `allowed_kanji.txt` の `[joyo_2010]`(字形の有無だけを調べる)。
    Joyo,
    /// `allowed_kanji.txt` の `[number_units]`(字形の有無だけを調べる)。
    NumberUnits,
}

impl Source {
    /// メッセージ用の名前。
    pub fn label(self) -> &'static str {
        match self {
            Source::Charset => "[charset]",
            Source::Text => "ja.toml",
            Source::Numeric => "OUTPUT_CHARS / JA_UNITS",
            Source::Joyo => "[joyo_2010]",
            Source::NumberUnits => "[number_units]",
        }
    }
}

/// `tm_numeric::OUTPUT_CHARS` と `JA_UNITS`(漢字と読み)のすべての字。
pub fn numeric_chars() -> BTreeSet<char> {
    let mut s: BTreeSet<char> = tm_numeric::OUTPUT_CHARS.chars().collect();
    for (unit, yomi) in tm_numeric::JA_UNITS {
        s.extend(unit.chars());
        s.extend(yomi.chars());
    }
    s
}

/// `tm_numeric::JA_UNITS` の単位の語(空の語を除く)。
pub fn unit_words() -> Vec<&'static str> {
    tm_numeric::JA_UNITS
        .iter()
        .map(|u| u.0)
        .filter(|w| !w.is_empty())
        .collect()
}

/// `numeric` の漢字が許可の一覧に(FG-E08)、それ以外の字が `[charset]` に(FG-E09)あることを確かめる。
pub fn check_numeric(
    charset: &BTreeSet<char>,
    allowed: &AllowedKanji,
    numeric: &BTreeSet<char>,
) -> Result<(), FontgenError> {
    for &c in numeric {
        if is_kanji(c) {
            if !allowed.contains(c) {
                return Err(FontgenError::NumericKanji { ch: c });
            }
        } else if !charset.contains(&c) {
            return Err(FontgenError::NumericChar { ch: c });
        }
    }
    Ok(())
}

/// フォントに入れる字 = `[charset]` ∪ `ja.toml` の漢字 ∪ `numeric`。出どころは先に挙げた方。
pub fn font_chars(
    charset: &BTreeSet<char>,
    text_kanji: &BTreeSet<char>,
    numeric: &BTreeSet<char>,
) -> BTreeMap<char, Source> {
    let mut out = BTreeMap::new();
    let groups = [
        (charset, Source::Charset),
        (text_kanji, Source::Text),
        (numeric, Source::Numeric),
    ];
    for (set, src) in groups {
        for &c in set {
            out.entry(c).or_insert(src);
        }
    }
    out
}

/// 許可の一覧の全字と出どころ(FG-E10 で字形の有無だけを調べる)。
pub fn allowed_chars(allowed: &AllowedKanji) -> BTreeMap<char, Source> {
    let mut out = BTreeMap::new();
    for (set, src) in [
        (&allowed.joyo, Source::Joyo),
        (&allowed.number_units, Source::NumberUnits),
    ] {
        for &c in set {
            out.entry(c).or_insert(src);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> AllowedKanji {
        AllowedKanji {
            joyo: "円万駅".chars().collect(),
            number_units: "億万".chars().collect(),
            kyoiku: None,
        }
    }

    #[test]
    fn union_with_sources() {
        let charset: BTreeSet<char> = "0123456789+まんおく".chars().collect();
        let text: BTreeSet<char> = "駅円".chars().collect();
        let numeric: BTreeSet<char> = "0円万億まんおく".chars().collect();
        check_numeric(&charset, &allowed(), &numeric).unwrap();
        let m = font_chars(&charset, &text, &numeric);
        assert_eq!(m.len(), 19);
        assert_eq!(m[&'0'], Source::Charset);
        assert_eq!(m[&'円'], Source::Text);
        assert_eq!(m[&'億'], Source::Numeric);
        assert!(!m.contains_key(&'丁'));
    }

    #[test]
    fn allowed_chars_cover_both_sections() {
        let m = allowed_chars(&allowed());
        assert_eq!(m.len(), 4);
        assert_eq!(m[&'万'], Source::Joyo);
        assert_eq!(m[&'億'], Source::NumberUnits);
    }

    #[test]
    fn numeric_kanji_missing_is_e08() {
        let numeric: BTreeSet<char> = "兆".chars().collect();
        let e = check_numeric(&BTreeSet::new(), &allowed(), &numeric).unwrap_err();
        assert_eq!(e.code(), "FG-E08");
    }

    #[test]
    fn numeric_non_kanji_missing_is_e09() {
        let numeric: BTreeSet<char> = "+".chars().collect();
        let e = check_numeric(&BTreeSet::new(), &allowed(), &numeric).unwrap_err();
        assert_eq!(e.code(), "FG-E09");
    }

    #[test]
    fn real_numeric_chars() {
        let n = numeric_chars();
        assert!(n.contains(&'秭') && n.contains(&'円') && n.contains(&'+') && n.contains(&'じ'));
        assert_eq!(unit_words().len(), 17);
        assert!(unit_words().contains(&"無量大数"));
    }
}
