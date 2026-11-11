//! `ja.toml` の読み込み・検査と `texts.rs` の生成(15 §6)。

use crate::error::{FontgenError, TextPos};
use crate::kanji::{AllowedKanji, is_kanji};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// 値を差し込むプレースホルダの名前(13 §4.3、15 §6.4)。
pub const PLACEHOLDER_NAMES: [&str; 10] = [
    "money", "people", "n", "a", "b", "used", "slots", "m", "unit", "yomi",
];

/// `{btn:<名前>}` の名前(13 §4.3)。
pub const BUTTON_NAMES: [&str; 10] = [
    "confirm",
    "cancel",
    "max",
    "tab",
    "build",
    "bend",
    "recommend",
    "menu",
    "move",
    "lr",
];

/// キーの深さの上限(15 §6.2)。
pub const MAX_DEPTH: usize = 4;

/// 文言の数の上限(`TextId(u16)` の範囲)。
pub const MAX_TEXTS: usize = u16::MAX as usize + 1;

/// キーのパスを定数名にする(`tut.s03.p1` → `TUT_S03_P1`)。
pub fn const_name(key: &str) -> String {
    key.replace('.', "_").to_ascii_uppercase()
}

fn key_err(key: &str, reason: impl Into<String>) -> FontgenError {
    FontgenError::Key {
        key: key.into(),
        reason: reason.into(),
    }
}

fn valid_segment(s: &str) -> bool {
    let mut it = s.chars();
    it.next().is_some_and(|c| c.is_ascii_lowercase())
        && it.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn walk(
    table: &toml::Table,
    prefix: &str,
    depth: usize,
    out: &mut Vec<(String, String)>,
) -> Result<(), FontgenError> {
    for (k, v) in table {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        if !valid_segment(k) {
            return Err(key_err(&key, format!("`{k}` は ^[a-z][a-z0-9_]*$ でない")));
        }
        if depth > MAX_DEPTH {
            return Err(key_err(&key, format!("深さは {MAX_DEPTH} まで")));
        }
        match v {
            toml::Value::String(s) => out.push((key, s.clone())),
            toml::Value::Table(t) => walk(t, &key, depth + 1, out)?,
            _ => return Err(key_err(&key, "値は文字列だけ")),
        }
    }
    Ok(())
}

/// 文言の数を検査する(FG-E15)。
pub fn check_count(count: usize) -> Result<(), FontgenError> {
    if count > MAX_TEXTS {
        return Err(FontgenError::TooManyTexts { count });
    }
    Ok(())
}

/// `ja.toml` を (キー, 文) の列にする。キーのバイト列の辞書順(= `TextId` の順)。FG-E01、E04、E15。
pub fn parse_ja(text: &str, path: &str) -> Result<Vec<(String, String)>, FontgenError> {
    let table: toml::Table = toml::from_str(text).map_err(|e| FontgenError::Read {
        path: path.into(),
        message: e.to_string(),
    })?;
    let mut out = Vec::new();
    walk(&table, "", 1, &mut out)?;
    out.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut names: BTreeMap<String, &str> = BTreeMap::new();
    for (k, _) in &out {
        if let Some(prev) = names.insert(const_name(k), k) {
            return Err(key_err(
                k,
                format!("`{prev}` と定数名 {} が同じ", const_name(k)),
            ));
        }
    }
    check_count(out.len())?;
    Ok(out)
}

/// 文の字句。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// そのまま描く字(`{{` と `}}` は `{`、`}` になる)。
    Lit {
        /// 字。
        ch: char,
        /// 文の中の何文字目か(1 始まり)。
        index: usize,
    },
    /// プレースホルダ(`{` と `}` を除いた名前)。
    Ph {
        /// 名前。
        name: String,
        /// `{` の位置(1 始まり)。
        index: usize,
    },
}

