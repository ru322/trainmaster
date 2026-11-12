//! 静的な層の中身(13 §3.11、15 §5.8 の 4 回の描画順)。

use tm_assets::sprite;
use tm_platform_api::{DrawCmd, SpriteId};

use super::{cheap_with_company, create_line};
use crate::testing::UiHarness;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    Clear,
    Ground,
    RailOutline,
    RailFill,
    BandOutline,
    BandFill,
    Station,
    PassMark,
    Badge,
}

fn pass_of(cmd: &DrawCmd) -> Step {
    let name = |id: SpriteId| sprite::name(id);
    match cmd {
        DrawCmd::Clear(_) => Step::Clear,
        DrawCmd::FillRect { .. } => Step::Ground,
        DrawCmd::Sprite { id, .. } => {
            let n = name(*id);
            if n.starts_with("trk_rail_o") {
                Step::RailOutline
            } else if n.starts_with("trk_rail_f") {
                Step::RailFill
            } else if n.starts_with("trk_band_o") {
                Step::BandOutline
            } else if n.starts_with("trk_band_f") {
                Step::BandFill
            } else if n.starts_with("stn_body") {
                Step::Station
            } else if n == "stn_pass" {
                Step::PassMark
            } else if n.starts_with("ico_line_no") {
                Step::Badge
            } else {
                panic!("層に {n}")
            }
        }
        other => panic!("層に {other:?}"),
    }
}

pub(super) fn shared_lines() -> UiHarness {
    let mut h = cheap_with_company(1);
    let g = h.game.as_mut().unwrap();
    let r2 = tm_core::Command::LevelUp {
        param: tm_core::ParamKey::R2LineSlots,
        scope: tm_core::UpgradeScope::Company,
        mode: tm_core::BuyMode::Max,
    };
    g.validate(&r2).expect("R2 を買える");
    g.enqueue(r2);
    h.frames(1);
    for l in [&[0u32, 1][..], &[1, 0, 2]] {
        let cmd = create_line(l);
        h.game
            .as_mut()
            .unwrap()
            .validate(&cmd)
            .expect("路線を作れる");
        h.game.as_mut().unwrap().enqueue(cmd);
        h.frames(1);
    }
    h.frames(2);
    for _ in 0..1200 {
        if h.ui.play().unwrap().map.as_ref().unwrap().hidden.is_empty() {
            break;
        }
        h.frames(1);
    }
    h
}

/// 曲がり角の形を集めた場面の駅(添字 = `StationId`)。最初の 11 駅が左上の 20 × 11 マスに入る。
pub(super) fn shapes_plan() -> Vec<tm_core::GridPos> {
    let g = |x, y| tm_core::GridPos { x, y };
    let mut v = vec![
        g(1, 2),
        g(6, 4),
        g(6, 8),
        g(17, 2),
        g(14, 1),
        g(8, 7),
        g(10, 9),
        g(12, 5),
        g(15, 6),
        g(19, 8),
        g(1, 9),
    ];
    let n = tm_core::BalanceTable::builtin().max_stations() as i32;
    v.extend((0..n - v.len() as i32).map(|j| g(22 + 3 * (j % 6), 2 + 3 * (j / 6))));
    v
}

/// 曲がり角の形(45° の折れ、直線からの分かれ、交差、S 字、駅での直角と 135°、2 路線の共有)を集めた場面。
pub(super) fn track_shapes() -> UiHarness {
    use tm_core::{Bend::*, Command, StationId};
    let mut h = UiHarness::from_game(
        super::cheap_game_on(shapes_plan(), |s| s.company.gauge_num = 1),
        false,
    );
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    for _ in 0..2000 {
        if h.game.as_ref().unwrap().view().stations().len() >= 11 {
            break;
        }
        super::buy_all(&mut h);
        h.frames(2);
    }
    let lines: [(&[u32], &[tm_core::Bend]); 5] = [
        (&[0, 3], &[StraightFirst]),
        (&[4, 5, 6], &[DiagonalFirst, StraightFirst]),
        (&[7, 8, 9], &[StraightFirst, DiagonalFirst]),
        (&[4, 7], &[StraightFirst]),
        (&[10, 2, 6], &[StraightFirst, StraightFirst]),
    ];
    for (st, bends) in lines {
        let cmd = Command::CreateLine {
            stations: st.iter().map(|&s| StationId(s)).collect(),
            bends: bends.to_vec(),
        };
        let g = h.game.as_mut().unwrap();
        g.validate(&cmd).expect("路線を作れる");
        g.enqueue(cmd);
        h.frames(1);
    }
    for _ in 0..12_000 {
        if h.ui.play().unwrap().map.as_ref().unwrap().hidden.is_empty() {
            break;
        }
        h.frames(1);
    }
    assert!(h.ui.play().unwrap().map.as_ref().unwrap().hidden.is_empty());
    h
}

