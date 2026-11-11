use tm_assets::txt;

use super::*;

fn lay(src: &str, size: FontId, max_w: u16) -> TextBlock {
    let mut b = TextBlock::default();
    layout(src, size, max_w, &[], InputDevice::Keyboard, &mut b);
    b
}

fn line_starts(b: &TextBlock) -> Vec<i16> {
    let mut ys: Vec<i16> = Vec::new();
    for g in &b.glyphs {
        let line = g.y / i16::from(b.line_height);
        if ys.len() <= line as usize {
            ys.push(g.x);
        }
    }
    ys
}

#[test]
fn half_and_full_width_advances() {
    let b = lay("Lv12", FontId::Body12, u16::MAX);
    assert_eq!((b.width, b.lines, b.line_height), (4 * 6, 1, 16));
    let b = lay("えき", FontId::Body12, u16::MAX);
    assert_eq!(b.width, 2 * 12);
    let b = lay("えき", FontId::Head16, u16::MAX);
    assert_eq!((b.width, b.line_height), (2 * 16, 20));
}

#[test]
fn wraps_at_spaces_and_drops_leading_space() {
    let b = lay("えきと えきを せんろで つなごう", FontId::Body12, 90);
    assert_eq!(b.lines, 3);
    assert!(b.width <= 90);
    let first_line_glyphs = b.glyphs.iter().filter(|g| g.y < 16).count();
    assert_eq!(first_line_glyphs, 6, "「えきと えきを」で折り返す");
    assert!(
        line_starts(&b).iter().all(|&x| x < 4),
        "行頭の空白は描かない"
    );
}

#[test]
fn explicit_newline() {
    let b = lay("あ\nい", FontId::Body12, u16::MAX);
    assert_eq!(b.lines, 2);
    assert_eq!(b.width, 12);
    assert!(b.glyphs[1].y >= 16);
}

#[test]
fn long_word_breaks_between_chars_with_kinsoku() {
    let b = lay("ああああ", FontId::Body12, 30);
    assert_eq!(b.lines, 2);
    let b = lay("あああ。", FontId::Body12, 36);
    assert_eq!(b.lines, 1, "句点は前の行に追い出す");
    assert_eq!(b.width, 48);
    let b = lay("ああっあ", FontId::Body12, 24);
    assert_eq!(b.lines, 2);
    let second: Vec<_> = b.glyphs.iter().filter(|g| g.y >= 16).collect();
    assert_eq!(second.len(), 1, "「っ」は行頭に来ない");
}

#[test]
fn buttons_follow_device_and_are_not_tinted() {
    let mut b = TextBlock::default();
    layout(
        text::text(txt::TITLE_HINT),
        FontId::Body12,
        u16::MAX,
        &[],
        InputDevice::Keyboard,
        &mut b,
    );
    assert_eq!(b.glyphs[0].sprite, sprite::BTN_KEY_Z);
    assert!(b.glyphs[0].plain && !b.glyphs[1].plain);
    assert_eq!(
        (b.glyphs[0].y, b.glyphs[0].w, b.glyphs[0].h),
        (0, 16, 16),
        "ボタンの絵は Body12 の行の高さ 16 にちょうど入る"
    );
    layout(
        text::text(txt::TITLE_HINT),
        FontId::Body12,
        u16::MAX,
        &[],
        InputDevice::Gamepad,
        &mut b,
    );
    assert_eq!(b.glyphs[0].sprite, sprite::BTN_PAD_A);
    layout(
        text::text(txt::CHIP_PANEL),
        FontId::Body12,
        u16::MAX,
        &[],
        InputDevice::Gamepad,
        &mut b,
    );
    assert_eq!(b.glyphs[0].sprite, sprite::BTN_PAD_RB);
    for name in [
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
    ] {
        for dev in [InputDevice::Keyboard, InputDevice::Gamepad] {
            let s = button_sprite(name, dev).expect(name);
            assert_eq!((sprite::meta(s).w, sprite::meta(s).h), (16, 16));
        }
    }
}

