mod common;

use std::collections::BTreeSet;

use common::{any_bignum, big, config};
use proptest::prelude::*;
use tm_numeric::{
    BigNum, FormatStyle, JA_UNIT_THRESHOLDS, JA_UNITS, OUTPUT_CHARS, UnitScript, format_ja,
    format_ja_into, ja_unit_index,
};

const NO_YEN: FormatStyle = FormatStyle {
    units: UnitScript::Kanji,
    append_yen: false,
};
const HIRAGANA: FormatStyle = FormatStyle {
    units: UnitScript::Hiragana,
    append_yen: true,
};

const GRADE_1: &str = "一右雨円王音下火花貝学気九休玉金空月犬見五口校左三山子四糸字耳七車手十出女小上森人水正生青夕石赤千川先早草足村大男竹中虫町天田土二日入年白八百文木本名目立力林六";
const GRADE_2: &str = "引羽雲園遠何科夏家歌画回会海絵外角楽活間丸岩顔汽記帰弓牛魚京強教近兄形計元言原戸古午後語工公広交光考行高黄合谷国黒今才細作算止市矢姉思紙寺自時室社弱首秋週春書少場色食心新親図数西声星晴切雪船線前組走多太体台地池知茶昼長鳥朝直通弟店点電刀冬当東答頭同道読内南肉馬売買麦半番父風分聞米歩母方北毎妹万明鳴毛門夜野友用曜来里理話";
const NUMBER_UNITS: &str = "億兆垓秭穣溝澗載極恒河沙阿僧祇那由他不可議無量";

const DISPLAY_UNITS: [&str; 20] = [
    "",
    "万",
    "億",
    "兆",
    "京",
    "垓",
    "秭",
    "穣",
    "溝",
    "澗",
    "正",
    "載",
    "極",
    "恒河沙",
    "阿僧祇",
    "那由他",
    "不可思議",
    "無量大数",
    "万無量大数",
    "億無量大数",
];

fn pow10(n: u32) -> BigNum {
    BigNum::pow10(n)
}

fn dec(d: u64, n: u32) -> BigNum {
    BigNum::from_u64(d).saturating_mul(pow10(n))
}

fn yen(v: BigNum) -> String {
    format_ja(v, &FormatStyle::DEFAULT)
}

fn chars(s: &str) -> BTreeSet<char> {
    s.chars().collect()
}

/// d × 10^n を 10 進の文字列から 01 §4.4 の規則で整形した期待値。
fn expected_dec(d: u64, n: u32) -> String {
    let digits = format!("{d}{}", "0".repeat(n as usize));
    let len = digits.len();
    let k = (len - 1) / 4;
    if k >= 20 {
        return "9999億無量大数+".to_string();
    }
    let split = len - 4 * k;
    let top: u64 = digits[..split].parse().unwrap();
    let low: u64 = if k > 0 {
        digits[split..split + 4].parse().unwrap()
    } else {
        0
    };
    if top < 10 && low > 0 {
        let head = DISPLAY_UNITS[k]
            .strip_suffix("無量大数")
            .filter(|_| k > 17)
            .unwrap_or(DISPLAY_UNITS[k]);
        format!("{top}{head}{low}{}", DISPLAY_UNITS[k - 1])
    } else {
        format!("{top}{}", DISPLAY_UNITS[k])
    }
}

#[test]
fn examples_from_design() {
    assert_eq!(yen(BigNum::from_u64(56_789)), "5万6789円");
    assert_eq!(yen(BigNum::from_u64(123_456_789)), "1億2345万円");
    assert_eq!(yen(BigNum::from_u64(98_765_432_100)), "987億円");
    assert_eq!(yen(dec(34, 71)), "3万4000無量大数円");
    assert_eq!(yen(BigNum::MAX), "9999億無量大数+円");
    assert_eq!(format_ja(BigNum::from_u64(123_456), &NO_YEN), "12万");
}

