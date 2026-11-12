use std::collections::HashSet;

use tm_assets::SpriteId;
use tm_assets::atlas::ALL_RECTS;
use tm_assets::sprite::{
    ANIM_FX_SPARKLE, Anim, LINE_NO, NS_UI_FRAME, NS_UI_POPUP, NS_UI_SEL, NS_UI_WIN, NineSlice,
    PAX_DOTS, SPRITE_COUNT, SPRITE_META, SPRITE_NAMES, TRK_BAND_F, TRK_BAND_O, TRK_RAIL_F,
    TRK_RAIL_O, TRN, TRN_GLASS, meta, name,
};

fn flat_band(t: &[[SpriteId; 5]; 8]) -> Vec<SpriteId> {
    t.iter().flatten().copied().collect()
}

#[test]
fn id_tables_are_distinct_and_have_the_designed_size() {
    let tables: [Vec<SpriteId>; 8] = [
        TRN.to_vec(),
        TRN_GLASS.to_vec(),
        TRK_RAIL_O.to_vec(),
        TRK_RAIL_F.to_vec(),
        flat_band(&TRK_BAND_O),
        flat_band(&TRK_BAND_F),
        PAX_DOTS.to_vec(),
        LINE_NO.to_vec(),
    ];
    let sizes: Vec<usize> = tables.iter().map(Vec::len).collect();
    assert_eq!(sizes, [8, 8, 8, 8, 40, 40, 11, 6]);
    let all: Vec<SpriteId> = tables.concat();
    let unique: HashSet<SpriteId> = all.iter().copied().collect();
    assert_eq!(unique.len(), all.len());
    assert!(all.iter().all(|id| usize::from(id.0) < SPRITE_COUNT));
}

#[test]
fn names_are_sorted_and_meta_matches_rects() {
    assert!(SPRITE_NAMES.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(SPRITE_META.len(), SPRITE_COUNT);
    for (i, r) in ALL_RECTS.iter().take(SPRITE_COUNT).enumerate() {
        let id = SpriteId(u16::try_from(i).unwrap());
        let m = meta(id);
        let k = tm_platform_api::RENDER_SCALE as u16;
        let scale = r.w / (m.w * k);
        assert!(
            (1..=2).contains(&scale) && (r.w, r.h) == (m.w * k * scale, m.h * k * scale),
            "{}: 絵は設計の大きさの 3 倍か 6 倍 {m:?} {r:?}",
            name(id)
        );
        if name(id).starts_with("trk_")
            || name(id).starts_with("trn_")
            || name(id).starts_with("stn_")
        {
            assert_eq!(scale, 2, "マップの絵は 6 倍: {}", name(id));
        }
        if !name(id).starts_with("trk_") {
            assert!(
                (0..=i16::try_from(m.w).unwrap()).contains(&m.pivot_x)
                    && (0..=i16::try_from(m.h).unwrap()).contains(&m.pivot_y),
                "{}: {m:?}",
                name(id)
            );
        }
    }
}

#[test]
fn track_sprites_fit_14px_and_stay_within_4px_of_the_tile() {
    let ids: Vec<SpriteId> = [TRK_RAIL_O.to_vec(), TRK_RAIL_F.to_vec()]
        .concat()
        .into_iter()
        .chain(flat_band(&TRK_BAND_O))
        .chain(flat_band(&TRK_BAND_F))
        .collect();
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for &id in &ids {
        let m = meta(id);
        let left = -i32::from(m.pivot_x);
        lo = lo.min(left).min(-i32::from(m.pivot_y));
        hi = hi
            .max(left + i32::from(m.w))
            .max(-i32::from(m.pivot_y) + i32::from(m.h));
    }
    assert_eq!(
        (lo, hi),
        (-4, 20),
        "外側の帯のレーンがマスから 4px はみ出す"
    );
    for id in ids {
        let m = meta(id);
        assert!(m.w <= 14 && m.h <= 14, "{}: {m:?}", name(id));
        let (left, top) = (-i32::from(m.pivot_x), -i32::from(m.pivot_y));
        let (right, bottom) = (left + i32::from(m.w), top + i32::from(m.h));
        assert!(
            left < 16 && top < 16 && right > 0 && bottom > 0,
            "マスと重ならない: {}",
            name(id)
        );
        assert!(
            left >= -4 && top >= -4 && right <= 20 && bottom <= 20,
            "マスから遠すぎる: {}",
            name(id)
        );
    }
}

#[test]
fn tint_flags_follow_the_naming() {
    for id in TRK_BAND_F.iter().flatten() {
        assert!(meta(*id).tintable, "{}", name(*id));
    }
    for id in TRK_RAIL_F.iter().chain(&TRK_RAIL_O) {
        assert!(!meta(*id).tintable, "{}", name(*id));
    }
    for (body, glass) in TRN.iter().zip(&TRN_GLASS) {
        assert!(meta(*body).tintable, "{}", name(*body));
        assert!(
            !meta(*glass).tintable,
            "窓と前照灯は路線の色を掛けない: {}",
            name(*glass)
        );
        assert_eq!(
            meta(*body),
            &tm_assets::sprite::SpriteMeta {
                tintable: true,
                ..*meta(*glass)
            }
        );
    }
}

#[test]
fn anim_picks_frames_by_elapsed_time() {
    let a = ANIM_FX_SPARKLE;
    let at = |ms| a.frame_at(ms).0 - a.first.0;
    assert_eq!([at(0), at(99), at(100), at(399)], [0, 0, 1, 3]);
    assert_eq!(at(10_000), u16::from(a.frames) - 1);
    let looping = Anim { looping: true, ..a };
    assert_eq!(looping.frame_at(400), a.first);
    assert_eq!(looping.frame_at(550).0 - a.first.0, 1);
    for i in 0..a.frames {
        assert!(name(SpriteId(a.first.0 + u16::from(i))).contains("_f"));
    }
}

#[test]
fn nine_slice_corners_have_the_declared_size() {
    for ns in [NS_UI_FRAME, NS_UI_POPUP, NS_UI_SEL, NS_UI_WIN] {
        let NineSlice { parts, corner } = ns;
        let unique: HashSet<_> = parts.iter().collect();
        assert_eq!(unique.len(), 9);
        for i in [0, 2, 6, 8] {
            let m = meta(parts[i]);
            assert_eq!((m.w, m.h), (u16::from(corner), u16::from(corner)));
        }
        assert_eq!(meta(parts[1]).h, u16::from(corner));
        assert_eq!(meta(parts[3]).w, u16::from(corner));
    }
}

/// 大きい版は、元の 2 倍の画素で、元と同じ設計の大きさのスプライトにだけある。
#[test]
fn alt_levels_are_twice_the_texels_of_their_sprite() {
    let b = tm_assets::bundle().unwrap();
    assert!(!b.alt.is_empty());
    for a in b.alt {
        let base = b.sprites[usize::from(a.id.0)];
        assert_eq!(
            (a.rect.w, a.rect.h),
            (base.w * 2, base.h * 2),
            "{}",
            name(a.id)
        );
        assert!(usize::from(a.id.0) < tm_assets::atlas::SPRITE_TOTAL);
    }
    for n in [
        "ico_line_no_1",
        "fx_arrow_down",
        "fx_arrow_up",
        "ico_up",
        "ui_sel_tl",
        "ui_sel_c",
    ] {
        assert!(b.alt.iter().any(|a| name(a.id) == n), "{n} の大きい版");
    }
}