/// 静的な層の 1 枚目を左上 (0, 0) に置いて描いた画素(描画の解像度。上の 360 論理行)。`keep` を満たすコマンドだけを描く。
fn layer_pixels(h: &UiHarness, keep: impl Fn(&DrawCmd) -> bool) -> Vec<[u8; 3]> {
    let chunk = &h.ui.play().unwrap().map.as_ref().unwrap().chunks[0];
    let mut cmds = vec![DrawCmd::BeginCachedLayer {
        layer: tm_platform_api::LayerId(0),
        revision: 1,
        x: 0,
        y: 0,
        w: crate::layout::LAYER_W as u32,
        h: crate::layout::WORLD_H as u32,
    }];
    cmds.extend(chunk.iter().filter(|c| keep(c)).copied());
    cmds.push(DrawCmd::EndCachedLayer);
    cmds.push(DrawCmd::DrawCachedLayer {
        layer: tm_platform_api::LayerId(0),
    });
    super::raster::rasterize(&tm_platform_api::DrawList { cmds })
}

fn is_track(c: &DrawCmd) -> bool {
    !matches!(c, DrawCmd::Sprite { id, .. } if !sprite::name(*id).starts_with("trk_"))
}

#[test]
#[ignore = "目で確かめるための画像を書く。TM_UI_DUMP_DIR がなければ何もしない"]
fn dump_track_shapes() {
    let Ok(dir) = std::env::var("TM_UI_DUMP_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let h = track_shapes();
    for (name, px) in [
        ("track_shapes_layer", layer_pixels(&h, |_| true)),
        ("track_shapes_bare", layer_pixels(&h, is_track)),
    ] {
        super::raster::write_ppm(&dir.join(format!("{name}.ppm")), &px);
    }
    super::raster::write_ppm(
        &dir.join("track_shapes.ppm"),
        &super::raster::rasterize(h.draw_list()),
    );
}

/// 画素 (x, y) の中心が、線分を四角いペン(半分の幅 `h`)でなぞった形に入るか。
fn in_pen(s: crate::draw::map::HalfSeg, h: i32, x: i32, y: i32) -> bool {
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for (q, a, b) in [
        (2 * x + 1, 2 * s.from.0, 2 * s.to.0),
        (2 * y + 1, 2 * s.from.1, 2 * s.to.1),
    ] {
        let (q, a, u, h) = (
            f64::from(q),
            f64::from(a),
            f64::from(b - a),
            f64::from(2 * h),
        );
        if u == 0.0 {
            if (q - a).abs() > h {
                return false;
            }
        } else {
            let (t0, t1) = ((q - h - a) / u, (q + h - a) / u);
            lo = lo.max(t0.min(t1));
            hi = hi.min(t0.max(t1));
        }
    }
    lo <= hi
}

/// 曲がり・交差・分かれの場面で、帯がずれない(1 路線だけの辺の)線路のマスの絵は、そのマスと隣のマスの
/// 「中心 → 辺の中点(角)」の線分を線路の縁の太さでなぞった形の中にだけある。中心の向こうへ線が突き出て X 字にならない。
#[test]
fn bends_and_crossings_have_no_ink_past_the_half_segments() {
    use crate::draw::map::{EdgeLanes, RAIL_OUTLINE_W, band_pieces, half_seg};
    use crate::draw::palette::{GROUND_A, GROUND_B};
    use crate::layout::{SCREEN_W, TILE};
    use tm_assets::sprite::BandVariant;
    use tm_core::{Dir8, GridPos, LineId};
    let h = track_shapes();
    let view = h.game.as_ref().unwrap().view();
    let cache = h.ui.play().unwrap().map.as_ref().unwrap();
    let refs: Vec<(LineId, &[GridPos])> = cache
        .paths
        .iter()
        .enumerate()
        .map(|(i, p)| (LineId::from_index(i), p.as_slice()))
        .collect();
    let pieces = band_pieces(&refs, &EdgeLanes::build(&refs));
    let grid = view.track_grid();
    let (gw, gh) = (i32::from(grid.width), i32::from(grid.height));
    let mask = |x: i32, y: i32| {
        if (0..gw).contains(&x) && (0..gh).contains(&y) {
            grid.masks[(y * gw + x) as usize]
        } else {
            tm_core::TrackMask::default()
        }
    };
    let stations: Vec<GridPos> = view.stations().map(|s| s.pos).collect();
    let px = super::raster::logical(&layer_pixels(&h, is_track));
    let ground = [GROUND_A, GROUND_B].map(|c| [c.r, c.g, c.b]);
    let (mut bends, mut crossings) = (0, 0);
    for ty in 0..10 {
        for tx in 0..20 {
            let t = GridPos { x: tx, y: ty };
            let m = mask(tx, ty);
            let near = |p: GridPos| (p.x - tx).abs() <= 1 && (p.y - ty).abs() <= 1;
            if m.is_empty()
                || stations.contains(&t)
                || pieces
                    .iter()
                    .any(|p| near(p.tile) && p.variant != BandVariant::Center)
            {
                continue;
            }
            let mut segs = Vec::new();
            for (x, y) in (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (tx + dx, ty + dy))) {
                for d in Dir8::ALL.into_iter().filter(|&d| mask(x, y).contains(d)) {
                    segs.push(half_seg(GridPos { x, y }, d, None));
                }
            }
            for y in ty * TILE..(ty + 1) * TILE {
                for x in tx * TILE..(tx + 1) * TILE {
                    if ground.contains(&px[(y * SCREEN_W + x) as usize]) {
                        continue;
                    }
                    assert!(
                        segs.iter().any(|&s| in_pen(s, RAIL_OUTLINE_W / 2, x, y)),
                        "マス ({tx}, {ty}) の画素 ({x}, {y}) が線分の外"
                    );
                }
            }
            let dirs: Vec<Dir8> = Dir8::ALL.into_iter().filter(|&d| m.contains(d)).collect();
            bends += usize::from(dirs.len() == 2 && dirs[0] != dirs[1].opposite());
            crossings += usize::from(dirs.len() == 4);
        }
    }
    assert!(
        bends >= 4 && crossings >= 2,
        "曲がり {bends}、交差 {crossings}"
    );
}

