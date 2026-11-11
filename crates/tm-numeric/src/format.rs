use core::fmt::Write;

use crate::{BigNum, JA_UNIT_THRESHOLDS};

/// 単位の書き方。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitScript {
    /// 漢字(「万」)。
    Kanji,
    /// ひらがな(「まん」)。01 D-10 の覆す条件に備える。
    Hiragana,
}

/// 数の表示の書式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatStyle {
    /// 単位の書き方。
    pub units: UnitScript,
    /// 末尾に「円」を付けるか。
    pub append_yen: bool,
}

impl FormatStyle {
    /// 漢字の単位で「円」を付ける。
    pub const DEFAULT: FormatStyle = FormatStyle {
        units: UnitScript::Kanji,
        append_yen: true,
    };
}

/// 単位の表(漢字, 読み)[01 §4.4]。k = 0..=17 が 10^(4k)。k = 0 は単位なし。
pub const JA_UNITS: [(&str, &str); 18] = [
    ("", ""),
    ("万", "まん"),
    ("億", "おく"),
    ("兆", "ちょう"),
    ("京", "けい"),
    ("垓", "がい"),
    ("秭", "じょ"),
    ("穣", "じょう"),
    ("溝", "こう"),
    ("澗", "かん"),
    ("正", "せい"),
    ("載", "さい"),
    ("極", "ごく"),
    ("恒河沙", "ごうがしゃ"),
    ("阿僧祇", "あそうぎ"),
    ("那由他", "なゆた"),
    ("不可思議", "ふかしぎ"),
    ("無量大数", "むりょうたいすう"),
];

/// `format_ja` が漢字の書式で出しうる字 [02 §7.1]: 数字、単位の漢字すべて、「円」「+」。
pub const OUTPUT_CHARS: &str =
    "0123456789万億兆京垓秭穣溝澗正載極恒河沙阿僧祇那由他不可思議無量大数円+";

const MURYOTAISU: usize = 17;
const OVERFLOW: usize = 20;
// 表示だけ相対 2^-40 持ち上げる。10^37 が丸め誤差で「9澗9999溝」になるのを防ぐ。
const NUDGE: f64 = 1.0 + 1.0 / 1_099_511_627_776.0;

/// 表示の単位の番号(0..=19)。18、19 は「万無量大数」「億無量大数」。
fn display_unit(v: f64) -> usize {
    let mut k = 0;
    while k + 1 < OVERFLOW && JA_UNIT_THRESHOLDS[k + 1].0 <= v {
        k += 1;
    }
    k
}

fn head(k: usize, script: UnitScript) -> &'static str {
    pick(
        JA_UNITS[if k > MURYOTAISU { k - MURYOTAISU } else { k }],
        script,
    )
}

fn tail(k: usize, script: UnitScript) -> &'static str {
    if k > MURYOTAISU {
        pick(JA_UNITS[MURYOTAISU], script)
    } else {
        ""
    }
}

fn pick(unit: (&'static str, &'static str), script: UnitScript) -> &'static str {
    match script {
        UnitScript::Kanji => unit.0,
        UnitScript::Hiragana => unit.1,
    }
}

fn push_uint(out: &mut String, n: u64) {
    let _ = write!(out, "{n}");
}

/// `v` が届いている単位の番号 k(0..=17)。10^(4k) ≤ v となる最大の k(なければ 0)。log10 は使わない [01 §4.4]。
pub fn ja_unit_index(v: BigNum) -> u8 {
    display_unit(v.0).min(MURYOTAISU) as u8
}

/// 01 §4.4 の書式で整形する(例: 「5万6789円」「9999億無量大数+円」)。
pub fn format_ja(v: BigNum, style: &FormatStyle) -> String {
    let mut out = String::new();
    format_ja_into(v, style, &mut out);
    out
}

/// `format_ja` の確保しない版。`out` は clear してから書く。
pub fn format_ja_into(v: BigNum, style: &FormatStyle, out: &mut String) {
    out.clear();
    let s = style.units;
    let w = v.0 * NUDGE;
    if w >= JA_UNIT_THRESHOLDS[OVERFLOW].0 {
        let k = OVERFLOW - 1;
        push_uint(out, 9999);
        out.push_str(head(k, s));
        out.push_str(tail(k, s));
        out.push('+');
    } else {
        let k = display_unit(w);
        if k == 0 {
            push_uint(out, w as u64);
        } else {
            // 上の桁と下の 4 桁を 1 回の割り算から取り、境界の丸めで食い違わないようにする。
            let u = ((w / JA_UNIT_THRESHOLDS[k - 1].0) as u64).clamp(10_000, 99_999_999);
            let (top, low) = (u / 10_000, u % 10_000);
            push_uint(out, top);
            out.push_str(head(k, s));
            if top < 10 && low > 0 {
                push_uint(out, low);
                out.push_str(head(k - 1, s));
                out.push_str(tail(k - 1, s));
            } else {
                out.push_str(tail(k, s));
            }
        }
    }
    if style.append_yen {
        out.push('円');
    }
}