#[test]
fn placeholders_are_filled() {
    let mut b = TextBlock::default();
    layout(
        text::text(txt::HUD_INCOME),
        FontId::Body12,
        u16::MAX,
        &[TextArg {
            name: "money",
            value: "56円",
        }],
        InputDevice::Keyboard,
        &mut b,
    );
    assert_eq!(b.width, 6 + 2 * 12 + 6 + 2 * 6 + 12);
}

#[test]
fn glyph_positions_follow_ascent_and_offsets() {
    let b = lay("A", FontId::Body12, u16::MAX);
    let g = font::glyph(FontId::Body12, 'A').unwrap().bitmap.unwrap();
    assert_eq!(b.glyphs[0].sprite, g.sprite);
    assert_eq!(i32::from(b.glyphs[0].x), i32::from(g.x_off));
    let ascent = i32::from(font::face(FontId::Body12).ascent);
    assert_eq!(i32::from(b.glyphs[0].y), ascent + i32::from(g.y_off));
}

#[test]
fn empty_text_has_no_lines() {
    let b = lay("", FontId::Body12, 100);
    assert_eq!((b.lines, b.width, b.glyphs.len()), (0, 0, 0));
}

#[test]
fn text_cache_hits_and_clears_at_the_limit() {
    let mut c = TextCache::default();
    let a = c.get(txt::CHIP_BUILD, FontId::Body12, 216, InputDevice::Keyboard);
    let b = c.get(txt::CHIP_BUILD, FontId::Body12, 216, InputDevice::Keyboard);
    assert_eq!(a, b);
    let g = c.get(txt::CHIP_BUILD, FontId::Body12, 216, InputDevice::Gamepad);
    assert_ne!(a, g);
    assert_eq!(c.block(g).glyphs[0].sprite, sprite::BTN_PAD_X);
    for w in 0..600u16 {
        c.get(txt::COMMON_YES, FontId::Body12, w, InputDevice::Keyboard);
    }
    assert_eq!(c.len(), 602);
    c.begin_frame();
    assert_eq!(c.len(), 0);
}

#[test]
fn dyn_text_relayouts_only_on_change() {
    let mut d = DynText::default();
    let n = |v| {
        [TextArg {
            name: "n",
            value: v,
        }]
    };
    assert!(d.set(
        txt::HUD_COMPANY_LEVEL,
        &n("3"),
        FontId::Body12,
        300,
        InputDevice::Keyboard
    ));
    assert!(!d.set(
        txt::HUD_COMPANY_LEVEL,
        &n("3"),
        FontId::Body12,
        300,
        InputDevice::Keyboard
    ));
    assert_eq!(d.relayouts(), 1);
    assert!(d.set(
        txt::HUD_COMPANY_LEVEL,
        &n("16"),
        FontId::Body12,
        300,
        InputDevice::Keyboard
    ));
    assert_eq!(d.block.width, 5 * 12 + 6 + 2 * 6);
    assert!(d.set(
        txt::HUD_COMPANY_LEVEL,
        &n("16"),
        FontId::Body12,
        300,
        InputDevice::Gamepad
    ));
    assert_eq!(d.relayouts(), 3);
}

const ALLOWED_PLACEHOLDERS: [&str; 10] = [
    "n", "a", "b", "used", "slots", "m", "money", "people", "unit", "yomi",
];

#[test]
fn all_texts_use_known_placeholders_and_buttons() {
    for i in 0..text::TEXT_COUNT {
        let id = TextId(i as u16);
        let src = text::text(id);
        assert_eq!(
            src.matches('{').count(),
            src.matches('}').count(),
            "{} の括弧",
            text::key(id)
        );
        for p in text::placeholders(id) {
            match p.strip_prefix("btn:") {
                Some(b) => assert!(
                    button_sprite(b, InputDevice::Keyboard).is_some(),
                    "{}: {p}",
                    text::key(id)
                ),
                None => assert!(ALLOWED_PLACEHOLDERS.contains(p), "{}: {p}", text::key(id)),
            }
        }
    }
}

const LONGEST_MONEY: &str = "1無量大数2345不可思議円";