#[test]
fn boundaries() {
    let cases: [(BigNum, &str); 22] = [
        (BigNum::ZERO, "0円"),
        (BigNum::ONE.div_small(2), "0円"),
        (BigNum::ONE, "1円"),
        (BigNum::from_u64(9_999), "9999円"),
        (big(9_999.75), "9999円"),
        (BigNum::from_u64(10_000), "1万円"),
        (BigNum::from_u64(10_001), "1万1円"),
        (BigNum::from_u64(10_100), "1万100円"),
        (BigNum::from_u64(99_999), "9万9999円"),
        (BigNum::from_u64(100_000), "10万円"),
        (BigNum::from_u64(99_999_999), "9999万円"),
        (BigNum::from_u64(100_000_000), "1億円"),
        (BigNum::from_u64(100_000_001), "1億円"),
        (BigNum::from_u64(100_010_000), "1億1万円"),
        (pow10(68), "1無量大数円"),
        (dec(2, 68), "2無量大数円"),
        (dec(12_345, 64), "1無量大数2345不可思議円"),
        (pow10(72), "1万無量大数円"),
        (dec(3, 76), "3億無量大数円"),
        (dec(34, 75), "3億4000万無量大数円"),
        (dec(9_999, 76), "9999億無量大数円"),
        (pow10(80), "9999億無量大数+円"),
    ];
    for (v, expected) in cases {
        assert_eq!(yen(v), expected, "{v:?}");
    }
}

/// 表示は相対 2^-40 だけ持ち上げる(format.rs の NUDGE)。それより下は切り捨てる。
#[test]
fn near_thresholds() {
    let below_nudge = BigNum::ONE
        .checked_sub(BigNum::ONE.div_small(1 << 30))
        .unwrap();
    for k in 1..=20usize {
        let t = JA_UNIT_THRESHOLDS[k];
        let next_below = BigNum::from_bits_checked(t.to_bits() - 1).unwrap();
        let reached = if k == 20 {
            "9999億無量大数+円".to_string()
        } else {
            format!("1{}円", DISPLAY_UNITS[k])
        };
        assert_eq!(yen(next_below), reached, "k = {k}");
        assert_eq!(yen(t), reached, "k = {k}");
        let below = t.saturating_mul(below_nudge);
        assert_eq!(
            yen(below),
            format!("9999{}円", DISPLAY_UNITS[k - 1]),
            "k = {k}"
        );
    }
}

#[test]
fn round_decimals_format_exactly() {
    let ds = [
        1u64, 2, 5, 9, 10, 12, 34, 99, 100, 101, 999, 1_000, 1_001, 1_234, 5_000, 9_999, 10_000,
        12_345, 50_000, 99_999, 1_234_567, 99_999_999,
    ];
    for &d in &ds {
        for n in 0..=84u32 {
            assert_eq!(format_ja(dec(d, n), &NO_YEN), expected_dec(d, n), "{d}e{n}");
        }
    }
}

#[test]
fn ja_unit_index_boundaries() {
    assert_eq!(ja_unit_index(BigNum::ZERO), 0);
    assert_eq!(ja_unit_index(BigNum::ONE.div_small(3)), 0);
    assert_eq!(ja_unit_index(BigNum::from_u64(9_999)), 0);
    for k in 1..=17u8 {
        let t = JA_UNIT_THRESHOLDS[usize::from(k)];
        let below = BigNum::from_bits_checked(t.to_bits() - 1).unwrap();
        assert_eq!(ja_unit_index(t), k);
        assert_eq!(ja_unit_index(below), k - 1);
    }
    assert_eq!(ja_unit_index(pow10(72)), 17);
    assert_eq!(ja_unit_index(pow10(80)), 17);
    assert_eq!(ja_unit_index(BigNum::MAX), 17);
}

#[test]
fn format_into_clears_and_reuses() {
    let mut out = String::from("junk");
    format_ja_into(BigNum::from_u64(56_789), &FormatStyle::DEFAULT, &mut out);
    assert_eq!(out, "5万6789円");
    format_ja_into(BigNum::from_u64(7), &NO_YEN, &mut out);
    assert_eq!(out, "7");
}

