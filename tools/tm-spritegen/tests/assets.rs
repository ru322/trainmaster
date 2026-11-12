use std::path::PathBuf;
use tm_spritegen::{Generated, atlas, generate, load_sources};

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn real() -> Generated {
    let dir = assets_dir();
    let strip = atlas::parse(&std::fs::read_to_string(dir.join("atlas.toml")).unwrap()).unwrap();
    generate(&load_sources(&dir.join("sprites")).unwrap(), &strip).unwrap()
}

fn expected_names() -> Vec<String> {
    let dirs = ["e", "ne", "n", "nw", "w", "sw", "s", "se"];
    let mut v: Vec<String> = Vec::new();
    for d in dirs {
        v.push(format!("trk_rail_o_{d}"));
        v.push(format!("trk_rail_f_{d}"));
        v.push(format!("trn_{d}"));
        v.push(format!("trn_glass_{d}"));
        for var in ["l1", "l0p", "l2p", "l0c", "l2c"] {
            v.push(format!("trk_band_o_{d}_{var}"));
            v.push(format!("trk_band_f_{d}_{var}"));
        }
    }
    for n in ["stn_body", "stn_body_off", "stn_pass"] {
        v.push(n.into());
    }
    for n in 1..=6 {
        v.push(format!("ico_line_no_{n}"));
    }
    for n in 0..=10 {
        v.push(format!("pax_dots_{n}"));
    }
    for n in [
        "ui_map_cursor",
        "ui_menu_cursor",
        "ui_popup_tail",
        "ui_vol_seg",
        "px_white",
        "title_logo",
    ] {
        v.push(n.into());
    }
    for c in ["tl", "tr", "bl", "br"] {
        v.push(format!("ui_hole_{c}"));
    }
    for base in ["ui_win", "ui_popup", "ui_frame", "ui_sel"] {
        for s in ["tl", "t", "tr", "l", "c", "r", "bl", "b", "br"] {
            v.push(format!("{base}_{s}"));
        }
    }
    for k in [
        "z", "x", "c", "q", "e", "b", "tab", "r", "esc", "arrows", "lr",
    ] {
        v.push(format!("btn_key_{k}"));
    }
    for k in [
        "a", "b", "x", "y", "lb", "rb", "r3", "back", "start", "dpad", "lr",
    ] {
        v.push(format!("btn_pad_{k}"));
    }
    for n in [
        "ico_station",
        "ico_train",
        "ico_line",
        "ico_coin",
        "ico_coin_x",
        "ico_star",
        "ico_star_off",
        "ico_new",
        "ico_max",
        "ico_lock",
        "ico_up",
        "ico_crown",
        "ico_people",
        "ico_check",
        "ico_cross",
        "ico_music",
        "ico_sound",
        "ico_tri_up",
        "ico_tri_down",
        "ico_tri_left",
        "ico_tri_right",
        "fx_arrow_down",
        "fx_arrow_up",
        "fx_dot",
        "ico_guide",
        "ico_save",
    ] {
        v.push(n.into());
    }
    for f in 0..4 {
        v.push(format!("fx_sparkle_f{f}"));
    }
    v.sort();
    v
}

#[test]
fn all_sprites_of_15_5_2_are_generated() {
    let g = real();
    let names: Vec<String> = g.sprites.iter().map(|s| s.name.clone()).collect();
    assert_eq!(names, expected_names());
    assert_eq!(names.len(), 230);
}

