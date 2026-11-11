//! `report` の表示(字種の内訳、帯の使用率、使っている漢字、教育漢字でない字。15 §4.1)。

use crate::Generated;
use crate::charset::Source;
use crate::kanji::is_kanji;
use std::collections::BTreeSet;
use std::fmt::Write;

/// 字の種類(`report` の内訳)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// U+0020〜U+007E。
    Ascii,
    /// ひらがな。
    Hiragana,
    /// カタカナと長音「ー」。
    Katakana,
    /// 漢字(15 §4.3 の定義)。
    Kanji,
    /// それ以外(和文の記号など)。
    Symbol,
}

impl Kind {
    /// 字の種類。
    pub fn of(c: char) -> Kind {
        match c {
            ' '..='~' => Kind::Ascii,
            '\u{3041}'..='\u{3096}' => Kind::Hiragana,
            '\u{30A1}'..='\u{30FA}' | 'ー' => Kind::Katakana,
            c if is_kanji(c) => Kind::Kanji,
            _ => Kind::Symbol,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Ascii => "ASCII",
            Kind::Hiragana => "ひらがな",
            Kind::Katakana => "カタカナ",
            Kind::Kanji => "漢字",
            Kind::Symbol => "記号",
        }
    }
}

/// 文言に出る漢字のうち、`[report_kyoiku]` にない字。区分がなければ `None`。
pub fn non_kyoiku(g: &Generated) -> Option<BTreeSet<char>> {
    let c = &g.checked;
    let kyoiku = c.allowed.kyoiku.as_ref()?;
    Some(c.texts.kanji.difference(kyoiku).copied().collect())
}

fn list(s: &mut String, set: &BTreeSet<char>) {
    if !set.is_empty() {
        let _ = writeln!(s, "  {}", set.iter().collect::<String>());
    }
}

/// `report` が表示する文。
pub fn report(g: &Generated) -> String {
    let c = &g.checked;
    let mut s = String::new();
    let _ = writeln!(s, "字: {}", c.chars.len());
    let kinds = [
        Kind::Ascii,
        Kind::Hiragana,
        Kind::Katakana,
        Kind::Symbol,
        Kind::Kanji,
    ];
    for k in kinds {
        let n = c.chars.keys().filter(|&&ch| Kind::of(ch) == k).count();
        let _ = writeln!(s, "  {}: {n}", k.label());
    }
    for src in [Source::Charset, Source::Text, Source::Numeric] {
        let n = c.chars.values().filter(|&&v| v == src).count();
        let _ = writeln!(s, "  出どころ {}: {n}", src.label());
    }
    let a = &c.allowed;
    let kyoiku = a
        .kyoiku
        .as_ref()
        .map_or("なし".to_string(), |k| k.len().to_string());
    let _ = writeln!(
        s,
        "allowed_kanji.txt: [joyo_2010] {}、[number_units] {}、[report_kyoiku] {kyoiku}",
        a.joyo.len(),
        a.number_units.len()
    );
    for f in &g.faces {
        let n = f.glyphs.iter().filter(|g| g.bitmap.is_some()).count();
        let _ = writeln!(
            s,
            "{}: グリフ {n}、ascent {}、descent {}、行送り {}、contrast {}",
            f.def.id, f.ascent, f.descent, f.def.line_height, f.def.contrast
        );
    }
    let _ = writeln!(s, "グリフ: {}", g.positions.len());
    let h = c.strip.height.max(1);
    let _ = writeln!(
        s,
        "帯の使用: {}/{} 行({}%)",
        g.rows_used,
        c.strip.height,
        g.rows_used * 100 / h
    );
    let _ = writeln!(s, "PNG: {} バイト", g.png.len());
    let _ = writeln!(
        s,
        "ja.toml: キー {}、使っている漢字 {}",
        c.texts.entries.len(),
        c.texts.kanji.len()
    );
    list(&mut s, &c.texts.kanji);
    match non_kyoiku(g) {
        Some(set) => {
            let _ = writeln!(s, "教育漢字でない字: {}", set.len());
            list(&mut s, &set);
        }
        None => {
            let _ = writeln!(s, "教育漢字でない字: [report_kyoiku] がないので出さない");
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        for (c, k) in [
            (' ', Kind::Ascii),
            ('~', Kind::Ascii),
            ('ぁ', Kind::Hiragana),
            ('ん', Kind::Hiragana),
            ('ァ', Kind::Katakana),
            ('ヶ', Kind::Katakana),
            ('ー', Kind::Katakana),
            ('・', Kind::Symbol),
            ('、', Kind::Symbol),
            ('〜', Kind::Symbol),
            ('電', Kind::Kanji),
            ('々', Kind::Kanji),
        ] {
            assert_eq!(Kind::of(c), k, "{c}");
        }
    }
}