fn fill_longest(id: TextId, size: FontId, max_w: u16) -> TextBlock {
    let args: Vec<TextArg<'_>> = text::placeholders(id)
        .iter()
        .filter(|p| !p.starts_with("btn:"))
        .map(|&p| TextArg {
            name: p,
            value: match p {
                "money" => LONGEST_MONEY,
                "people" => "1無量大数2345不可思議",
                "unit" => "無量大数",
                "yomi" => "むりょうたいすう",
                _ => "16",
            },
        })
        .collect();
    let mut b = TextBlock::default();
    layout(
        text::text(id),
        size,
        max_w,
        &args,
        InputDevice::Gamepad,
        &mut b,
    );
    b
}

/// 説明の本文の内側の幅(376 = Head16 の全角 23 字)。
fn page_width() -> u16 {
    use crate::layout::{EXPLAIN_PAD, EXPLAIN_W};
    (EXPLAIN_W as i32 - 2 * EXPLAIN_PAD) as u16
}

#[test]
fn explanation_pages_fit_the_box() {
    use crate::layout::EXPLAIN_LINES;
    let w = page_width();
    for i in 0..text::TEXT_COUNT {
        let id = TextId(i as u16);
        let k = text::key(id);
        let is_page =
            k.starts_with("tut.s") && k.rsplit('.').next().is_some_and(|l| l.starts_with('p'));
        if is_page {
            let b = fill_longest(id, FontId::Head16, w);
            assert!(
                b.lines <= EXPLAIN_LINES && b.width <= w && !b.wrapped,
                "{k}: {} 行 {}px(自動の折り返し {})",
                b.lines,
                b.width,
                b.wrapped
            );
        }
    }
}

#[test]
fn band_messages_fit_one_line() {
    use crate::layout::BAND_TEXT_W;
    for i in 0..text::TEXT_COUNT {
        let id = TextId(i as u16);
        let k = text::key(id);
        let last = k.rsplit('.').next().unwrap_or("");
        let guide = k.starts_with("tut.s") && (last.starts_with('g') || last == "w");
        if guide || k.starts_with("err.") {
            let b = fill_longest(id, FontId::Body12, BAND_TEXT_W);
            assert!(
                b.lines == 1 && b.width <= BAND_TEXT_W,
                "{k}: {} 行 {}px",
                b.lines,
                b.width
            );
        }
    }
}

#[test]
fn hud_texts_fit_their_rows() {
    use crate::layout::{BAND_RIGHT, GOAL_RIGHT, HUD_PAD, LEVEL_RIGHT, MONEY_X};
    let w = |id, size| i32::from(fill_longest(id, size, u16::MAX).width);
    let money = i32::from(lay(LONGEST_MONEY, FontId::Body12, u16::MAX).width);
    assert_eq!(money, 138, "13 §3.5 の見積もり");
    let level = w(txt::HUD_COMPANY_LEVEL, FontId::Body12);
    assert!(
        MONEY_X + money + 8 <= LEVEL_RIGHT - level,
        "HUD 1 段目: {money} / {level}"
    );
    let (income, goal) = (
        w(txt::HUD_INCOME, FontId::Body12),
        w(txt::HUD_GOAL, FontId::Body12).max(w(txt::HUD_GOAL_DONE, FontId::Body12)),
    );
    assert!(
        HUD_PAD + income + 8 <= GOAL_RIGHT - goal,
        "HUD 2 段目: {income} / {goal}"
    );
    let build = w(txt::CHIP_BUILD, FontId::Body12);
    let panel = w(txt::CHIP_PANEL, FontId::Body12);
    assert!(
        build + 8 + panel + 2 + 12 <= BAND_RIGHT.w as i32,
        "13 §3.5.1 の下の帯の右: {build} + {panel}"
    );
}