#[test]
fn static_layer_is_drawn_in_four_passes_then_stations() {
    let h = shared_lines();
    let view = h.game.as_ref().unwrap().view();
    assert_eq!(view.lines().len(), 3);
    let layer = &h.ui.play().unwrap().map.as_ref().unwrap().layer;
    let passes: Vec<Step> = layer.iter().map(pass_of).collect();
    let mut sorted = passes.clone();
    sorted.sort();
    assert_eq!(
        passes, sorted,
        "Clear → 市松 → 線路の縁 → 線路の中身 → 帯の縁 → 帯の中身 → 駅 → つうか → 札"
    );
    let (w, h) = view.map_size();
    let (w, h) = (i32::from(w), i32::from(h));
    assert_eq!(
        passes.iter().filter(|p| **p == Step::Ground).count(),
        (w * h / 2) as usize + crate::draw::map::off_map(w, h).count(),
        "市松とマップの外"
    );
    let n = |p: Step| passes.iter().filter(|q| **q == p).count();
    assert_eq!(n(Step::RailOutline), n(Step::RailFill));
    assert_eq!(n(Step::BandOutline), n(Step::BandFill));
    let rail_bits: u32 = view
        .track_grid()
        .masks
        .iter()
        .map(|m| m.0.count_ones())
        .sum();
    assert_eq!(
        n(Step::RailOutline),
        rail_bits as usize,
        "TrackMask のビットごとに 1 つ"
    );
    assert_eq!(n(Step::Station), view.stations().len());
    assert_eq!(n(Step::Badge), 6, "路線ごとに両端");
}

#[test]
fn shared_edges_use_three_lanes_with_line_colors() {
    let h = shared_lines();
    let layer = &h.ui.play().unwrap().map.as_ref().unwrap().layer;
    let fills: Vec<(String, tm_platform_api::Color)> = layer
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Sprite { id, tint, .. } if sprite::name(*id).starts_with("trk_band_f") => {
                Some((sprite::name(*id).to_string(), *tint))
            }
            _ => None,
        })
        .collect();
    assert!(fills.iter().any(|(n, _)| n.ends_with("_l0p")));
    assert!(fills.iter().any(|(n, _)| n.ends_with("_l1")));
    assert!(fills.iter().any(|(n, _)| n.ends_with("_l2p")));
    for (_, tint) in &fills {
        assert!(crate::draw::palette::LINES.contains(tint));
    }
}

