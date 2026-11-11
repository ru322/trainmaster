//! 漢字の判定と `allowed_kanji.txt` の読み込み(15 §4.2.3、§4.3)。

use crate::error::FontgenError;
use std::collections::BTreeSet;

/// `[joyo_2010]` の字数(常用漢字表の本表)。
pub const JOYO_COUNT: usize = 2136;

/// 15 §4.3 の漢字の定義(CJK 統合漢字とその拡張、互換漢字、「々」「〆」「〇」)。
pub fn is_kanji(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF
        | 0x3005 | 0x3006 | 0x3007)
}

/// `allowed_kanji.txt` の区分。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllowedKanji {
    /// `[joyo_2010]`: 常用漢字表の 2136 字。
    pub joyo: BTreeSet<char>,
    /// `[number_units]`: 数の単位の字。
    pub number_units: BTreeSet<char>,
    /// `[report_kyoiku]`: 教育漢字(`report` だけで使う)。区分がなければ `None`。
    pub kyoiku: Option<BTreeSet<char>>,
}

/// ファイル全体の誤り(行を持たない)に使う行番号。
pub const WHOLE_FILE: usize = 0;

const SECTIONS: [&str; 3] = ["joyo_2010", "number_units", "report_kyoiku"];

impl AllowedKanji {
    /// 文言で使ってよい字か(`[joyo_2010]` か `[number_units]` にある)。
    pub fn contains(&self, c: char) -> bool {
        self.joyo.contains(&c) || self.number_units.contains(&c)
    }

    /// 単位の語の中でだけ使える字か(`[number_units]` にあり `[joyo_2010]` にない。FG-E07)。
    pub fn unit_only(&self, c: char) -> bool {
        self.number_units.contains(&c) && !self.joyo.contains(&c)
    }