#[test]
fn hiragana_units() {
    assert_eq!(
        format_ja(BigNum::from_u64(56_789), &HIRAGANA),
        "5まん6789円"
    );
    assert_eq!(
        format_ja(dec(34, 71), &HIRAGANA),
        "3まん4000むりょうたいすう円"
    );
    assert_eq!(
        format_ja(BigNum::MAX, &HIRAGANA),
        "9999おくむりょうたいすう+円"
    );
}

#[test]
fn ja_units_match_design() {
    let expected = [
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
    assert_eq!(JA_UNITS, expected);
    assert_eq!(JA_UNITS[6].0.chars().next(), Some('\u{79ED}'));
    for (_, yomi) in JA_UNITS {
        assert!(yomi.chars().all(|c| ('ぁ'..='ん').contains(&c)), "{yomi}");
    }
}

/// NUDGE の大きさを 2^-40 に固定する: 2^-41 下は繰り上がり、2^-39 下は繰り上がらない。
#[test]
fn nudge_is_two_to_minus_40() {
    let factor = |e: u64| {
        BigNum::ONE
            .checked_sub(BigNum::ONE.div_small(1 << e))
            .unwrap()
    };
    for k in 1..=19usize {
        let t = JA_UNIT_THRESHOLDS[k];
        assert_eq!(
            yen(t.saturating_mul(factor(41))),
            format!("1{}円", DISPLAY_UNITS[k]),
            "k = {k}"
        );
        assert_eq!(
            yen(t.saturating_mul(factor(39))),
            format!("9999{}円", DISPLAY_UNITS[k - 1]),
            "k = {k}"
        );
    }
}

#[test]
fn output_chars_match_appendix_c() {
    assert_eq!(GRADE_1.chars().count(), 80);
    assert_eq!(GRADE_2.chars().count(), 160);
    assert_eq!(NUMBER_UNITS.chars().count(), 23);
    assert_eq!(OUTPUT_CHARS.chars().count(), chars(OUTPUT_CHARS).len());

    let grades = &chars(GRADE_1) | &chars(GRADE_2);
    let units = chars(NUMBER_UNITS);
    assert!(grades.is_disjoint(&units));
    let mut extra = BTreeSet::new();
    for c in OUTPUT_CHARS.chars() {
        if c.is_ascii() {
            assert!(c.is_ascii_digit() || c == '+', "{c}");
        } else if !grades.contains(&c) {
            assert!(units.contains(&c), "{c} は付録 C にない");
            extra.insert(c);
        }
    }
    assert_eq!(
        extra, units,
        "1・2 年にない単位の字が付録 C の 23 字と一致しない"
    );

    let mut derived = chars("0123456789円+");
    for (unit, _) in JA_UNITS {
        derived.extend(unit.chars());
    }
    assert_eq!(chars(OUTPUT_CHARS), derived);
}

fn assert_output_covered(v: BigNum) {
    let allowed = chars(OUTPUT_CHARS);
    let mut hiragana_allowed = allowed.clone();
    for (_, yomi) in JA_UNITS {
        hiragana_allowed.extend(yomi.chars());
    }
    for style in [FormatStyle::DEFAULT, NO_YEN] {
        let s = format_ja(v, &style);
        assert!(s.chars().all(|c| allowed.contains(&c)), "{s}");
    }
    let s = format_ja(v, &HIRAGANA);
    assert!(s.chars().all(|c| hiragana_allowed.contains(&c)), "{s}");
}

#[test]
fn every_unit_output_is_covered() {
    for n in 0..=81 {
        assert_output_covered(pow10(n));
        assert_output_covered(dec(12_345, n));
    }
    assert_output_covered(BigNum::MAX);
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn output_chars_cover_any_value(v in any_bignum()) {
        assert_output_covered(v);
    }

    #[test]
    fn displayed_top_is_in_range(v in any_bignum()) {
        let s = format_ja(v, &NO_YEN);
        let top: String = s.chars().take_while(char::is_ascii_digit).collect();
        let top: u64 = top.parse().unwrap();
        prop_assert!(top <= 9_999, "{}", s);
        prop_assert!(top >= 1 || v < BigNum::ONE, "{}", s);
    }

    #[test]
    fn unit_index_is_monotone(a in any_bignum(), b in any_bignum()) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(ja_unit_index(lo) <= ja_unit_index(hi));
    }
}