#[test]
fn layer_positions_are_in_layer_coordinates() {
    let h = shared_lines();
    let layer = &h.ui.play().unwrap().map.as_ref().unwrap().layer;
    let view = h.game.as_ref().unwrap().view();
    let st = view.stations().next().unwrap();
    let (lx, ly) = crate::layout::tile_layer(st.pos);
    let want = tm_platform_api::Rect {
        x: lx + 2,
        y: ly + 2,
        w: 28,
        h: 28,
    };
    assert!(layer.iter().any(
        |c| matches!(c, DrawCmd::Sprite { id, dst, .. } if *id == sprite::STN_BODY && *dst == want)
    ));
}

#[test]
fn line_badges_in_the_shared_scene_do_not_overlap() {
    let h = shared_lines();
    let layer = &h.ui.play().unwrap().map.as_ref().unwrap().layer;
    let badges: Vec<tm_platform_api::Rect> = layer
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Sprite { id, dst, .. } if sprite::name(*id).starts_with("ico_line_no") => {
                Some(*dst)
            }
            _ => None,
        })
        .collect();
    assert_eq!(badges.len(), 6);
    for (i, a) in badges.iter().enumerate() {
        for b in &badges[i + 1..] {
            let apart = a.x + a.w as i32 <= b.x
                || b.x + b.w as i32 <= a.x
                || a.y + a.h as i32 <= b.y
                || b.y + b.h as i32 <= a.y;
            assert!(apart, "札が重なる {a:?} {b:?}");
        }
    }
}

/// 40 × 18 より狭いマップ(32 × 18): 層は 40 × 18 の大きさ、マップの外は塗るだけで何も置かない。
#[test]
fn small_map_fills_the_left_of_the_display_area() {
    use crate::layout::{LAYER_W, MAP_COLS, MAP_ROWS, MAP_X, MAP_Y, TILE, WORLD_H, WORLD_W};
    let mut spec = tm_core::BalanceTable::builtin().spec().clone();
    spec.map.width = 32;
    let table = tm_core::BalanceTable::from_spec(spec).expect("検証を通る表");
    let game = tm_core::Game::with_station_plan(
        tm_core::GameConfig {
            seed: 7,
            balance: std::sync::Arc::new(table),
        },
        super::plan(),
    );
    let mut h = UiHarness::from_game(game, false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(3);
    let view = h.game.as_ref().unwrap().view();
    assert_eq!(view.lines().len(), 1);
    let (w, rows) = view.map_size();
    let (w, rows) = (i32::from(w), i32::from(rows));
    assert!(w < MAP_COLS && rows == MAP_ROWS);
    let cam = h.ui.play().unwrap().camera.pos;
    let cmds = &h.draw_list().cmds;
    let begin = cmds
        .iter()
        .find_map(|c| match *c {
            DrawCmd::BeginCachedLayer { x, y, w, h, .. } => Some((x, y, w, h)),
            _ => None,
        })
        .expect("層");
    assert_eq!(
        begin,
        (MAP_X - cam.x, MAP_Y - cam.y, LAYER_W as u32, WORLD_H as u32)
    );
    assert!(
        cam.x <= w * TILE - crate::layout::MAP_W,
        "カメラはマップの右端より先へ行かない"
    );
    let layer = &h.ui.play().unwrap().map.as_ref().unwrap().layer;
    let map_right = w * TILE;
    let mut off = 0;
    for c in layer {
        match *c {
            DrawCmd::Sprite { dst, .. } => assert!(
                dst.x < map_right && dst.y < rows * TILE,
                "マップの外に絵を置かない: {dst:?}"
            ),
            DrawCmd::FillRect { rect, color } if rect.x >= map_right || rect.y >= rows * TILE => {
                assert_eq!(color, crate::draw::palette::OFF_MAP);
                off += rect.w * rect.h;
            }
            _ => {}
        }
    }
    assert_eq!(
        off as i32,
        WORLD_W * WORLD_H - w * rows * TILE * TILE,
        "マップの外(右の 8 マス)を塗る"
    );
}