    /// 書式を検査して読む(FG-E03)。
    pub fn parse(text: &str) -> Result<AllowedKanji, FontgenError> {
        let err = |line: usize, ch: Option<char>, reason: &str| FontgenError::KanjiList {
            line,
            ch,
            reason: reason.into(),
        };
        let mut sets: [Option<BTreeSet<char>>; 3] = [None, None, None];
        let mut current: Option<usize> = None;
        for (i, raw) in text.lines().enumerate() {
            let line_no = i + 1;
            let line = raw.split('#').next().unwrap_or("");
            let trimmed = line.trim_matches([' ', '\u{3000}', '\t', '\r']);
            if trimmed.is_empty() {
                continue;
            }
            if let Some(name) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                let Some(k) = SECTIONS.iter().position(|s| *s == name) else {
                    return Err(err(line_no, None, &format!("知らない区分 [{name}]")));
                };
                if sets[k].is_some() {
                    return Err(err(line_no, None, &format!("区分 [{name}] が 2 回ある")));
                }
                sets[k] = Some(BTreeSet::new());
                current = Some(k);
                continue;
            }
            for c in trimmed
                .chars()
                .filter(|c| !matches!(c, ' ' | '\u{3000}' | '\t'))
            {
                let Some(set) = current.and_then(|k| sets[k].as_mut()) else {
                    return Err(err(line_no, Some(c), "区分の外の字"));
                };
                if !is_kanji(c) {
                    return Err(err(line_no, Some(c), "漢字でない字"));
                }
                if !set.insert(c) {
                    return Err(err(line_no, Some(c), "同じ区分の中で重複"));
                }
            }
        }
        let [joyo, number_units, kyoiku] = sets;
        let (Some(joyo), Some(number_units)) = (joyo, number_units) else {
            return Err(err(
                WHOLE_FILE,
                None,
                "必須の区分 [joyo_2010] と [number_units] の両方が要る",
            ));
        };
        if joyo.len() != JOYO_COUNT {
            return Err(err(
                WHOLE_FILE,
                None,
                &format!("[joyo_2010] が {} 字(ちょうど {JOYO_COUNT} 字)", joyo.len()),
            ));
        }
        if let Some(&c) = kyoiku.iter().flatten().find(|c| !joyo.contains(c)) {
            return Err(err(
                WHOLE_FILE,
                Some(c),
                "[report_kyoiku] の字が [joyo_2010] にない",
            ));
        }
        Ok(AllowedKanji {
            joyo,
            number_units,
            kyoiku,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joyo_block() -> String {
        (0x8000..0x8000 + JOYO_COUNT as u32)
            .filter_map(char::from_u32)
            .collect()
    }

    fn file(extra: &str) -> String {
        format!(
            "[joyo_2010]\n{}\n[number_units]\n億兆垓\n{extra}",
            joyo_block()
        )
    }

    fn code(text: &str) -> &'static str {
        AllowedKanji::parse(text).unwrap_err().code()
    }

    #[test]
    fn kanji_ranges() {
        for c in [
            '一', '龥', '㐀', '\u{F900}', '\u{FAFF}', '𥝱', '𠮟', '々', '〆', '〇', '秭',
        ] {
            assert!(is_kanji(c), "{c}");
        }
        for c in [
            'あ', 'ア', 'A', '、', '〜', 'ー', '\u{3000}', '\u{4DC0}', '\u{3004}',
        ] {
            assert!(!is_kanji(c), "{c}");
        }
    }

    #[test]
    fn parses_sections_comments_and_spaces() {
        let text = format!(
            "# c\n[number_units]  # x\n億 兆\u{3000}耀\t# 円\n\n[joyo_2010]\n{}\n[report_kyoiku]\n耀老 # x\n",
            joyo_block()
        );
        let a = AllowedKanji::parse(&text).unwrap();
        assert_eq!(a.joyo.len(), JOYO_COUNT);
        assert_eq!(a.number_units, "億兆耀".chars().collect());
        assert_eq!(a.kyoiku, Some("耀老".chars().collect()));
        assert!(a.contains('兆') && a.contains('耀') && a.contains('老') && !a.contains('円'));
        assert!(a.unit_only('億') && !a.unit_only('耀') && !a.unit_only('老'));
    }

    #[test]
    fn report_kyoiku_is_optional() {
        let a = AllowedKanji::parse(&file("")).unwrap();
        assert_eq!(a.kyoiku, None);
    }

    #[test]
    fn format_errors_are_e03() {
        let joyo = joyo_block();
        let short: String = joyo.chars().skip(1).collect();
        let cases = [
            file("[grade_1_2]\n一\n"),
            format!("一\n{}", file("")),
            file("あ\n"),
            format!("[joyo_2010]\n{joyo}\n[number_units]\n億億\n"),
            format!("[joyo_2010]\n{short}\n[number_units]\n億\n"),
            format!("[joyo_2010]\n{short}老\n[number_units]\n億\n"),
            format!("[joyo_2010]\n{joyo}\u{5600}\n[number_units]\n億\n"),
            format!("[joyo_2010]\n{joyo}\n"),
            "[number_units]\n億\n".into(),
            file("[number_units]\n京\n"),
            file("[report_kyoiku]\n耀億\n"),
            file("[report_kyoiku]\n耀\n[report_kyoiku]\n老\n"),
            file("[report_kyoiku]\n耀耀\n"),
        ];
        for (i, text) in cases.iter().enumerate() {
            assert_eq!(code(text), "FG-E03", "{i}");
        }
    }

    #[test]
    fn error_lines() {
        let e = AllowedKanji::parse("[number_units]\n億\n").unwrap_err();
        assert!(e.to_string().contains("ファイル全体"), "{e}");
        assert!(!e.to_string().contains("0 行目"), "{e}");
        let e = AllowedKanji::parse(&file("あ\n")).unwrap_err();
        assert!(e.to_string().contains(": 5 行目 「あ」"), "{e}");
    }

    #[test]
    fn committed_list() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/text/allowed_kanji.txt"
        ))
        .unwrap();
        let a = AllowedKanji::parse(&text).unwrap();
        assert_eq!(a.joyo.len(), JOYO_COUNT);
        for c in ['𠮟', '剝', '塡', '頰'] {
            assert!(a.joyo.contains(&c), "{c}");
        }
        for c in ['叱', '剥', '填', '頬'] {
            assert!(!a.contains(c), "{c}");
        }
        let joyo: String = a.joyo.iter().collect();
        assert_eq!(
            crate::sha256_hex(joyo.as_bytes()),
            "3fa93c1d4f65fd4dc43b8b608110743bb29eb6759cc513a7ad24bb684b2bc2ea"
        );
        assert_eq!(
            a.number_units,
            "億兆垓秭穣溝澗載極恒河沙阿僧祇那由他不可議無量"
                .chars()
                .collect()
        );
        assert_eq!(a.number_units.len(), 23);
        let unit_only: String = a.number_units.iter().filter(|&&c| a.unit_only(c)).collect();
        assert_eq!(unit_only, "垓澗祇秭穣阿");
        assert_eq!(a.kyoiku.as_ref().map(BTreeSet::len), Some(1026));
    }
}
