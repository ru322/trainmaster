use tm_assets::atlas::{ALL_RECTS, ATLASES, PAGE_COUNT, PAGE_DIMS, SPRITE_TOTAL, STRIP_DIMS};
use tm_assets::font::{FONT_STRIP, GLYPH_COUNT, GLYPH_RECTS};
use tm_assets::sprite::{PX_WHITE, SPRITE_COUNT, SPRITE_RECTS, SPRITE_STRIP};
use tm_assets::{bundle, palette, sound};

fn strip(owner: &str) -> tm_assets::atlas::StripInfo {
    STRIP_DIMS
        .iter()
        .find(|(o, _)| *o == owner)
        .unwrap_or_else(|| panic!("帯 {owner} がない"))
        .1
}

#[test]
fn bundle_is_ok_and_validates() {
    let b = bundle().expect("bundle()");
    b.validate().expect("validate");
    assert!(std::ptr::eq(b, bundle().unwrap()));
    assert_eq!(b.white, PX_WHITE);
    assert_eq!(b.sounds.len(), sound::SOUND_COUNT);
}

#[test]
fn rect_table_is_sprites_then_glyphs() {
    assert_eq!(SPRITE_TOTAL, SPRITE_COUNT + GLYPH_COUNT);
    assert_eq!(ALL_RECTS.len(), SPRITE_TOTAL);
    assert_eq!(ALL_RECTS[..SPRITE_COUNT], SPRITE_RECTS);
    assert_eq!(ALL_RECTS[SPRITE_COUNT..], GLYPH_RECTS);
    assert_eq!(bundle().unwrap().sprites.len(), SPRITE_TOTAL);
}

#[test]
fn strip_positions_match_the_tools_output() {
    assert_eq!(strip("font"), FONT_STRIP);
    assert_eq!(strip("sprites"), SPRITE_STRIP);
    assert_eq!(PAGE_COUNT, 1);
    let total: u32 = STRIP_DIMS.iter().map(|(_, s)| s.height).sum();
    assert_eq!(total, PAGE_DIMS[0].1);
    assert_eq!(PAGE_DIMS[0], (2048, 1536));
    assert_eq!(ATLASES[0].rgba.len(), 2048 * 1536 * 4);
}

fn within(rect: &tm_platform_api::SpriteRect, s: tm_assets::atlas::StripInfo) -> bool {
    s.page == rect.atlas
        && u32::from(rect.y) >= s.y
        && u32::from(rect.y) + u32::from(rect.h) <= s.y + s.height
        && u32::from(rect.x) + u32::from(rect.w) <= PAGE_DIMS[0].0
}

#[test]
fn rects_stay_inside_their_own_strip() {
    for (i, r) in GLYPH_RECTS.iter().enumerate() {
        assert!(within(r, FONT_STRIP), "グリフ {i}: {r:?}");
    }
    for (i, r) in SPRITE_RECTS.iter().enumerate() {
        assert!(within(r, SPRITE_STRIP), "スプライト {i}: {r:?}");
    }
}

fn pixel(x: usize, y: usize) -> [u8; 4] {
    let at = (y * PAGE_DIMS[0].0 as usize + x) * 4;
    ATLASES[0].rgba[at..at + 4].try_into().unwrap()
}

#[test]
fn strips_are_concatenated_in_order() {
    let white = ALL_RECTS[usize::from(PX_WHITE.0)];
    for dy in 0..usize::from(white.h) {
        for dx in 0..usize::from(white.w) {
            let p = pixel(usize::from(white.x) + dx, usize::from(white.y) + dy);
            assert_eq!(p, [255, 255, 255, 255]);
        }
    }
    let font_ink = GLYPH_RECTS.iter().all(|r| {
        (0..usize::from(r.h)).any(|dy| {
            (0..usize::from(r.w))
                .any(|dx| pixel(usize::from(r.x) + dx, usize::from(r.y) + dy)[3] != 0)
        })
    });
    assert!(font_ink, "画素のないグリフがある");
}

/// スプライトの帯の画素は、パレットの色か、パレットの色の画素の近く(9×9 の中。丸い端どうしの細い切れ込みを含む)にある縁のアンチエイリアスの中間の色だけ。
#[test]
fn sprite_strip_pixels_are_palette_colors_or_their_antialiased_edges() {
    let pal = [
        palette::PAL_CLEAR,
        palette::PAL_OUTLINE,
        palette::PAL_WHITE,
        palette::PAL_RAIL,
        palette::PAL_LIGHT,
        palette::PAL_MID,
        palette::PAL_PANEL,
        palette::PAL_FRAME,
        palette::PAL_HIGHLIGHT,
        palette::PAL_COIN,
        palette::PAL_COIN_DARK,
        palette::PAL_RED,
        palette::PAL_BLUE,
        palette::PAL_GREEN,
        palette::PAL_SHADOW,
        palette::PAL_ACCENT,
        palette::PAL_GLASS,
        palette::PAL_DARK,
    ]
    .map(|c| [c.r, c.g, c.b, c.a]);
    let (y0, h) = (SPRITE_STRIP.y as usize, SPRITE_STRIP.height as usize);
    let w = PAGE_DIMS[0].0 as usize;
    let (mut solid, mut edge) = (0usize, 0usize);
    for y in y0..y0 + h {
        for x in 0..w {
            if pal.contains(&pixel(x, y)) {
                solid += 1;
                continue;
            }
            edge += 1;
            let near = (y.saturating_sub(4)..(y + 5).min(y0 + h))
                .flat_map(|ny| (x.saturating_sub(4)..(x + 5).min(w)).map(move |nx| (nx, ny)))
                .any(|(nx, ny)| pal.contains(&pixel(nx, ny)) && pixel(nx, ny)[3] != 0);
            assert!(
                near,
                "({x}, {y}): {:?} は塗りの色から離れた中間の色",
                pixel(x, y)
            );
        }
    }
    assert!(
        edge > 0 && edge * 4 < solid,
        "中間の色 {edge}、パレットの色 {solid}"
    );
    assert_eq!(pixel(w - 1, y0 + h - 1), [0, 0, 0, 0]);
}

#[test]
fn sound_data_matches_the_embedded_entries() {
    let sounds = bundle().unwrap().sounds;
    assert_eq!(sounds.len(), sound::SOUND_ENTRIES.len());
    for (data, entry) in sounds.iter().zip(&sound::SOUND_ENTRIES) {
        assert_eq!(data.name, entry.name);
        assert_eq!(data.kind, entry.kind);
        assert_eq!(data.loop_start, entry.loop_start);
        assert_eq!(data.samples, sound::pcm16(entry).unwrap());
    }
    assert!(sounds.iter().any(|s| s.loop_start.is_some()));
}