#[test]
fn sizes_follow_the_list() {
    let g = real();
    let size = |n: &str| g.sprites.iter().find(|s| s.name == n).unwrap().size;
    for s in &g.sprites {
        let r = 3 * s.scale;
        assert_eq!(
            (s.image.w, s.image.h),
            (s.size.0 * r, s.size.1 * r),
            "{}",
            s.name
        );
    }
    for s in g.sprites.iter().filter(|s| s.name.starts_with("trk_")) {
        assert!(s.size.0 <= 14 && s.size.1 <= 14, "{}", s.name);
        assert_eq!(s.scale, 2, "{}", s.name);
    }
    assert_eq!(size("trk_rail_o_e"), (14, 6));
    assert_eq!(size("trk_rail_f_ne"), (12, 12));
    assert_eq!(size("trk_rail_o_se"), (14, 14));
    for (n, d) in [
        ("stn_body", (14, 14)),
        ("stn_pass", (7, 7)),
        ("trn_se", (14, 14)),
        ("ico_line_no_6", (8, 9)),
        ("pax_dots_10", (5, 14)),
        ("ui_map_cursor", (22, 22)),
        ("ui_menu_cursor", (8, 9)),
        ("ui_popup_tail", (10, 6)),
        ("ui_vol_seg", (6, 10)),
        ("btn_key_tab", (16, 16)),
        ("btn_pad_start", (16, 16)),
        ("ico_station", (12, 12)),
        ("ico_save", (12, 12)),
        ("ico_new", (18, 8)),
        ("ico_lock", (10, 12)),
        ("ico_up", (10, 10)),
        ("ico_crown", (16, 16)),
        ("ico_people", (8, 10)),
        ("ico_tri_up", (7, 4)),
        ("ico_tri_left", (8, 8)),
        ("fx_sparkle_f3", (8, 8)),
        ("fx_arrow_down", (11, 8)),
        ("title_logo", (240, 32)),
        ("px_white", (2, 2)),
    ] {
        assert_eq!(size(n), d, "{n}");
    }
}

/// 半透明の塗りは影だけ: 中間のアルファの画素は、透明の画素の近く(9×9 の中。丸い端どうしの細い切れ込みを含む)にある縁のアンチエイリアスか、影の縁だけ。
#[test]
fn only_shadow_is_translucent_and_logo_fits() {
    let g = real();
    for s in &g.sprites {
        let img = &s.image;
        for y in 0..img.h as i32 {
            for x in 0..img.w as i32 {
                let a = img.get(x, y)[3];
                if a == 0 || a == 255 || a == 96 {
                    continue;
                }
                let edge = (-4..=4)
                    .any(|dy| (-4..=4).any(|dx| matches!(img.get(x + dx, y + dy)[3], 0 | 96)));
                assert!(edge, "{} ({x}, {y}) α={a} は縁でない", s.name);
            }
        }
    }
    assert!(g.rows_used <= g.strip.height);
}

#[test]
fn real_assets_build_is_deterministic() {
    let (a, b) = (real(), real());
    assert_eq!(a.png, b.png);
    assert_eq!(a.sprites_rs, b.sprites_rs);
    assert_eq!(a.palette_rs, b.palette_rs);
}

#[test]
fn sprite_tables_are_emitted() {
    let g = real();
    for t in [
        "pub const TRN: [SpriteId; 8]",
        "pub const TRN_GLASS: [SpriteId; 8]",
        "pub const TRK_RAIL_O: [SpriteId; 8]",
        "pub const TRK_RAIL_F: [SpriteId; 8]",
        "pub const TRK_BAND_O: [[SpriteId; 5]; 8]",
        "pub const TRK_BAND_F: [[SpriteId; 5]; 8]",
        "pub enum BandVariant",
        "pub const PAX_DOTS: [SpriteId; 11]",
        "pub const LINE_NO: [SpriteId; 6]",
        "pub const ANIM_FX_SPARKLE: Anim",
        "pub const NS_UI_WIN: NineSlice",
        "pub const NS_UI_POPUP: NineSlice",
        "pub const NS_UI_FRAME: NineSlice",
        "pub const NS_UI_SEL: NineSlice",
        "pub const TRACK_TILE: u16 = 16;",
        "pub const TRACK_PENS: [u16; 4] = [6, 4, 4, 2];",
        "pub const TRACK_LANES: [u16; 2] = [3, 2];",
    ] {
        assert!(g.sprites_rs.contains(t), "{t}");
    }
}

