use tm_spritegen::atlas::Strip;
use tm_spritegen::draw::{corner_frame, nine_slice};
use tm_spritegen::palette::Palette;
use tm_spritegen::{Generated, Sources, SpriteError, generate};

const PAL: &str = "Palette(colors: [
    ('.', \"clear\", 0x00000000),
    ('K', \"outline\", 0x000000FF),
    ('W', \"white\", 0xFFFFFFFF),
    ('g', \"rail\", 0x5A5A5AFF),
    ('p', \"panel\", 0x1E2A4AFF),
    ('s', \"shadow\", 0x00000060),
])";

const OUTLINE: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const RAIL: [u8; 4] = [0x5A, 0x5A, 0x5A, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn pal() -> Palette {
    Palette::parse(PAL).unwrap()
}

fn strip(width: u32, height: u32) -> Strip {
    Strip {
        width,
        page: 0,
        index: 1,
        y: 8,
        height,
    }
}

fn from_defs_with(
    palette: &str,
    files: &[(&str, &str)],
    strip: &Strip,
) -> Result<Generated, SpriteError> {
    let src = Sources {
        palette: palette.into(),
        files: files
            .iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect(),
    };
    generate(&src, strip)
}

fn from_defs(body: &str) -> Result<Generated, SpriteError> {
    from_defs_with(
        PAL,
        &[("a.ron", &format!("Sprites(tile: 16, sprites: [{body}])"))],
        &strip(64, 64),
    )
}

fn code(r: Result<Generated, SpriteError>) -> &'static str {
    r.expect_err("エラーになるはず").code().unwrap_or("none")
}

fn sprite<'a>(g: &'a Generated, name: &str) -> &'a tm_spritegen::resolve::Sprite {
    g.sprites
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("{name} がない"))
}

#[test]
fn pixels_are_read_by_symbol() {
    let g = from_defs(r#"(name: "a", source: Pixels(["KW", ".g"]))"#).unwrap();
    let s = sprite(&g, "a");
    assert_eq!((s.image.w, s.image.h), (2, 2));
    assert_eq!(s.image.px, vec![OUTLINE, WHITE, CLEAR, RAIL]);
}

#[test]
fn flip_x_mirrors_pixels_and_pivot() {
    let g = from_defs(
        r#"(name: "a", pivot: (1, 0), source: Pixels(["KWW", "g.."])),
           (name: "b", source: Derive(from: "a", op: FlipX))"#,
    )
    .unwrap();
    let b = sprite(&g, "b");
    assert_eq!(b.image.px, vec![WHITE, WHITE, OUTLINE, CLEAR, CLEAR, RAIL]);
    assert_eq!(b.pivot, (2, 0));
}

#[test]
fn flip_y_rot180_and_recolor() {
    let g = from_defs(
        r#"(name: "a", tint: true, source: Pixels(["KW", "g."])),
           (name: "fy", source: Derive(from: "a", op: FlipY)),
           (name: "rot", source: Derive(from: "a", op: Rot180)),
           (name: "rc", source: Derive(from: "a", op: Recolor([('W', 'p')])))"#,
    )
    .unwrap();
    assert_eq!(sprite(&g, "fy").image.px, vec![RAIL, CLEAR, OUTLINE, WHITE]);
    assert_eq!(
        sprite(&g, "rot").image.px,
        vec![CLEAR, RAIL, WHITE, OUTLINE]
    );
    assert_eq!(sprite(&g, "rc").image.px[1], [0x1E, 0x2A, 0x4A, 255]);
    assert!(sprite(&g, "rc").tint);
}

#[test]
fn nine_slice_has_expected_sizes() {
    let parts = nine_slice(4, 1, [WHITE, RAIL, OUTLINE], "n").unwrap();
    let dims: Vec<(u32, u32)> = parts.iter().map(|i| (i.w, i.h)).collect();
    assert_eq!(
        dims,
        vec![
            (4, 4),
            (1, 4),
            (4, 4),
            (4, 1),
            (1, 1),
            (4, 1),
            (4, 4),
            (1, 4),
            (4, 4)
        ]
    );
    assert_eq!(parts[0].get(0, 0), CLEAR);
    assert_eq!(parts[0].get(1, 0), OUTLINE);
    assert_eq!(parts[0].get(1, 1), RAIL);
    assert_eq!(parts[0].get(2, 2), WHITE);
    assert_eq!(parts[2].get(3, 0), CLEAR);
    assert_eq!(parts[8].get(3, 3), CLEAR);
    assert!(nine_slice(2, 1, [WHITE, RAIL, OUTLINE], "n").is_err());
}

#[test]
fn corner_frame_is_tile_plus_six() {
    let img = corner_frame(16, &pal(), "t").unwrap();
    assert_eq!((img.w, img.h), (22, 22));
    assert_eq!(img.get(0, 11), OUTLINE);
    assert_eq!(img.get(1, 11), WHITE);
    assert_eq!(img.get(2, 11), WHITE);
    assert_eq!(img.get(3, 11), CLEAR);
    assert_eq!(corner_frame(20, &pal(), "t").unwrap().w, 26);
}

#[test]
fn anim_frames_get_consecutive_ids() {
    let g = from_defs(
        r#"(name: "fx", pivot: (1, 1), anim: (frame_ms: 100, looping: false),
             frames: [Pixels(["W."]), Pixels([".W"]), Pixels(["WW"])]),
           (name: "zz", source: Pixels(["W"])),
           (name: "aa", source: Pixels(["W"]))"#,
    );
    let g = g.unwrap();
    let names: Vec<&str> = g.sprites.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["aa", "fx_f0", "fx_f1", "fx_f2", "zz"]);
    assert!(
        g.sprites_rs
            .contains("pub const FX_F0: SpriteId = SpriteId(1);")
    );
    assert!(g.sprites_rs.contains(
        "pub const ANIM_FX: Anim = Anim { first: FX_F0, frames: 3, frame_ms: 100, looping: false };"
    ));
}