fn known_name(name: &str) -> bool {
    match name.strip_prefix("btn:") {
        Some(b) => BUTTON_NAMES.contains(&b),
        None => PLACEHOLDER_NAMES.contains(&name),
    }
}

/// 文を字句に分ける(FG-E12)。
pub fn lex(value: &str, pos: &dyn Fn(usize) -> TextPos) -> Result<Vec<Token>, FontgenError> {
    let err = |index: usize, reason: String| FontgenError::Placeholder {
        pos: pos(index),
        reason,
    };
    let chars: Vec<char> = value.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let index = i + 1;
        match chars[i] {
            '{' if chars.get(i + 1) == Some(&'{') => {
                out.push(Token::Lit { ch: '{', index });
                i += 2;
            }
            '}' if chars.get(i + 1) == Some(&'}') => {
                out.push(Token::Lit { ch: '}', index });
                i += 2;
            }
            '{' => {
                let Some(len) = chars[i + 1..].iter().position(|&c| c == '}') else {
                    return Err(err(index, "閉じていない {".into()));
                };
                let name: String = chars[i + 1..i + 1 + len].iter().collect();
                let well_formed = !name.is_empty()
                    && name.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == ':'
                    });
                if !well_formed {
                    return Err(err(
                        index,
                        format!("プレースホルダの名前 `{name}` の書式の誤り(ASCII の小文字)"),
                    ));
                }
                if !known_name(&name) {
                    return Err(err(
                        index,
                        format!("知らないプレースホルダ `{{{name}}}`(13 §4.3 の表にない)"),
                    ));
                }
                out.push(Token::Ph { name, index });
                i += len + 2;
            }
            '}' => return Err(err(index, "対応する { がない }".into())),
            ch => {
                out.push(Token::Lit { ch, index });
                i += 1;
            }
        }
    }
    Ok(out)
}

/// 文の検査に使う字の規則。
#[derive(Debug, Clone, Copy)]
pub struct Rules<'a> {
    /// `ja.toml` のパス(メッセージ用)。
    pub file: &'a str,
    /// 漢字以外で使ってよい字(`[charset]`)。
    pub charset: &'a BTreeSet<char>,
    /// 許可漢字。
    pub allowed: &'a AllowedKanji,
    /// 単位の語(`JA_UNITS` の漢字)。
    pub unit_words: &'a [&'a str],
    /// 1 行の字数の警告の閾値。
    pub warn_line_chars: usize,
}

/// 検査を通った文言。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// キー。
    pub key: String,
    /// 文(テンプレートのまま)。
    pub value: String,
    /// プレースホルダの名前(出てくる順、重複なし)。
    pub placeholders: Vec<String>,
}

fn is_space(t: Option<&Token>) -> bool {
    matches!(t, Some(Token::Lit { ch: ' ', .. }))
}

fn is_newline(t: Option<&Token>) -> bool {
    matches!(t, Some(Token::Lit { ch: '\n', .. }))
}

fn check_spaces(tokens: &[Token], pos: &dyn Fn(usize) -> TextPos) -> Result<(), FontgenError> {
    let err = |t: &Token, reason| {
        let index = match t {
            Token::Lit { index, .. } | Token::Ph { index, .. } => *index,
        };
        FontgenError::Space {
            pos: pos(index),
            reason,
        }
    };
    for (i, t) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1);
        if is_space(Some(t)) {
            if i == 0 {
                return Err(err(t, "先頭の空白"));
            }
            if next.is_none() {
                return Err(err(t, "末尾の空白"));
            }
            if is_space(next) {
                return Err(err(t, "2 つ続いた空白"));
            }
            if is_newline(next) || is_newline(i.checked_sub(1).and_then(|p| tokens.get(p))) {
                return Err(err(t, "空白と改行が隣り合う"));
            }
        }
        if is_newline(Some(t)) && next.is_none() {
            return Err(err(t, "末尾の改行"));
        }
    }
    Ok(())
}