#[test]
fn counts_match_the_design() {
    let g = real();
    let pixels: u32 = g.sprites.iter().map(|s| s.size.0 * s.size.1).sum();
    assert_eq!(pixels, 30_871, "設計の画素");
    let track: u32 = g
        .sprites
        .iter()
        .filter(|s| s.name.starts_with("trk_"))
        .map(|s| s.size.0 * s.size.1)
        .sum();
    assert_eq!(track, 8_592);
    let texels: u32 = g
        .sprites
        .iter()
        .map(|s| s.image.w * s.image.h + s.alt.as_ref().map_or(0, |a| a.w * a.h))
        .sum();
    assert_eq!(texels, TEXELS, "アトラスの画素");
    assert_eq!(g.sprites.iter().filter(|s| s.alt.is_some()).count(), 24);
    assert_eq!(g.rows_used, ROWS);
}

#[test]
fn only_track_sprites_have_a_nonzero_negative_pivot_and_icons_use_zero() {
    let g = real();
    for s in &g.sprites {
        if !s.name.starts_with("trk_") {
            assert!(s.pivot.0 >= 0 && s.pivot.1 >= 0, "{}", s.name);
        }
    }
    let pivot = |n: &str| g.sprites.iter().find(|s| s.name == n).unwrap().pivot;
    assert_eq!(pivot("ico_tri_down"), (0, 0));
    assert_eq!(pivot("ico_tri_right"), (0, 0));
    assert_eq!(pivot("ico_tri_up"), (0, 0));
    assert_eq!(pivot("ico_tri_left"), (0, 0));
}

fn luminance(c: [u8; 4]) -> f64 {
    let f = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])
}

fn contrast(a: [u8; 4], b: [u8; 4]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn text_and_shapes_in_the_art_have_enough_contrast() {
    let text = std::fs::read_to_string(assets_dir().join("sprites/palette.ron")).unwrap();
    let pal = tm_spritegen::palette::Palette::parse(&text).unwrap();
    let c = |n: &str| pal.by_name(n, "t").unwrap();
    for (fg, bg) in [
        ("white", "green"),
        ("white", "red"),
        ("white", "blue"),
        ("white", "mid"),
        ("outline", "highlight"),
        ("outline", "light"),
        ("outline", "white"),
    ] {
        assert!(contrast(c(fg), c(bg)) >= 4.5, "{fg} on {bg}");
    }
    let ground = [0xE8, 0xE2, 0xCC, 255];
    assert!(contrast(c("outline"), ground) >= 3.0);
    assert!(contrast(c("red"), ground) >= 3.0);
}

#[test]
fn stn_pass_has_a_black_outline() {
    let g = real();
    let s = g.sprites.iter().find(|s| s.name == "stn_pass").unwrap();
    assert_eq!(s.size, (7, 7));
    assert_eq!((s.image.w, s.image.h), (42, 42));
    let black = s.image.px.iter().filter(|p| **p == [0, 0, 0, 255]).count();
    let red = s
        .image
        .px
        .iter()
        .filter(|p| **p == [0xC0, 0x39, 0x2B, 255])
        .count();
    assert!(black > red && red > 0);
}

const TEXELS: u32 = 682_056;
const ROWS: u32 = 428;

/// 説明の穴の四隅の覆い(`ui_hole_*`)は、`ui_sel` の角の外側とちょうど補い合う(アルファの和が 255)。
#[test]
fn hole_masks_complement_the_rounded_selection_corners() {
    let g = real();
    let find = |n: &str| g.sprites.iter().find(|s| s.name == n).unwrap();
    for c in ["tl", "tr", "bl", "br"] {
        let (mask, sel) = (find(&format!("ui_hole_{c}")), find(&format!("ui_sel_{c}")));
        assert_eq!((mask.image.w, mask.image.h), (sel.image.w, sel.image.h));
        for (m, s) in mask.image.px.iter().zip(&sel.image.px) {
            let sum = u32::from(m[3]) + u32::from(s[3]);
            assert!(sum <= 256, "{c}: 重なる {m:?} {s:?}");
            if m[3] != 0 {
                assert!(sum >= 254, "{c}: 角の外の縁で隙間 {m:?} {s:?}");
            }
        }
        assert!(
            mask.image.px.iter().any(|p| p[3] == 255) && mask.image.px.iter().any(|p| p[3] == 0)
        );
    }
}
