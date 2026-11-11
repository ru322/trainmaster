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