fn check_chars(
    tokens: &[Token],
    rules: &Rules<'_>,
    pos: &dyn Fn(usize) -> TextPos,
) -> Result<(), FontgenError> {
    for t in tokens {
        let Token::Lit { ch, index } = *t else {
            continue;
        };
        if ch == '\n' {
            continue;
        }
        if is_kanji(ch) {
            if !rules.allowed.contains(ch) {
                return Err(FontgenError::Kanji {
                    pos: pos(index),
                    ch,
                });
            }
        } else if !rules.charset.contains(&ch) {
            return Err(FontgenError::Char {
                pos: pos(index),
                ch,
            });
        }
    }
    let mut segment: Vec<(char, usize)> = Vec::new();
    for t in tokens {
        match *t {
            Token::Lit { ch, index } => segment.push((ch, index)),
            Token::Ph { .. } => {
                check_units(&segment, rules, pos)?;
                segment.clear();
            }
        }
    }
    check_units(&segment, rules, pos)
}

fn check_units(
    segment: &[(char, usize)],
    rules: &Rules<'_>,
    pos: &dyn Fn(usize) -> TextPos,
) -> Result<(), FontgenError> {
    let mut covered = vec![false; segment.len()];
    for word in rules.unit_words {
        let w: Vec<char> = word.chars().collect();
        if w.is_empty() || w.len() > segment.len() {
            continue;
        }
        for start in 0..=segment.len() - w.len() {
            if segment[start..start + w.len()]
                .iter()
                .map(|s| s.0)
                .eq(w.iter().copied())
            {
                covered[start..start + w.len()].fill(true);
            }
        }
    }
    for (&(ch, index), &ok) in segment.iter().zip(&covered) {
        if !ok && rules.allowed.unit_only(ch) {
            return Err(FontgenError::UnitChar {
                pos: pos(index),
                ch,
            });
        }
    }
    Ok(())
}

fn check_yen(tokens: &[Token], pos: &dyn Fn(usize) -> TextPos) -> Result<(), FontgenError> {
    for w in tokens.windows(2) {
        if let [Token::Ph { name, .. }, Token::Lit { ch: '円', index }] = w
            && name == "money"
        {
            return Err(FontgenError::YenAfterMoney { pos: pos(*index) });
        }
    }
    Ok(())
}

/// 1 つの文を検査し、プレースホルダの名前と警告を返す(FG-E05〜E07、E12〜E14)。
/// 漢字(`{…}` の中を除く)を `kanji` に足す。
pub fn check_entry(
    key: &str,
    value: &str,
    rules: &Rules<'_>,
    kanji: &mut BTreeSet<char>,
) -> Result<(Entry, Vec<String>), FontgenError> {
    let pos = |index: usize| TextPos {
        file: rules.file.into(),
        key: key.into(),
        index,
    };
    let tokens = lex(value, &pos)?;
    check_spaces(&tokens, &pos)?;
    check_chars(&tokens, rules, &pos)?;
    check_yen(&tokens, &pos)?;
    kanji.extend(tokens.iter().filter_map(|t| match *t {
        Token::Lit { ch, .. } if is_kanji(ch) => Some(ch),
        _ => None,
    }));
    let mut placeholders: Vec<String> = Vec::new();
    for t in &tokens {
        if let Token::Ph { name, .. } = t
            && !placeholders.contains(name)
        {
            placeholders.push(name.clone());
        }
    }
    let mut warnings = Vec::new();
    for (n, line) in tokens.split(|t| is_newline(Some(t))).enumerate() {
        let count = line
            .iter()
            .filter(|t| matches!(t, Token::Lit { ch, .. } if *ch != ' '))
            .count();
        if count > rules.warn_line_chars {
            warnings.push(format!(
                "{}: {} の {} 行目が {} 字(目安は {} 字まで)",
                rules.file,
                key,
                n + 1,
                count,
                rules.warn_line_chars
            ));
        }
    }
    let entry = Entry {
        key: key.into(),
        value: value.into(),
        placeholders,
    };
    Ok((entry, warnings))
}