#[test]
fn nine_slice_def_emits_parts_and_constant() {
    let g = from_defs(
        r#"(name: "win", source: Proc(NineSlice(corner: 4, fill: "panel", border: "white", outline: "outline")))"#,
    )
    .unwrap();
    let names: Vec<&str> = g.sprites.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "win_b", "win_bl", "win_br", "win_c", "win_l", "win_r", "win_t", "win_tl", "win_tr"
        ]
    );
    assert!(g.sprites_rs.contains(
        "pub const NS_WIN: NineSlice = NineSlice { parts: [WIN_TL, WIN_T, WIN_TR, WIN_L, WIN_C, WIN_R, WIN_BL, WIN_B, WIN_BR], corner: 4 };"
    ));
}

#[test]
fn partial_tables_are_rejected() {
    let r = from_defs(r#"(name: "trn_e", source: Pixels(["W"]))"#);
    assert_eq!(code(r), "SG-E02");
}

#[test]
fn e01_syntax() {
    assert_eq!(
        code(from_defs(r#"(name: "a", source: Pixels(["W"])"#)),
        "SG-E01"
    );
    assert_eq!(code(from_defs(r#"(name: "a")"#)), "SG-E01");
    assert_eq!(
        code(from_defs(
            r#"(name: "a", source: Pixels(["W"]), frames: [Pixels(["W"])])"#
        )),
        "SG-E01"
    );
    assert_eq!(
        code(from_defs(r#"(name: "a", nope: 1, source: Pixels(["W"]))"#)),
        "SG-E01"
    );
    assert_eq!(
        code(from_defs_with("Palette(colors: [", &[], &strip(8, 8))),
        "SG-E01"
    );
}

#[test]
fn e02_names() {
    assert_eq!(
        code(from_defs(r#"(name: "Bad", source: Pixels(["W"]))"#)),
        "SG-E02"
    );
    assert_eq!(
        code(from_defs(r#"(name: "1a", source: Pixels(["W"]))"#)),
        "SG-E02"
    );
    assert_eq!(
        code(from_defs(
            r#"(name: "a", source: Pixels(["W"])), (name: "a", source: Pixels(["K"]))"#
        )),
        "SG-E02"
    );
}

#[test]
fn e03_unknown_symbol() {
    assert_eq!(
        code(from_defs(r#"(name: "a", source: Pixels(["Z"]))"#)),
        "SG-E03"
    );
    assert_eq!(
        code(from_defs(
            r#"(name: "a", source: Proc(Fill(w: 1, h: 1, color: "nope")))"#
        )),
        "SG-E03"
    );
    let dup = PAL.replace("('g'", "('K'");
    assert_eq!(code(from_defs_with(&dup, &[], &strip(8, 8))), "SG-E03");
}

#[test]
fn e04_shape() {
    assert_eq!(
        code(from_defs(r#"(name: "a", source: Pixels(["WW", "W"]))"#)),
        "SG-E04"
    );
    assert_eq!(
        code(from_defs(
            r#"(name: "a", anim: (frame_ms: 1), frames: [Pixels(["W"]), Pixels(["WW"])])"#
        )),
        "SG-E04"
    );
}

#[test]
fn e05_derive() {
    assert_eq!(
        code(from_defs(
            r#"(name: "a", source: Derive(from: "none", op: FlipX))"#
        )),
        "SG-E05"
    );
    assert_eq!(
        code(from_defs(
            r#"(name: "a", source: Derive(from: "b", op: FlipX)), (name: "b", source: Derive(from: "a", op: FlipX))"#
        )),
        "SG-E05"
    );
    assert_eq!(
        code(from_defs(
            r#"(name: "a", source: Derive(from: "a", op: FlipX))"#
        )),
        "SG-E05"
    );
}

#[test]
fn e06_pivot() {
    assert_eq!(
        code(from_defs(
            r#"(name: "a", pivot: (3, 0), source: Pixels(["W"]))"#
        )),
        "SG-E06"
    );
    assert_eq!(
        code(from_defs(
            r#"(name: "a", pivot: (-1, 0), source: Pixels(["W"]))"#
        )),
        "SG-E06"
    );
    assert!(from_defs(r#"(name: "a", pivot: (1, 1), source: Pixels(["W"]))"#).is_ok());
}

#[test]
fn e07_overflow() {
    let body = r#"(name: "a", source: Pixels(["WWWW", "WWWW"])), (name: "b", source: Pixels(["WWWW", "WWWW"]))"#;
    let files = [("a.ron", format!("Sprites(tile: 16, sprites: [{body}])"))];
    let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
    assert_eq!(code(from_defs_with(PAL, &files, &strip(6, 3))), "SG-E07");
    assert!(from_defs_with(PAL, &files, &strip(6, 6)).is_ok());
}

#[test]
fn e08_too_many_colors() {
    let mut s = String::from("Palette(colors: [");
    for i in 0..33u32 {
        s.push_str(&format!(
            "('{}', \"c{i}\", 0x000000FF),",
            char::from_u32(0x3041 + i).unwrap()
        ));
    }
    s.push_str("])");
    assert_eq!(code(from_defs_with(&s, &[], &strip(8, 8))), "SG-E08");
}

#[test]
fn e09_translucent() {
    let bad = PAL.replace("0x00000060", "0x00000080");
    assert_eq!(code(from_defs_with(&bad, &[], &strip(8, 8))), "SG-E09");
    let wrong_name = PAL.replace("\"shadow\"", "\"smoke\"");
    assert_eq!(
        code(from_defs_with(&wrong_name, &[], &strip(8, 8))),
        "SG-E09"
    );
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, &b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    })
}

const GOLDEN_BODY: &str = r#"
    (name: "b", pivot: (1, 1), tint: true, source: Pixels(["KW", "Wg"])),
    (name: "a", source: Pixels(["W"])),
    (name: "r", source: Proc(TrackHalf(dir: E, part: RailFill))),
    (name: "h", source: Proc(TrackHalf(dir: NE, part: BandFill(lane: 0, conv: true))))
"#;

#[test]
fn golden_outputs_and_determinism() {
    let g1 = from_defs(GOLDEN_BODY).unwrap();
    let g2 = from_defs(GOLDEN_BODY).unwrap();
    assert_eq!(g1.png, g2.png);
    assert_eq!(g1.sprites_rs, g2.sprites_rs);
    assert_eq!(g1.palette_rs, g2.palette_rs);
    assert_eq!(g1.sprites_rs, GOLDEN_SPRITES_RS);
    assert!(
        g1.palette_rs
            .contains("pub const PAL_SHADOW: Color = Color { r: 0, g: 0, b: 0, a: 96 };")
    );
    assert_eq!(fnv(&g1.png), GOLDEN_PNG_HASH);
}

const GOLDEN_PNG_HASH: u64 = 5369632085020170234;

const GOLDEN_SPRITES_RS: &str = "\
// @generated by tm-spritegen from assets/sprites — 手で編集しない
pub const SPRITE_STRIP: StripInfo = StripInfo { page: 0, y: 8, height: 64 };
pub const SPRITE_COUNT: usize = 4;
pub static SPRITE_RECTS: [SpriteRect; SPRITE_COUNT] = [
    SpriteRect { atlas: 0, x: 31, y: 8, w: 1, h: 1 },
    SpriteRect { atlas: 0, x: 28, y: 8, w: 2, h: 2 },
    SpriteRect { atlas: 0, x: 0, y: 8, w: 11, h: 15 },
    SpriteRect { atlas: 0, x: 12, y: 8, w: 15, h: 4 },
];
pub static SPRITE_META: [SpriteMeta; SPRITE_COUNT] = [
    SpriteMeta { w: 1, h: 1, pivot_x: 0, pivot_y: 0, tintable: false },
    SpriteMeta { w: 2, h: 2, pivot_x: 1, pivot_y: 1, tintable: true },
    SpriteMeta { w: 11, h: 15, pivot_x: -5, pivot_y: 4, tintable: true },
    SpriteMeta { w: 15, h: 4, pivot_x: -5, pivot_y: -6, tintable: false },
];
pub static SPRITE_NAMES: [&str; SPRITE_COUNT] = [
    \"a\",
    \"b\",
    \"h\",
    \"r\",
];
pub const A: SpriteId = SpriteId(0);
pub const B: SpriteId = SpriteId(1);
pub const H: SpriteId = SpriteId(2);
pub const R: SpriteId = SpriteId(3);
";

#[test]
fn png_roundtrips_and_has_no_extra_chunks() {
    let g = from_defs(GOLDEN_BODY).unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(&g.png));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (64, 64));
    assert_eq!(info.color_type, png::ColorType::Rgba);
    let flat: Vec<u8> = g.image.px.iter().flatten().copied().collect();
    assert_eq!(&buf[..info.buffer_size()], &flat[..]);
    let text = String::from_utf8_lossy(&g.png).into_owned();
    for tag in ["gAMA", "sRGB", "tEXt", "tIME", "pHYs", "iCCP"] {
        assert!(!text.contains(tag), "{tag}");
    }
}

#[test]
fn packing_is_shelf_by_height_then_name() {
    let g = from_defs(
        r#"(name: "c", source: Pixels(["W"])),
           (name: "b", source: Pixels(["WW", "WW"])),
           (name: "a", source: Pixels(["WW", "WW"]))"#,
    )
    .unwrap();
    let pos: Vec<(u32, u32)> = g.positions.clone();
    assert_eq!(pos, vec![(0, 8), (3, 8), (6, 8)]);
}

#[test]
fn sprite_rects_use_absolute_page_coordinates() {
    let g = from_defs(r#"(name: "a", source: Pixels(["W"]))"#).unwrap();
    assert!(
        g.sprites_rs
            .contains("SpriteRect { atlas: 0, x: 0, y: 8, w: 1, h: 1 },")
    );
    assert_eq!(g.image.get(0, 0), WHITE);
}

#[test]
fn derive_pivots_follow_the_flip() {
    let g = from_defs(
        r#"(name: "a", pivot: (1, 0), source: Pixels(["KWW", "g.."])),
           (name: "fx", source: Derive(from: "a", op: FlipX)),
           (name: "fy", source: Derive(from: "a", op: FlipY)),
           (name: "rot", source: Derive(from: "a", op: Rot180)),
           (name: "rc", source: Derive(from: "a", op: Recolor([('W', 'p')]))),
           (name: "set", pivot: (0, 0), source: Derive(from: "a", op: FlipY))"#,
    )
    .unwrap();
    assert_eq!(sprite(&g, "fx").pivot, (2, 0));
    assert_eq!(sprite(&g, "fy").pivot, (1, 2));
    assert_eq!(sprite(&g, "rot").pivot, (2, 2));
    assert_eq!(sprite(&g, "rc").pivot, (1, 0));
    assert_eq!(sprite(&g, "set").pivot, (0, 0));
}

#[test]
fn pack_uses_several_shelves_with_padding() {
    let s = strip(10, 100);
    let items = [
        (4, 3, "a"),
        (4, 3, "b"),
        (4, 2, "c"),
        (4, 2, "d"),
        (2, 1, "e"),
    ];
    let (places, used) = tm_spritegen::pack::pack(&items, &s).unwrap();
    assert_eq!(places, vec![(0, 8), (5, 8), (0, 12), (5, 12), (0, 15)]);
    assert_eq!(used, 8);
}

#[test]
fn pack_orders_by_height_then_name_and_rejects_wide_items() {
    let s = strip(10, 100);
    let (places, _) =
        tm_spritegen::pack::pack(&[(2, 1, "z"), (2, 4, "m"), (2, 4, "k")], &s).unwrap();
    assert_eq!(places, vec![(6, 8), (3, 8), (0, 8)]);
    assert_eq!(
        tm_spritegen::pack::pack(&[(10, 1, "w")], &s)
            .unwrap_err()
            .code(),
        Some("SG-E04")
    );
}

const ATLAS: &str = "width = 1024
[[page]]
strips = [
  { owner = \"font\", height = 768 },
  { owner = \"sprites\", height = 256 },
]
";

#[test]
fn atlas_parse_finds_the_sprite_strip() {
    let s = tm_spritegen::atlas::parse(ATLAS).unwrap();
    assert_eq!(
        s,
        Strip {
            width: 1024,
            page: 0,
            index: 1,
            y: 768,
            height: 256
        }
    );
    let two_pages = "width = 512
[[page]]
strips = [{ owner = \"font\", height = 512 }]
[[page]]
strips = [{ owner = \"font\", height = 100 }, { owner = \"sprites\", height = 50 }, { owner = \"x\", height = 1 }]
";
    let s = tm_spritegen::atlas::parse(two_pages).unwrap();
    assert_eq!(
        (s.page, s.index, s.y, s.height, s.width),
        (1, 1, 100, 50, 512)
    );
}

#[test]
fn atlas_parse_rejects_bad_configs() {
    let parse = tm_spritegen::atlas::parse;
    let sprites = |h: u32| format!("{{ owner = \"sprites\", height = {h} }}");
    let page =
        |strips: &str, width: u32| format!("width = {width}\n[[page]]\nstrips = [{strips}]\n");
    assert!(parse(&page(&format!("{}, {}", sprites(10), sprites(10)), 64)).is_err());
    assert!(parse(&page("{ owner = \"font\", height = 10 }", 64)).is_err());
    assert!(
        parse(&page(
            &format!("{{ owner = \"font\", height = 1000 }}, {}", sprites(25)),
            64
        ))
        .is_err()
    );
    assert!(
        parse(&page(
            &format!("{{ owner = \"font\", height = 1000 }}, {}", sprites(24)),
            64
        ))
        .is_ok()
    );
    assert!(parse(&page(&sprites(10), 0)).is_err());
    assert!(parse(&page(&sprites(10), 1025)).is_err());
    assert!(parse(&page(&sprites(10), 1024)).is_ok());
    assert!(parse("width = ").is_err());
    assert_eq!(parse("width = 64").unwrap_err().code(), None);
}

#[test]
fn generated_rects_and_png_use_the_atlas_strip() {
    let strip = tm_spritegen::atlas::parse(ATLAS).unwrap();
    let g = from_defs_with(
        PAL,
        &[(
            "a.ron",
            "Sprites(tile: 16, sprites: [(name: \"a\", source: Pixels([\"W\"]))])",
        )],
        &strip,
    )
    .unwrap();
    assert!(
        g.sprites_rs
            .contains("StripInfo { page: 0, y: 768, height: 256 }")
    );
    assert!(
        g.sprites_rs
            .contains("SpriteRect { atlas: 0, x: 0, y: 768, w: 1, h: 1 }")
    );
    assert_eq!((g.image.w, g.image.h), (1024, 256));
    assert_eq!(g.image.get(0, 0), WHITE);
    assert_eq!(g.png_name(), "atlas_p0_s1_sprites.png");
}

#[test]
fn non_consecutive_anim_frames_are_rejected() {
    let r = from_defs(
        r#"(name: "fx", anim: (frame_ms: 100), frames: [Pixels(["W"]), Pixels(["W"])]),
           (name: "fx_f0a", source: Pixels(["W"]))"#,
    );
    assert_eq!(code(r), "SG-E04");
}

#[test]
fn anim_needs_two_or_more_frames_and_vice_versa() {
    assert_eq!(
        code(from_defs(
            r#"(name: "a", frames: [Pixels(["W"]), Pixels(["W"])])"#
        )),
        "SG-E01"
    );
    assert_eq!(
        code(from_defs(
            r#"(name: "a", anim: (frame_ms: 1), source: Pixels(["W"]))"#
        )),
        "SG-E01"
    );
}

fn with_tile(tile: u32) -> Result<Generated, SpriteError> {
    from_defs_with(
        PAL,
        &[(
            "a.ron",
            &format!("Sprites(tile: {tile}, sprites: [(name: \"a\", source: Pixels([\"W\"]))])"),
        )],
        &strip(8, 8),
    )
}

#[test]
fn tile_must_be_even_and_at_least_eight() {
    assert_eq!(code(with_tile(15)), "SG-E01");
    assert_eq!(code(with_tile(7)), "SG-E01");
    assert_eq!(code(with_tile(6)), "SG-E01");
    assert!(with_tile(8).is_ok());
    assert!(with_tile(16).is_ok());
    assert!(with_tile(20).is_ok());
}

#[test]
fn nine_slice_border_width_rules() {
    let def = |corner: u32, bw: u32| {
        from_defs(&format!(
            r#"(name: "w", source: Proc(NineSlice(corner: {corner}, border_w: {bw}, fill: "panel", border: "white", outline: "outline")))"#
        ))
    };
    assert_eq!(code(def(2, 1)), "SG-E06");
    assert_eq!(code(def(3, 2)), "SG-E06");
    assert!(def(3, 1).is_ok());
    assert!(def(2, 0).is_ok());
    let g = def(2, 0).unwrap();
    assert_eq!(sprite(&g, "w_tl").image.get(1, 1), [0x1E, 0x2A, 0x4A, 255]);
    assert_eq!(sprite(&g, "w_tl").image.get(1, 0), [0, 0, 0, 255]);
}

#[test]
fn track_sprites_are_cut_with_a_computed_pivot() {
    let g = from_defs(
        r#"(name: "r", source: Proc(TrackHalf(dir: E, part: RailOutline))),
           (name: "f", source: Proc(TrackHalf(dir: NE, part: BandFill(lane: 2, conv: false)))),
           (name: "d", source: Derive(from: "r", op: FlipX))"#,
    )
    .unwrap();
    let r = sprite(&g, "r");
    assert_eq!((r.image.w, r.image.h, r.pivot), (17, 6, (-4, -5)));
    assert!(!sprite(&g, "r").tint);
    assert!(sprite(&g, "f").tint);
    assert_eq!(sprite(&g, "d").pivot, (21, -5));
}

#[test]
fn band_tables_and_variant_enum_are_emitted() {
    let mut body = String::new();
    for d in ["E", "NE", "N", "NW", "W", "SW", "S", "SE"] {
        for part in ["RailOutline", "RailFill"] {
            let n = if part == "RailOutline" { "o" } else { "f" };
            body += &format!(
                r#"(name: "trk_rail_{n}_{}", source: Proc(TrackHalf(dir: {d}, part: {part}))),"#,
                d.to_lowercase()
            );
        }
        for (v, lane, conv) in [
            ("l1", 1, false),
            ("l0p", 0, false),
            ("l2p", 2, false),
            ("l0c", 0, true),
            ("l2c", 2, true),
        ] {
            for (n, kind) in [("o", "BandOutline"), ("f", "BandFill")] {
                body += &format!(
                    r#"(name: "trk_band_{n}_{}_{v}", source: Proc(TrackHalf(dir: {d}, part: {kind}(lane: {lane}, conv: {conv})))),"#,
                    d.to_lowercase()
                );
            }
        }
    }
    let src = format!("Sprites(tile: 16, sprites: [{body}])");
    let g = from_defs_with(PAL, &[("a.ron", &src)], &strip(1024, 256)).unwrap();
    assert_eq!(g.sprites.len(), 96);
    assert!(
        g.sprites_rs
            .contains("pub const TRK_RAIL_O: [SpriteId; 8] = [TRK_RAIL_O_E, TRK_RAIL_O_NE,")
    );
    assert!(g.sprites_rs.contains("pub const TRK_RAIL_F: [SpriteId; 8]"));
    assert!(g.sprites_rs.contains("pub const TRK_BAND_O: [[SpriteId; 5]; 8] = [[TRK_BAND_O_E_L1, TRK_BAND_O_E_L0P, TRK_BAND_O_E_L2P, TRK_BAND_O_E_L0C, TRK_BAND_O_E_L2C], [TRK_BAND_O_NE_L1,"));
    assert!(
        g.sprites_rs
            .contains("pub const TRK_BAND_F: [[SpriteId; 5]; 8]")
    );
    assert!(
        g.sprites_rs
            .contains("pub enum BandVariant { Center, Lane0, Lane2, Lane0Conv, Lane2Conv }")
    );
}

#[test]
fn pack_accepts_an_exact_fit_and_rejects_one_row_more() {
    let s = strip(10, 4);
    let (places, used) = tm_spritegen::pack::pack(&[(4, 2, "a"), (4, 2, "b")], &s).unwrap();
    assert_eq!(places, vec![(0, 8), (5, 8)]);
    assert_eq!(used, 2);
    let (_, used) = tm_spritegen::pack::pack(&[(9, 2, "a"), (9, 1, "b")], &s).unwrap();
    assert_eq!(used, 4);
    let err = tm_spritegen::pack::pack(&[(9, 2, "a"), (9, 2, "b")], &s).unwrap_err();
    assert_eq!(
        err,
        SpriteError::Overflow {
            needed: 5,
            available: 4
        }
    );
}

const DIR8: [&str; 8] = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];

fn table(name: &str, prefix: &str, suffix: &str) -> String {
    let items: Vec<String> = DIR8
        .iter()
        .map(|d| format!("{prefix}{d}{suffix}"))
        .collect();
    format!("pub const {name}: [SpriteId; 8] = [{}];", items.join(", "))
}

fn nested(name: &str, prefix: &str) -> String {
    let rows: Vec<String> = DIR8
        .iter()
        .map(|d| {
            let v: Vec<String> = ["L1", "L0P", "L2P", "L0C", "L2C"]
                .iter()
                .map(|v| format!("{prefix}{d}_{v}"))
                .collect();
            format!("[{}]", v.join(", "))
        })
        .collect();
    format!(
        "pub const {name}: [[SpriteId; 5]; 8] = [{}];",
        rows.join(", ")
    )
}

#[test]
fn track_tables_are_emitted_in_dir8_and_variant_order() {
    let mut body = String::new();
    for d in ["E", "NE", "N", "NW", "W", "SW", "S", "SE"] {
        let l = d.to_lowercase();
        body += &format!(
            r#"(name: "trk_rail_o_{l}", source: Proc(TrackHalf(dir: {d}, part: RailOutline))),"#
        );
        body += &format!(
            r#"(name: "trk_rail_f_{l}", source: Proc(TrackHalf(dir: {d}, part: RailFill))),"#
        );
        body += &format!(r#"(name: "trn_{l}", source: Pixels(["W"])),"#);
        for (v, lane, conv) in [
            ("l1", 1, false),
            ("l0p", 0, false),
            ("l2p", 2, false),
            ("l0c", 0, true),
            ("l2c", 2, true),
        ] {
            body += &format!(
                r#"(name: "trk_band_o_{l}_{v}", source: Proc(TrackHalf(dir: {d}, part: BandOutline(lane: {lane}, conv: {conv})))),"#
            );
            body += &format!(
                r#"(name: "trk_band_f_{l}_{v}", source: Proc(TrackHalf(dir: {d}, part: BandFill(lane: {lane}, conv: {conv})))),"#
            );
        }
    }
    for n in 0..=10 {
        body += &format!(r#"(name: "pax_dots_{n}", source: Pixels(["W"])),"#);
    }
    for n in 1..=6 {
        body += &format!(r#"(name: "ico_line_no_{n}", source: Pixels(["W"])),"#);
    }
    let src = format!("Sprites(tile: 16, sprites: [{body}])");
    let g = from_defs_with(PAL, &[("a.ron", &src)], &strip(1024, 256)).unwrap();
    let lines: Vec<&str> = g.sprites_rs.lines().collect();
    let has = |want: String| assert!(lines.contains(&want.as_str()), "missing: {want}");
    has(table("TRN", "TRN_", ""));
    has(table("TRK_RAIL_O", "TRK_RAIL_O_", ""));
    has(table("TRK_RAIL_F", "TRK_RAIL_F_", ""));
    has(nested("TRK_BAND_O", "TRK_BAND_O_"));
    has(nested("TRK_BAND_F", "TRK_BAND_F_"));
    let dots: Vec<String> = (0..=10).map(|n| format!("PAX_DOTS_{n}")).collect();
    has(format!(
        "pub const PAX_DOTS: [SpriteId; 11] = [{}];",
        dots.join(", ")
    ));
    let nos: Vec<String> = (1..=6).map(|n| format!("ICO_LINE_NO_{n}")).collect();
    has(format!(
        "pub const LINE_NO: [SpriteId; 6] = [{}];",
        nos.join(", ")
    ));
}

#[test]
fn nine_slice_corners_and_edges_follow_the_rings() {
    let parts = nine_slice(4, 1, [WHITE, RAIL, OUTLINE], "n").unwrap();
    let ring = |r: u32| match r {
        0 => OUTLINE,
        1 => RAIL,
        _ => WHITE,
    };
    let corner = |flip_x: bool, flip_y: bool| {
        move |x: u32, y: u32| {
            let (dx, dy) = (
                if flip_x { 3 - x } else { x },
                if flip_y { 3 - y } else { y },
            );
            if dx == 0 && dy == 0 {
                CLEAR
            } else {
                ring(dx.min(dy))
            }
        }
    };
    for (index, f) in [
        (0, corner(false, false)),
        (2, corner(true, false)),
        (6, corner(false, true)),
        (8, corner(true, true)),
    ] {
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(
                    parts[index].get(x, y),
                    f(x as u32, y as u32),
                    "part {index} ({x}, {y})"
                );
            }
        }
    }
    for i in 0..4 {
        assert_eq!(parts[1].get(0, i), ring(i as u32), "t y={i}");
        assert_eq!(parts[7].get(0, i), ring(3 - i as u32), "b y={i}");
        assert_eq!(parts[3].get(i, 0), ring(i as u32), "l x={i}");
        assert_eq!(parts[5].get(i, 0), ring(3 - i as u32), "r x={i}");
    }
    assert_eq!(parts[4].get(0, 0), WHITE);
}

#[test]
fn pivot_range_boundaries() {
    let ok = |p: &str| {
        from_defs(&format!(
            r#"(name: "a", pivot: {p}, source: Pixels(["WW", "WW"]))"#
        ))
        .is_ok()
    };
    assert!(ok("(0, 0)"));
    assert!(ok("(2, 2)"));
    assert!(ok("(2, 0)"));
    assert!(ok("(0, 2)"));
    assert!(!ok("(3, 0)"));
    assert!(!ok("(0, 3)"));
    assert!(!ok("(-1, 0)"));
    assert!(!ok("(0, -1)"));
    let err = from_defs(r#"(name: "a", pivot: (0, -1), source: Pixels(["W"]))"#);
    assert_eq!(code(err), "SG-E06");
}

#[test]
fn frame_height_mismatch_is_e04() {
    let r = from_defs(
        r#"(name: "a", anim: (frame_ms: 1), frames: [Pixels(["W"]), Pixels(["W", "W"])])"#,
    );
    assert_eq!(code(r), "SG-E04");
    let r =
        from_defs(r#"(name: "a", anim: (frame_ms: 1), frames: [Pixels(["WW"]), Pixels(["W"])])"#);
    assert_eq!(code(r), "SG-E04");
}