#[test]
fn notices_fit_their_boxes() {
    use crate::layout::{BANNER, TOAST};
    let inner = BANNER.w as u16 - 16;
    assert_eq!(TOAST.w, BANNER.w);
    fits_one_line_in(&[txt::NOTICE_COMPANY_UP], FontId::Head16, inner);
    fits_one_line_in(
        &[
            txt::NOTICE_COMPANY_UP2,
            txt::NOTICE_UNLOCKED,
            txt::NOTICE_UNIT,
            txt::NOTICE_PASS,
        ],
        FontId::Body12,
        inner,
    );
}

fn fits_one_line_in(ids: &[TextId], size: FontId, max_w: u16) {
    for &id in ids {
        let b = fill_longest(id, size, max_w);
        assert!(
            b.lines == 1 && b.width <= max_w,
            "{}: {} 行 {}px(上限 {max_w}px)",
            text::key(id),
            b.lines,
            b.width
        );
    }
}

fn fits_one_line(ids: &[TextId], max_w: u16) {
    fits_one_line_in(ids, FontId::Body12, max_w);
}

#[test]
fn band_mode_texts_and_hints_fit() {
    use crate::layout::{BAND_RIGHT, BAND_TEXT_W};
    fits_one_line(
        &[
            txt::BUILD_CHOOSE_TITLE,
            txt::BUILD_CREATE_FIRST,
            txt::BUILD_CREATE_NEXT,
            txt::BUILD_EXTEND_END,
            txt::BUILD_EXTEND_WHICH,
            txt::BUILD_EXTEND_NEW,
        ],
        BAND_TEXT_W,
    );
    fits_one_line(
        &[
            txt::BUILD_HINT,
            txt::BUILD_CHOOSE_HINT,
            txt::BUILD_END_HINT,
            txt::BUILD_WHICH_HINT,
        ],
        BAND_RIGHT.w as u16,
    );
}

/// 13 §3.5.1: 情報の段の値段と不足分は、最長の数で 1 行に収まる。
#[test]
fn track_cost_row_fits() {
    use crate::layout::{INFO_GAP, INFO_TEXT_W};
    let w = |id| i32::from(fill_longest(id, FontId::Body12, u16::MAX).width);
    let row =
        w(txt::BUILD_COST) + INFO_GAP + crate::scene::build::COIN_W + w(txt::BUILD_COST_SHORT);
    assert!(row <= i32::from(INFO_TEXT_W), "{row}px");
}

#[test]
fn panel_row_notes_fit_with_an_icon() {
    let inner = crate::scene::panel::INNER_W;
    fits_one_line(
        &[
            txt::PANEL_NO_STATION,
            txt::PANEL_NO_LINE,
            txt::PANEL_LIMIT_COMPANY,
            txt::PANEL_LIMIT_INTRINSIC,
            txt::PANEL_LIMIT_PLATFORM,
            txt::PANEL_LIMIT_CAPACITY,
            txt::PANEL_LOCKED,
        ],
        inner - BUTTON,
    );
    fits_one_line(&[txt::PANEL_NEED_MORE, txt::PANEL_HINT], inner - 8);
    fits_one_line(&keys_ending("param.", ".desc"), inner - 8);
    let new = fill_longest(txt::PANEL_NEW, FontId::Body12, u16::MAX).width;
    let level = fill_longest(txt::PANEL_LEVEL_RANGE, FontId::Body12, u16::MAX).width;
    fits_one_line(&keys_ending("param.", ".name"), inner - new - 4 - 8 - level);
}

fn keys_ending(prefix: &str, suffix: &str) -> Vec<TextId> {
    let ids: Vec<TextId> = (0..text::TEXT_COUNT)
        .map(|i| TextId(i as u16))
        .filter(|&id| text::key(id).starts_with(prefix) && text::key(id).ends_with(suffix))
        .collect();
    assert!(!ids.is_empty(), "{prefix}*{suffix}");
    ids
}