/// 検査を通った全文言と警告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Texts {
    /// キーの辞書順(`TextId` の順)。
    pub entries: Vec<Entry>,
    /// 1 行の字数の警告(終了コードは 0)。
    pub warnings: Vec<String>,
    /// 文に出る漢字(`{…}` の中を除く)。
    pub kanji: BTreeSet<char>,
}

/// `ja.toml` を読んで全文言を検査する。
pub fn check_texts(ja: &str, rules: &Rules<'_>) -> Result<Texts, FontgenError> {
    let pairs = parse_ja(ja, rules.file)?;
    let mut entries = Vec::with_capacity(pairs.len());
    let mut warnings = Vec::new();
    let mut kanji = BTreeSet::new();
    for (k, v) in &pairs {
        let (e, w) = check_entry(k, v, rules, &mut kanji)?;
        entries.push(e);
        warnings.extend(w);
    }
    Ok(Texts {
        entries,
        warnings,
        kanji,
    })
}

/// Rust の文字列リテラル(`"` で囲む)。改行と制御文字はエスケープする。
pub fn rust_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `texts.rs` の中身(15 §6.3)。
pub fn texts_rs(texts: &Texts, source: &str) -> String {
    let mut s = format!("// @generated by tm-fontgen from {source} — 手で編集しない\n");
    s.push_str("pub mod txt {\n    use super::TextId;\n");
    for (i, e) in texts.entries.iter().enumerate() {
        let _ = writeln!(
            s,
            "    pub const {}: TextId = TextId({i});",
            const_name(&e.key)
        );
    }
    s.push_str("}\n");
    let _ = writeln!(s, "pub const TEXT_COUNT: usize = {};", texts.entries.len());
    s.push_str("static TEXTS: [&str; TEXT_COUNT] = [\n");
    for e in &texts.entries {
        let _ = writeln!(s, "    {},", rust_str(&e.value));
    }
    s.push_str("];\nstatic KEYS: [&str; TEXT_COUNT] = [\n");
    for e in &texts.entries {
        let _ = writeln!(s, "    {},", rust_str(&e.key));
    }
    s.push_str("];\nstatic PLACEHOLDERS: [&[&str]; TEXT_COUNT] = [\n");
    for e in &texts.entries {
        let names: Vec<String> = e.placeholders.iter().map(|p| rust_str(p)).collect();
        let _ = writeln!(s, "    &[{}],", names.join(", "));
    }
    s.push_str("];\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        chars: BTreeSet<char>,
        allowed: AllowedKanji,
        units: Vec<&'static str>,
    }

    fn fixture() -> Fixture {
        let allowed = AllowedKanji {
            joyo: "円電車会社大数思正万駅多億兆無量不可議那由他"
                .chars()
                .collect(),
            number_units: "億兆垓阿僧祇無量不可議那由他".chars().collect(),
            kyoiku: None,
        };
        let mut chars: BTreeSet<char> = (' '..='~').collect();
        chars.extend('\u{3041}'..='\u{3093}');
        chars.extend("ー、。「」〜".chars());
        Fixture {
            chars,
            allowed,
            units: vec![
                "万",
                "億",
                "兆",
                "垓",
                "正",
                "不可思議",
                "那由他",
                "阿僧祇",
                "無量大数",
            ],
        }
    }

    fn check(value: &str) -> Result<(Entry, Vec<String>), FontgenError> {
        let f = fixture();
        let rules = Rules {
            file: "ja.toml",
            charset: &f.chars,
            allowed: &f.allowed,
            unit_words: &f.units,
            warn_line_chars: 20,
        };
        check_entry("a.b", value, &rules, &mut BTreeSet::new())
    }

    fn code(value: &str) -> &'static str {
        check(value).unwrap_err().code()
    }

    #[test]
    fn const_names() {
        assert_eq!(const_name("tut.s03.p1"), "TUT_S03_P1");
        assert_eq!(
            const_name("hud.info.not_connected"),
            "HUD_INFO_NOT_CONNECTED"
        );
    }

    #[test]
    fn keys_sorted_by_bytes() {
        let v = parse_ja("[a]\nz = \"1\"\n[a_c]\nd = \"2\"\n[a.b]\nc = \"3\"\n", "f").unwrap();
        let keys: Vec<&str> = v.iter().map(|p| p.0.as_str()).collect();
        assert_eq!(keys, ["a.b.c", "a.z", "a_c.d"]);
    }

    #[test]
    fn key_errors_are_e04() {
        for src in [
            "[a_b]\nc = \"1\"\n[a]\nb_c = \"2\"\n",
            "[A]\nb = \"1\"\n",
            "[a]\n\"b-c\" = \"1\"\n",
            "[a]\nb = 1\n",
            "[a]\nb = [\"x\"]\n",
            "[a.b.c.d]\ne = \"x\"\n",
            "[a]\n1b = \"x\"\n",
        ] {
            assert_eq!(parse_ja(src, "f").unwrap_err().code(), "FG-E04", "{src}");
        }
        assert!(parse_ja("[a.b.c]\nd = \"x\"\n", "f").is_ok());
    }

    #[test]
    fn toml_syntax_is_e01() {
        assert_eq!(
            parse_ja("[a]\nb = \"x\"\nb = \"y\"\n", "f")
                .unwrap_err()
                .code(),
            "FG-E01"
        );
    }

    #[test]
    fn count_limit_is_e15() {
        assert!(check_count(MAX_TEXTS).is_ok());
        assert_eq!(check_count(MAX_TEXTS + 1).unwrap_err().code(), "FG-E15");
    }

    #[test]
    fn lexes_placeholders_and_escapes() {
        let pos = |index| TextPos {
            file: "f".into(),
            key: "k".into(),
            index,
        };
        let t = lex("a{{{n}}}{btn:confirm}", &pos).unwrap();
        assert_eq!(
            t,
            vec![
                Token::Lit { ch: 'a', index: 1 },
                Token::Lit { ch: '{', index: 2 },
                Token::Ph {
                    name: "n".into(),
                    index: 4
                },
                Token::Lit { ch: '}', index: 7 },
                Token::Ph {
                    name: "btn:confirm".into(),
                    index: 9
                },
            ]
        );
    }

    #[test]
    fn placeholder_errors_are_e12() {
        for v in [
            "{n",
            "n}",
            "{money",
            "{coins}",
            "{btn:jump}",
            "{}",
            "{Money}",
            "{円}",
        ] {
            assert_eq!(code(v), "FG-E12", "{v}");
        }
    }

    #[test]
    fn placeholders_in_order_without_duplicates() {
        let (e, _) = check("{btn:confirm}{n}/{m} {n}").unwrap();
        assert_eq!(e.placeholders, ["btn:confirm", "n", "m"]);
    }

    #[test]
    fn inside_placeholder_is_not_checked() {
        assert!(check("{unit}({yomi})に とどいたよ!").is_ok());
    }

    #[test]
    fn fullwidth_symbols_are_e05() {
        for v in [
            "すごい\u{FF01}",
            "なに\u{FF1F}",
            "\u{FF08}a\u{FF09}",
            "あ\u{3000}い",
            "あ\tい",
            "\u{FF71}",
            "\u{1F600}",
        ] {
            assert_eq!(code(v), "FG-E05", "{v}");
        }
    }

    #[test]
    fn unknown_kanji_is_e06() {
        assert_eq!(code("駅に行く"), "FG-E06");
        let e = check("繋ぐ").unwrap_err();
        assert_eq!(e.code(), "FG-E06");
        assert!(e.to_string().contains("U+7E4B"), "{e}");
        assert!(e.to_string().contains("常用漢字ではありません"), "{e}");
    }

    #[test]
    fn unit_only_char_outside_word_is_e07() {
        for v in ["阿", "阿僧", "僧祇", "阿の駅"] {
            assert_eq!(code(v), "FG-E07", "{v}");
        }
        for v in [
            "1無量大数円!",
            "1垓円",
            "垓の駅",
            "1阿僧祇円",
            "不可思議",
            "量が多い",
            "不",
            "他の駅",
            "那由",
        ] {
            assert!(check(v).is_ok(), "{v}");
        }
    }

    #[test]
    fn unit_word_split_by_placeholder_is_e07() {
        assert_eq!(code("阿{n}僧祇"), "FG-E07");
        assert!(check("無量{n}大数").is_ok());
    }

    #[test]
    fn collects_kanji_outside_placeholders() {
        let f = fixture();
        let rules = Rules {
            file: "ja.toml",
            charset: &f.chars,
            allowed: &f.allowed,
            unit_words: &f.units,
            warn_line_chars: 20,
        };
        let ja = "[a]\nx = \"駅と{unit}({yomi})\"\ny = \"会社の駅{{円}}\"\n";
        let t = check_texts(ja, &rules).unwrap();
        assert_eq!(t.kanji, "駅会社円".chars().collect());
        assert_eq!(check_texts(ja, &rules).unwrap(), t);
    }

    #[test]
    fn yen_after_money_is_e13() {
        assert_eq!(code("あと {money}円"), "FG-E13");
        assert!(check("あと {money}").is_ok());
        assert!(check("{n}円").is_ok());
    }

    #[test]
    fn space_errors_are_e14() {
        for v in [" あ", "あ ", "あ  い", "あ \nい", "あ\n い", "あ\n"] {
            assert_eq!(code(v), "FG-E14", "{v:?}");
        }
        assert!(check("あ\nい").is_ok());
    }

    #[test]
    fn long_line_warns_without_counting_spaces_and_placeholders() {
        let (_, w) = check("あいうえおかきくけこさしすせそたちつてと").unwrap();
        assert!(w.is_empty());
        let (_, w) = check("あいうえおかきくけこさしすせそたちつてとな").unwrap();
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("21 字"), "{}", w[0]);
        let (_, w) =
            check("あいうえお かきくけこ {money} さしすせそ たちつてと{btn:confirm}").unwrap();
        assert!(w.is_empty(), "{w:?}");
        let (_, w) = check("あいうえおかきくけこ\nさしすせそたちつてとなにぬねの").unwrap();
        assert!(w.is_empty());
    }

    #[test]
    fn rust_string_literal() {
        assert_eq!(rust_str("a\"b\\c\nd電"), "\"a\\\"b\\\\c\\nd電\"");
    }

    #[test]
    fn texts_rs_shape() {
        let f = fixture();
        let rules = Rules {
            file: "ja.toml",
            charset: &f.chars,
            allowed: &f.allowed,
            unit_words: &f.units,
            warn_line_chars: 20,
        };
        let t = check_texts(
            "[hud]\nincome = \"1びょうで {money}\"\n[expl]\npage = \"{n}/{m}\"\n",
            &rules,
        )
        .unwrap();
        let s = texts_rs(&t, "assets/text/ja.toml");
        assert_eq!(
            s,
            "// @generated by tm-fontgen from assets/text/ja.toml — 手で編集しない
pub mod txt {
    use super::TextId;
    pub const EXPL_PAGE: TextId = TextId(0);
    pub const HUD_INCOME: TextId = TextId(1);
}
pub const TEXT_COUNT: usize = 2;
static TEXTS: [&str; TEXT_COUNT] = [
    \"{n}/{m}\",
    \"1びょうで {money}\",
];
static KEYS: [&str; TEXT_COUNT] = [
    \"expl.page\",
    \"hud.income\",
];
static PLACEHOLDERS: [&[&str]; TEXT_COUNT] = [
    &[\"n\", \"m\"],
    &[\"money\"],
];
"
        );
    }
}
