use tm_assets::SpriteId;
use tm_assets::atlas::ALL_RECTS;
use tm_assets::font::{FontId, GLYPH_COUNT, face, fallback_glyph, glyph, measure};
use tm_assets::sprite::SPRITE_COUNT;
use tm_assets::text::{TEXT_COUNT, TextArg, TextId, fill_into, key, placeholders, text};
use tm_assets::txt;

const FONTS: [FontId; 2] = [FontId::Body12, FontId::Head16];
const REGISTERED: [&str; 8] = ["money", "people", "n", "a", "b", "used", "slots", "m"];
const BUTTONS: [&str; 10] = [
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

fn all_ids() -> impl Iterator<Item = TextId> {
    (0..TEXT_COUNT).map(|i| TextId(u16::try_from(i).unwrap()))
}

fn literal_chars(s: &str) -> Vec<char> {
    let mut out = Vec::new();
    let mut inside = false;
    for ch in s.chars() {
        match ch {
            '{' => inside = true,
            '}' => inside = false,
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    out
}

#[test]
fn every_text_char_exists_in_both_fonts() {
    for id in all_ids() {
        for ch in literal_chars(text(id)) {
            if ch == '\n' {
                continue;
            }
            for font in FONTS {
                assert!(glyph(font, ch).is_some(), "{} の {ch:?}({font:?})", key(id));
            }
        }
    }
}

#[test]
fn numeric_output_chars_and_units_exist_in_both_fonts() {
    let units = tm_numeric::JA_UNITS.iter().flat_map(|(a, b)| [*a, *b]);
    for ch in tm_numeric::OUTPUT_CHARS
        .chars()
        .chain(units.flat_map(str::chars))
    {
        for font in FONTS {
            assert!(glyph(font, ch).is_some(), "{ch:?}({font:?})");
        }
    }
}

#[test]
fn placeholders_are_registered() {
    for id in all_ids() {
        for name in placeholders(id) {
            let ok = REGISTERED.contains(name)
                || *name == "unit"
                || *name == "yomi"
                || name
                    .strip_prefix("btn:")
                    .is_some_and(|b| BUTTONS.contains(&b));
            assert!(ok, "{}: {name}", key(id));
        }
    }
}

#[test]
fn keys_are_sorted_and_ids_match() {
    let keys: Vec<&str> = all_ids().map(key).collect();
    assert!(keys.windows(2).all(|w| w[0].as_bytes() < w[1].as_bytes()));
    assert_eq!(key(txt::BUILD_CHOOSE_CREATE), "build.choose.create");
    assert_eq!(txt::BUILD_CHOOSE_CREATE, TextId(0));
}

fn find_with(name: &str) -> TextId {
    all_ids()
        .find(|id| placeholders(*id) == [name])
        .unwrap_or_else(|| panic!("{name} だけを持つ文言がない"))
}

#[test]
fn fill_into_replaces_and_reuses_the_buffer() {
    let id = find_with("n");
    let mut out = String::from("古い内容");
    fill_into(
        id,
        &[TextArg {
            name: "n",
            value: "42",
        }],
        &mut out,
    );
    assert_eq!(out, text(id).replace("{n}", "42"));
    fill_into(
        id,
        &[TextArg {
            name: "n",
            value: "7",
        }],
        &mut out,
    );
    assert_eq!(out, text(id).replace("{n}", "7"));
}

#[test]
fn fill_into_keeps_button_placeholders() {
    let id = find_with("btn:confirm");
    let mut out = String::new();
    fill_into(id, &[], &mut out);
    assert_eq!(out, text(id));
    assert!(out.contains("{btn:confirm}"));
}

#[test]
fn fill_into_without_placeholders_copies_the_text() {
    let id = all_ids().find(|id| placeholders(*id).is_empty()).unwrap();
    let mut out = String::new();
    fill_into(id, &[], &mut out);
    assert_eq!(out, text(id));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "足りない引数")]
fn fill_into_panics_on_missing_arg_in_debug() {
    fill_into(find_with("n"), &[], &mut String::new());
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "余分な引数")]
fn fill_into_panics_on_extra_arg_in_debug() {
    let args = [TextArg {
        name: "zzz",
        value: "1",
    }];
    fill_into(find_with("n"), &args, &mut String::new());
}

#[test]
fn font_metrics_follow_the_design() {
    assert_eq!(glyph(FontId::Body12, 'あ').unwrap().advance, 12);
    assert_eq!(glyph(FontId::Body12, 'A').unwrap().advance, 6);
    assert_eq!(glyph(FontId::Head16, 'あ').unwrap().advance, 16);
    assert_eq!(glyph(FontId::Head16, 'A').unwrap().advance, 8);
    for (font, size, line, ascent) in [(FontId::Body12, 12, 16, 11), (FontId::Head16, 16, 20, 15)] {
        let f = face(font);
        assert_eq!(
            (
                f.size_px,
                f.line_height,
                f.full_advance,
                f.ascent,
                f.descent
            ),
            (size, line, size, ascent, 2)
        );
        assert_eq!(f.id, font);
    }
}

#[test]
fn space_has_no_bitmap_and_other_glyphs_map_to_sprite_ids() {
    let space = glyph(FontId::Body12, ' ').unwrap();
    assert!(space.bitmap.is_none() && space.advance > 0);
    let first = glyph(FontId::Body12, '!').unwrap().bitmap.unwrap();
    assert_eq!(first.sprite, SpriteId(u16::try_from(SPRITE_COUNT).unwrap()));
    let head = glyph(FontId::Head16, '!').unwrap().bitmap.unwrap();
    assert_eq!(head.sprite.0 as usize, SPRITE_COUNT + GLYPH_COUNT / 2);
}

#[test]
fn unknown_chars_fall_back_to_question_mark() {
    assert!(glyph(FontId::Body12, '\u{E000}').is_none());
    assert_eq!(fallback_glyph(FontId::Body12).ch, '?');
    let q = u32::from(fallback_glyph(FontId::Head16).advance);
    assert_eq!(measure(FontId::Head16, "\u{E000}"), q);
}

#[test]
fn measure_sums_advances_and_takes_the_longest_line() {
    let sum = |font, s: &str| {
        s.chars()
            .map(|c| u32::from(glyph(font, c).unwrap().advance))
            .sum::<u32>()
    };
    for font in FONTS {
        let s = "AB あいう 電車 123";
        assert_eq!(measure(font, s), sum(font, s));
        assert_eq!(measure(font, ""), 0);
        assert_eq!(measure(font, "A\nあいう"), sum(font, "あいう"));
    }
    assert_eq!(measure(FontId::Body12, "AB"), 12);
}

#[test]
fn fill_into_matches_arguments_by_name_not_position() {
    let id = find_with_all(&["a", "b"]);
    let mut out = String::new();
    let args = [
        TextArg {
            name: "b",
            value: "BB",
        },
        TextArg {
            name: "a",
            value: "AA",
        },
    ];
    fill_into(id, &args, &mut out);
    assert_eq!(out, text(id).replace("{a}", "AA").replace("{b}", "BB"));
    assert!(out.find("AA") < out.find("BB"));
}

fn find_with_all(names: &[&str]) -> TextId {
    all_ids()
        .find(|id| placeholders(*id) == names)
        .unwrap_or_else(|| panic!("{names:?} を持つ文言がない"))
}

#[test]
fn glyph_bitmap_sizes_match_the_atlas_rects() {
    let mut bitmaps = 0;
    for font in FONTS {
        for ch in all_chars(font) {
            let g = glyph(font, ch).unwrap();
            if let Some(b) = g.bitmap {
                let r = ALL_RECTS[usize::from(b.sprite.0)];
                let k = tm_platform_api::RENDER_SCALE as u16;
                assert_eq!(
                    (u16::from(b.w) * k, u16::from(b.h) * k),
                    (r.w, r.h),
                    "{ch:?}: 字形は論理の 3 倍の画素"
                );
                bitmaps += 1;
            }
        }
    }
    assert_eq!(bitmaps, GLYPH_COUNT);
}

fn all_chars(font: FontId) -> impl Iterator<Item = char> {
    (0x20..=0xFFFFu32)
        .filter_map(char::from_u32)
        .filter(move |c| glyph(font, *c).is_some())
}

#[test]
fn glyph_offsets_match_the_golden_values() {
    let b = glyph(FontId::Body12, '!').unwrap().bitmap.unwrap();
    assert_eq!((b.w, b.h, b.x_off, b.y_off), (2, 11, 2, -10));
    let h = glyph(FontId::Head16, '!').unwrap().bitmap.unwrap();
    assert_eq!((h.w, h.h, h.x_off, h.y_off), (2, 14, 3, -13));
}