#[test]
fn dialogs_and_settings_rows_fit() {
    use crate::layout::{BUILD_CHOICES, LINE_CONFIRM_W, SETTINGS, SUB_CONFIRM};
    let confirm = SUB_CONFIRM.w as u16 - 32;
    for id in [
        txt::RESET_CONFIRM1,
        txt::RESET_CONFIRM2,
        txt::TITLE_NEW_CONFIRM1,
        txt::QUIT_CONFIRM1,
        txt::QUIT_CONFIRM1_NOSAVE,
        txt::QUIT_CONFIRM2,
    ] {
        let b = fill_longest(id, FontId::Body12, confirm);
        assert!(b.lines <= 2 && b.width <= confirm, "{}", text::key(id));
    }
    fits_one_line(
        &[
            txt::BUILD_CONFIRM_TITLE,
            txt::BUILD_CONFIRM_PASS,
            txt::BUILD_COST,
        ],
        LINE_CONFIRM_W as u16 - 32,
    );
    fits_one_line(
        &[txt::BUILD_CHOOSE_CREATE, txt::BUILD_CHOOSE_EXTEND],
        BUILD_CHOICES.w as u16 - 24,
    );
    fits_one_line_in(
        &[txt::SETTINGS_TITLE, txt::SETTINGS_REPLAY],
        FontId::Head16,
        SETTINGS.w as u16 - 24,
    );
    fits_one_line(
        &[
            txt::SETTINGS_CONTINUE,
            txt::SETTINGS_REPLAY,
            txt::SETTINGS_QUIT,
            txt::SETTINGS_RESET,
        ],
        SETTINGS.w as u16 - 34,
    );
    fits_one_line(&keys_ending("tut.s", ".title"), SETTINGS.w as u16 - 34);
    fits_one_line(&[txt::BUILD_CREATE_FINISH], crate::scene::build::TAG_W);
    let ctrl = 120 - 22 - 4;
    fits_one_line(
        &[txt::SETTINGS_MUSIC, txt::SETTINGS_SOUND, txt::SETTINGS_SHOW],
        ctrl,
    );
}

#[test]
fn title_and_quit_texts_fit() {
    use crate::layout::{BAND_RIGHT, POWER_BOX, SAVING_BOX, TITLE_ROWS};
    fits_one_line_in(
        &[txt::TITLE_CONTINUE, txt::TITLE_NEW_GAME],
        FontId::Head16,
        TITLE_ROWS[0].w as u16 - 2 * 16,
    );
    fits_one_line(&[txt::TITLE_HINT], BAND_RIGHT.w as u16);
    fits_one_line(&[txt::QUIT_SAVING], SAVING_BOX.w as u16 - 2 * (12 + 4) - 16);
    let power_w = POWER_BOX.w as u16 - 32;
    let b = fill_longest(txt::QUIT_POWER_OFF, FontId::Head16, power_w);
    assert!(
        b.lines == 2 && b.width <= power_w,
        "{} 行 {}px",
        b.lines,
        b.width
    );
    assert!(i32::from(b.lines) * i32::from(b.line_height) <= POWER_BOX.h as i32 - 16);
}

fn plain(id: TextId) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text::text(id).chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// 書き直し前のかな書き、まぜ書き、使わない語 [13 §4.5、01 §10.3、付録 B]。
const BANNED: [&str; 28] = [
    "えき",
    "ろせん",
    "せんろ",
    "うんちん",
    "おきゃくさん",
    "せってい",
    "せつめい",
    "みりょく",
    "のばす",
    "のばし",
    "のばそ",
    "のりかえ",
    "乗りかえ",
    "つうか",
    "詰め",
    "線ろ",
    "ろ線",
    "運ちん",
    "通か",
    "決定",
    "列車",
    "延伸",
    "乗車",
    "混雑",
    "魅力",
    "カーソル",
    "タブ",
    "早く走",
];

#[test]
fn no_banned_words() {
    for i in 0..text::TEXT_COUNT {
        let id = TextId(i as u16);
        let s = plain(id);
        for w in BANNED {
            assert!(!s.contains(w), "{}: 「{w}」は使わない", text::key(id));
        }
    }
}

/// 分かち書きはしない。空白は数・プレースホルダとボタンの絵の前の区切りだけ [01 §10.3、18 §C.3]。
#[test]
fn spaces_only_before_placeholders() {
    for i in 0..text::TEXT_COUNT {
        let id = TextId(i as u16);
        let s = text::text(id);
        for (at, _) in s.match_indices(' ') {
            assert!(
                s[at + 1..].starts_with('{'),
                "{}: 分かち書きの空白「{s}」",
                text::key(id)
            );
        }
    }
}

