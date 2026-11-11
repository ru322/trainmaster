use super::*;

const F: FontId = FontId::Body12;

/// 合成した送り幅の項目: 全角 10、半角 5、空白 5、`#` はボタンの絵 16、`[…]` はプレースホルダの値。
fn syn(src: &str) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut from = None;
    for c in src.chars() {
        let kind = match c {
            '[' => {
                from = Some(items.len() + 1);
                continue;
            }
            ']' => {
                for it in &mut items[from.take().expect("[ の対応")..] {
                    it.glue = true;
                }
                continue;
            }
            '\n' => Kind::Newline,
            ' ' => Kind::Space,
            '#' => Kind::Button(sprite::BTN_KEY_Z),
            _ => Kind::Glyph(Glyph {
                ch: c,
                advance: if c.is_ascii() { 5 } else { 10 },
                bitmap: None,
            }),
        };
        let mut it = Item::new(kind);
        if matches!(kind, Kind::Space) {
            it.adv = 5;
        }
        items.push(it);
    }
    items
}

fn lines(src: &str, max_w: u16) -> (Vec<String>, bool) {
    let mut items = syn(src);
    link(&mut items);
    let mut starts = Vec::new();
    let wrapped = break_lines(&items, max_w, &mut starts);
    let mut ends = starts[1..].to_vec();
    ends.push(items.len());
    let out = starts
        .iter()
        .zip(ends)
        .map(|(&a, b)| {
            items[a..b]
                .iter()
                .filter_map(|it| match it.kind {
                    Kind::Glyph(g) => Some(g.ch),
                    Kind::Button(_) => Some('#'),
                    Kind::Space => Some(' '),
                    Kind::Newline => None,
                })
                .collect::<String>()
                .trim()
                .to_string()
        })
        .collect();
    (out, wrapped)
}

fn wrap(src: &str, max_w: u16) -> Vec<String> {
    let (out, wrapped) = lines(src, max_w);
    assert!(wrapped, "{src:?} は折り返す");
    out
}

#[test]
fn breaks_between_chars() {
    assert_eq!(wrap("あいうえおかき", 30), ["あいう", "えおか", "き"]);
    assert_eq!(lines("あいう", 30), (vec!["あいう".to_string()], false));
}

#[test]
fn explicit_newline_is_not_a_wrap() {
    assert_eq!(
        lines("あい\nう", 20),
        (vec!["あい".into(), "う".into()], false)
    );
    assert_eq!(wrap("あいう\nえ", 20), ["あい", "う", "え"]);
    assert_eq!(wrap("あ い\nうえおか", 30), ["あ い", "うえお", "か"]);
}

#[test]
fn space_is_preferred() {
    assert_eq!(wrap("あい うえおか", 60), ["あい", "うえおか"]);
    assert_eq!(wrap("あい うえおかきく", 50), ["あい", "うえおかき", "く"]);
    assert_eq!(wrap("あ いう。", 40), ["あ", "いう。"]);
    assert_eq!(wrap("あい   う", 20), ["あい", "う"]);
}

#[test]
fn line_start_kinsoku_hangs() {
    assert_eq!(lines("あいう。", 30), (vec!["あいう。".to_string()], false));
    assert_eq!(wrap("あいうっえ", 30), ["あいうっ", "え"]);
    assert_eq!(wrap("あい[」12]", 30), ["あ", "い」12"]);
    assert_eq!(lines("あいう!」", 30).0, ["あいう!」"]);
}

#[test]
fn line_end_kinsoku_moves_to_next_line() {
    assert_eq!(wrap("あい「う", 30), ["あい", "「う"]);
    assert_eq!(wrap("あい『う", 30), ["あい", "『う"]);
    assert_eq!(wrap("あい(う", 25), ["あい", "(う"]);
}

#[test]
fn alnum_runs_stay_together() {
    assert_eq!(lines("あLv12", 30).0, ["あLv12"]);
    assert_eq!(wrap("あLv12", 25), ["あ", "Lv12"]);
}

#[test]
fn placeholder_values_stay_together() {
    assert_eq!(wrap("あい[1万2345円]", 50), ["あい", "1万2345円"]);
    assert_eq!(wrap("あい1万2345円", 50), ["あい1万", "2345円"]);
}

#[test]
fn button_keeps_the_next_char() {
    assert_eq!(wrap("あい#選ぶ", 40), ["あい", "#選ぶ"]);
    assert_eq!(wrap("あい#選ぶ", 50), ["あい#選", "ぶ"]);
}

#[test]
fn breaks_anyway_when_nothing_else_fits() {
    assert_eq!(wrap("ABCDEFG", 20), ["ABCD", "EFG"]);
    assert_eq!(lines("#", 10), (vec!["#".to_string()], false));
    assert_eq!(lines("", 10), (vec![String::new()], false));
}

#[test]
fn expand_glues_placeholder_values_and_buttons() {
    let mut items = Vec::new();
    expand(
        "あ{n}い{btn:confirm}う",
        F,
        &[TextArg {
            name: "n",
            value: "1万2",
        }],
        InputDevice::Keyboard,
        &mut items,
    );
    link(&mut items);
    let glue: Vec<bool> = items.iter().map(|it| it.glue).collect();
    assert_eq!(glue, [false, false, true, true, false, false, true]);
}

#[test]
fn layout_reports_auto_wraps() {
    let full = u16::from(font::face(F).full_advance);
    let mut b = TextBlock::default();
    layout("あい\nう", F, full * 2, &[], InputDevice::Keyboard, &mut b);
    assert_eq!((b.lines, b.wrapped), (2, false));
    layout("あいう", F, full * 2, &[], InputDevice::Keyboard, &mut b);
    assert_eq!((b.lines, b.wrapped, b.width), (2, true, full * 2));
    layout("あいう", F, full * 3, &[], InputDevice::Keyboard, &mut b);
    assert_eq!((b.lines, b.wrapped), (1, false));
}

#[test]
fn layout_drops_spaces_at_line_edges() {
    let face = font::face(F);
    let full = u16::from(face.full_advance);
    let mut b = TextBlock::default();
    layout("あ  い", F, full, &[], InputDevice::Keyboard, &mut b);
    assert_eq!((b.lines, b.width), (2, full));
    let i = font::glyph(F, 'い').and_then(|g| g.bitmap).expect("い");
    assert_eq!(b.glyphs[1].x, i16::from(i.x_off), "行頭の空白は描かない");
    assert!(b.glyphs[1].y >= i16::from(face.line_height));
    layout("あ\n  い", F, u16::MAX, &[], InputDevice::Keyboard, &mut b);
    assert_eq!(
        b.glyphs[1].x,
        i16::from(i.x_off),
        "\\n の後の空白も描かない"
    );
}

#[test]
fn scale_multiplies_every_dimension() {
    let mut b = TextBlock::default();
    layout(
        "{btn:confirm}A\nあ",
        F,
        u16::MAX,
        &[],
        InputDevice::Keyboard,
        &mut b,
    );
    let one = b.clone();
    b.scale(2);
    assert_eq!(b.width, one.width * 2);
    assert_eq!(b.line_height, one.line_height * 2);
    assert_eq!(b.lines, one.lines);
    assert_eq!(b.height(), one.height() * 2);
    for (g, o) in b.glyphs.iter().zip(&one.glyphs) {
        assert_eq!(
            (g.x, g.y, g.w, g.h, g.sprite, g.plain),
            (o.x * 2, o.y * 2, o.w * 2, o.h * 2, o.sprite, o.plain)
        );
    }
}