/// 子どもが知っているカタカナ語と英字 [01 §10.3]。
const KNOWN_WORDS: [&str; 9] = [
    "ゲージ",
    "ゲーム",
    "レベル",
    "レベルアップ",
    "ホーム",
    "マップ",
    "MAX",
    "NEW",
    "Lv",
];

#[test]
fn katakana_words_outside_the_list_are_reported() {
    let kind = |c: char| match c {
        'ァ'..='ヶ' | 'ー' => 1,
        'A'..='Z' | 'a'..='z' => 2,
        _ => 0,
    };
    for i in 0..text::TEXT_COUNT {
        let id = TextId(i as u16);
        let s: Vec<char> = plain(id).chars().collect();
        for w in s.chunk_by(|a, b| kind(*a) == kind(*b)) {
            let word: String = w.iter().collect();
            if kind(w[0]) != 0 && w.len() >= 2 && !KNOWN_WORDS.contains(&word.as_str()) {
                eprintln!("警告: {}: 一覧にない語「{word}」", text::key(id));
            }
        }
    }
}

/// まだコードから使わない文言(理由つき)。
const NOT_YET_USED: [(&str, &str); 0] = [];

fn sources(dir: &std::path::Path, out: &mut String) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("ソースのディレクトリ")
        .map(|e| e.expect("エントリ").path())
        .collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if name != "tests" {
                sources(&p, out);
            }
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            out.push_str(&std::fs::read_to_string(&p).expect("ソース"));
        }
    }
}

#[test]
fn every_text_is_used_by_the_code() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut src = String::new();
    for c in ["tm-ui", "tm-tutorial", "tm-app"] {
        sources(&crates.join(c).join("src"), &mut src);
    }
    let mut unused = Vec::new();
    for i in 0..text::TEXT_COUNT {
        let name = text::key(TextId(i as u16)).replace('.', "_").to_uppercase();
        let used = src.match_indices(&format!("txt::{name}")).any(|(at, m)| {
            !src[at + m.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        let allowed = NOT_YET_USED.iter().any(|(n, _)| *n == name);
        assert!(
            !(used && allowed),
            "{name} は使われたので NOT_YET_USED から外す"
        );
        if !used && !allowed {
            unused.push(name);
        }
    }
    assert!(
        unused.is_empty(),
        "どこからも使われていない文言: {unused:?}"
    );
}

/// 1 文の目安の長さ [01 §10.3、13 §4.5]。
const SENTENCE_MAX: usize = 20;

/// 文ごとの字数。「。」「!」「?」で区切り(区切りの字も数える)、空白・改行と `{…}` は数えない。
fn sentence_lengths(plain: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut n = 0;
    for c in plain.chars().filter(|c| !c.is_whitespace()) {
        n += 1;
        if matches!(c, '。' | '!' | '?' | '\u{FF01}' | '\u{FF1F}') {
            out.push(n);
            n = 0;
        }
    }
    if n > 0 {
        out.push(n);
    }
    out
}

#[test]
fn sentence_lengths_split_at_the_end_marks() {
    assert_eq!(sentence_lengths("駅を選んでみよう"), [8]);
    assert_eq!(sentence_lengths("電車が 走るよ。見て いよう"), [7, 5]);
    assert_eq!(sentence_lengths("1無量大数円!\nきみは電車王だ!"), [7, 8]);
    assert!(sentence_lengths("").is_empty());
}

#[test]
fn sentences_over_20_chars_are_reported() {
    for i in 0..text::TEXT_COUNT {
        let id = TextId(i as u16);
        for n in sentence_lengths(&plain(id)) {
            if n > SENTENCE_MAX {
                eprintln!(
                    "警告: {}: 1 文が {n} 字({SENTENCE_MAX} 字を超える)",
                    text::key(id)
                );
            }
        }
    }
}
