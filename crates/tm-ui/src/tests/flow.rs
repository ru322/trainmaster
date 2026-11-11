//! タイトル → ゲーム、カーソル、決定論、描画が状態を変えないこと。

use tm_input::Action;
use tm_platform_api::{DrawCmd, DrawList, MonoTime};

use super::{fill_at, harness_with_line, map_tiles, plan};
use crate::AppRequest;
use crate::layout::{BAND_ROW2, HUD, INFO_ROW, LAYER_W, MAP_SCALE, MAP_X, MAP_Y, TILE, WORLD_H};
use crate::scene::cursor_start;
use crate::testing::{FRAME_US, UiHarness};

#[test]
fn title_ignores_confirm_for_500ms() {
    let mut h = UiHarness::new_title();
    h.press(Action::Confirm);
    h.frames(25);
    assert!(h.requests.is_empty());
    assert!(h.now.0 < 500_000 + 3 * FRAME_US);
    h.frames(5);
    h.press(Action::Confirm);
    assert_eq!(h.requests, [AppRequest::StartNewGame]);
    assert!(h.game.is_some() && h.tutorial.is_some());
    assert!(h.ui.map_cursor().is_some());
}

#[test]
fn first_play_frame_draws_only_background() {
    let mut h = UiHarness::new_title();
    h.frames(40);
    h.press(Action::Menu);
    assert!(h.requests.is_empty(), "タイトルでは Menu を無視");
    h.key_down(Action::Confirm);
    h.frames(1);
    assert_eq!(h.requests, [AppRequest::StartNewGame]);
    assert_eq!(
        h.draw_text(),
        "Clear #000000FF\n",
        "Session は update の後に作られる(13 §3.3)"
    );
    h.key_up(Action::Confirm);
    h.frames(1);
    assert!(h.draw_text().contains("BeginLayer 0"));
}

#[test]
fn go_to_title_keeps_settings_and_resets_scene() {
    let mut h = harness_with_line();
    assert!(!h.ui.is_paused(h.tutorial.as_ref()));
    let mut audio = Vec::new();
    h.ui.go_to_title(&mut audio);
    assert!(h.ui.map_cursor().is_none());
    assert!(h.ui.show_explanations());
    assert!(!h.ui.is_paused(None));
}

#[test]
fn initial_cursor_is_center_or_left_of_a_station() {
    let h = harness_with_line();
    let (w, rows) = map_tiles(&h);
    let start = cursor_start(h.game.as_ref().unwrap().view());
    assert_eq!(
        start,
        tm_core::GridPos {
            x: w / 2,
            y: rows / 2
        },
        "マップの中央"
    );
    assert_eq!(h.ui.map_cursor(), Some(start));
    let mut p = plan();
    p[1] = start;
    let mut h = UiHarness::with_game(p, true);
    h.frames(1);
    assert_eq!(
        h.ui.map_cursor(),
        Some(tm_core::GridPos {
            x: start.x - 1,
            y: start.y
        })
    );
}

#[test]
fn move_cursor_to_reaches_the_target() {
    let mut h = harness_with_line();
    let target = tm_core::GridPos { x: 3, y: 15 };
    h.move_cursor_to(target);
    assert_eq!(h.ui.map_cursor(), Some(target));
    let (w, rows) = map_tiles(&h);
    let corner = tm_core::GridPos { x: w - 1, y: 0 };
    h.move_cursor_to(corner);
    assert_eq!(h.ui.map_cursor(), Some(corner));
    h.press(Action::MoveRight);
    h.press(Action::MoveUp);
    assert_eq!(h.ui.map_cursor(), Some(corner), "マップの右上で止まる");
    let last = tm_core::GridPos {
        x: w - 1,
        y: rows - 1,
    };
    h.move_cursor_to(last);
    h.press(Action::MoveDown);
    assert_eq!(h.ui.map_cursor(), Some(last));
}

#[test]
fn same_inputs_give_the_same_frames() {
    let run = || {
        let mut h = UiHarness::with_seed(42, true, MonoTime(1_000));
        let mut out = Vec::new();
        for i in 0..90 {
            if i % 30 == 0 {
                h.press(Action::MoveRight);
            }
            h.frames(1);
            out.push(h.draw_text());
        }
        out
    };
    assert_eq!(run(), run());
}

#[test]
fn render_does_not_change_state() {
    let mut h = harness_with_line();
    h.frames(100);
    let view = h.game.as_ref().map(tm_core::Game::view);
    let mut a = DrawList::default();
    let mut b = DrawList::default();
    h.ui.render(view, h.tutorial.as_ref(), &mut a);
    h.ui.render(view, h.tutorial.as_ref(), &mut b);
    assert_eq!(a.cmds, b.cmds);
    assert_eq!(a.cmds, h.draw_list().cmds);
}

#[test]
fn cached_layer_is_drawn_every_frame_and_rebuilt_only_on_revision() {
    let mut h = harness_with_line();
    let builds = h.ui.play().unwrap().map_builds;
    let mut revs = Vec::new();
    for _ in 0..30 {
        h.frames(1);
        let cam = h.ui.play().unwrap().camera.pos;
        let cmds = &h.draw_list().cmds;
        let begins: Vec<usize> = cmds
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c, DrawCmd::BeginCachedLayer { .. }))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            begins.len(),
            2,
            "カメラが真ん中なら左右の 2 枚とも毎フレーム Begin"
        );
        for begin in begins {
            let DrawCmd::BeginCachedLayer {
                layer,
                revision,
                x,
                y,
                w,
                h: lh,
            } = cmds[begin]
            else {
                unreachable!()
            };
            let end = begin
                + cmds[begin..]
                    .iter()
                    .position(|c| *c == DrawCmd::EndCachedLayer)
                    .unwrap();
            assert_eq!(cmds[end + 1], DrawCmd::DrawCachedLayer { layer });
            let k = i32::from(layer.0);
            assert_eq!(
                (x, y, w, lh),
                (
                    MAP_X + k * LAYER_W - cam.x,
                    MAP_Y - cam.y,
                    LAYER_W as u32,
                    WORLD_H as u32
                )
            );
            assert!(
                matches!(cmds[begin + 1], DrawCmd::Clear(c) if c.a == 255),
                "層は不透明の Clear で始める"
            );
            revs.push(revision);
        }
    }
    assert!(revs.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(
        h.ui.play().unwrap().map_builds,
        builds,
        "revision が同じなら作り直さない"
    );
    assert_eq!(
        revs[0],
        h.ui.play().unwrap().map.as_ref().unwrap().layer_version
    );
}

#[test]
fn new_line_bumps_revision_and_rebuilds_once() {
    let mut h = UiHarness::from_game(super::cheap_game(), false);
    h.frames(1);
    let rev0 = h.ui.play().unwrap().map.as_ref().unwrap().revision;
    let v0 = h.ui.play().unwrap().map.as_ref().unwrap().layer_version;
    let builds = h.ui.play().unwrap().map_builds;
    let g = h.game.as_mut().unwrap();
    let cmd = super::create_line(&[0, 1]);
    g.validate(&cmd).expect("作れる");
    g.enqueue(cmd);
    h.frames(1);
    let rev1 = h.game.as_ref().unwrap().view().map_revision();
    assert_ne!(rev0, rev1);
    let layer_rev = |h: &UiHarness| {
        h.draw_list().cmds.iter().find_map(|c| match c {
            DrawCmd::BeginCachedLayer { revision, .. } => Some(*revision),
            _ => None,
        })
    };
    assert_eq!(
        layer_rev(&h),
        Some(v0),
        "中身を作ったときの版を渡す(1 フレーム遅れ)"
    );
    h.frames(1);
    assert_eq!(layer_rev(&h), Some(v0 + 1));
    assert_eq!(h.ui.play().unwrap().map_builds, builds + 1);
    h.frames(10);
    assert_eq!(h.ui.play().unwrap().map_builds, builds + 1);
}

fn start_game(h: &mut UiHarness, seed: u64) {
    h.new_game_seed = seed;
    h.frames(40);
    h.press(Action::Confirm);
    h.frames(3);
}

/// 最初の層の版と、マップ全体の層の中身(層の座標)。
fn layer_of(h: &UiHarness) -> (u64, Vec<DrawCmd>) {
    let cmds = &h.draw_list().cmds;
    let begin = cmds
        .iter()
        .position(|c| matches!(c, DrawCmd::BeginCachedLayer { .. }))
        .expect("層");
    let DrawCmd::BeginCachedLayer { revision, .. } = cmds[begin] else {
        unreachable!()
    };
    let map = h.ui.play().unwrap().map.as_ref().unwrap();
    assert_eq!(map.layer_version, revision);
    (revision, map.layer.clone())
}

#[test]
fn reset_changes_the_layer_version_and_shows_the_new_stations() {
    let mut h = UiHarness::new_title();
    start_game(&mut h, 1);
    assert_eq!(h.game.as_ref().unwrap().view().map_revision(), 0);
    let (rev_a, layer_a) = layer_of(&h);
    h.reset_to_title();
    start_game(&mut h, 2);
    let view = h.game.as_ref().unwrap().view();
    assert_eq!(view.map_revision(), 0, "新しいゲームの map_revision も 0");
    let (rev_b, layer_b) = layer_of(&h);
    assert_ne!(layer_a, layer_b, "シードが違えば駅の配置が違う");
    assert_ne!(rev_a, rev_b, "中身が違えば層の版も違う(14 §3.5.2)");
    for st in view.stations() {
        let want = tm_platform_api::Rect {
            x: st.pos.x * TILE + MAP_SCALE,
            y: st.pos.y * TILE + MAP_SCALE,
            w: 14 * MAP_SCALE as u32,
            h: 14 * MAP_SCALE as u32,
        };
        assert!(
            layer_b
                .iter()
                .any(|c| matches!(c, DrawCmd::Sprite { dst, .. } if *dst == want)),
            "新しいゲームの駅 {:?} が描かれる",
            st.pos
        );
    }
}

#[test]
fn map_layers_are_drawn_in_order() {
    let mut h = harness_with_line();
    h.frames(240);
    h.move_cursor_to(plan()[0]);
    h.frames(30);
    let now = h.now;
    let ui_observe =
        |h: &mut UiHarness, ev: tm_core::GameEvent| h.ui.observe(now, &ev, &mut Vec::new());
    ui_observe(
        &mut h,
        tm_core::GameEvent::TrainArrived {
            train: tm_core::TrainId(0),
            line: tm_core::LineId(0),
            station: tm_core::StationId(1),
            riders: tm_core::People(1),
            alighted: tm_core::People(1),
            boarded: tm_core::People(1),
            income: tm_numeric::BigNum::from_u64(5),
            dwell: tm_core::Ticks(1),
        },
    );
    ui_observe(
        &mut h,
        tm_core::GameEvent::StationSpawned {
            id: tm_core::StationId(0),
            pos: plan()[0],
        },
    );
    ui_observe(
        &mut h,
        tm_core::GameEvent::LineCreated {
            line: tm_core::LineId(0),
            station_count: 3,
            cost: tm_core::Money::ZERO,
            new_tiles: 0,
        },
    );
    h.frames(1);
    let text = h.draw_text();
    let lines: Vec<&str> = text.lines().collect();
    let first = |p: &str| {
        lines
            .iter()
            .position(|l| l.contains(p))
            .unwrap_or_else(|| panic!("{p} がない"))
    };
    let last = |p: &str| {
        lines
            .iter()
            .rposition(|l| l.contains(p))
            .unwrap_or_else(|| panic!("{p} がない"))
    };
    let (hud, info, band) = (fill_at(HUD), fill_at(INFO_ROW), fill_at(BAND_ROW2));
    let order = [
        ("DrawLayer 0", "DrawLayer 0"),
        ("#F2CF7F", "#F2CF7F"),
        ("Sprite trn_", "Sprite trn_"),
        ("Sprite pax_dots", "Sprite pax_dots"),
        ("Sprite fx_sparkle", "Sprite '+'12"),
        ("Sprite ui_map_cursor", "Sprite ui_map_cursor"),
        (hud.as_str(), hud.as_str()),
        (info.as_str(), info.as_str()),
        (band.as_str(), band.as_str()),
    ];
    for w in order.windows(2) {
        assert!(
            last(w[0].1) < first(w[1].0),
            "{} は {} より前",
            w[0].1,
            w[1].0
        );
    }
}

/// タイトルから始めたものと `with_seed` の一致(APP-T-21 の揃え方を tm-ui の中で確かめる)。
#[test]
fn with_seed_matches_a_game_started_from_the_title() {
    let seed = 9;
    let mut a = UiHarness::new_title();
    a.new_game_seed = seed;
    a.frames(40);
    a.key_down(Action::Confirm);
    a.frames(1);
    assert_eq!(a.requests, [AppRequest::StartNewGame]);
    let start = a.now;
    let mut b = UiHarness::with_seed(seed, true, start);
    a.key_up(Action::Confirm);
    let script: Vec<Option<Action>> = (0..900)
        .map(|i| match i {
            1 | 30 => Some(Action::Menu),
            60 | 400 | 720 => Some(Action::Confirm),
            200 => Some(Action::MoveLeft),
            300 => Some(Action::Menu),
            330 => Some(Action::Menu),
            _ => None,
        })
        .collect();
    let mut pending: Option<Action> = None;
    for (i, act) in script.iter().enumerate() {
        for h in [&mut a, &mut b] {
            if let Some(u) = pending {
                h.key_up(u);
            }
            if let Some(x) = *act {
                h.key_down(x);
            }
        }
        pending = *act;
        let (ma, mb) = (a.audio.len(), b.audio.len());
        a.frames(1);
        b.frames(1);
        assert_eq!(a.now, b.now, "フレーム {i}");
        assert_eq!(a.audio[ma..], b.audio[mb..], "フレーム {i} の音");
        assert_eq!(a.draw_text(), b.draw_text(), "フレーム {i} の描画");
        assert_eq!(
            a.game.as_ref().unwrap().now(),
            b.game.as_ref().unwrap().now(),
            "フレーム {i} の tick"
        );
    }
    assert!(
        a.audio.contains(&tm_platform_api::AudioCmd::PlaySe(
            tm_assets::sound::SE_POPUP
        )),
        "説明が出ている"
    );
}

#[test]
fn with_seed_keeps_the_show_setting_and_does_not_reboot() {
    for show in [true, false] {
        let mut h = UiHarness::with_seed(3, show, MonoTime(5_000_000));
        assert_eq!(h.ui.show_explanations(), show, "UI の設定も同じ");
        h.frames(1);
        assert!(
            !h.audio.iter().any(|c| matches!(
                c,
                tm_platform_api::AudioCmd::SetVolume { .. }
                    | tm_platform_api::AudioCmd::PlayBgm { .. }
            )),
            "起動の音とゲームの曲はタイトルで出したことにする"
        );
        assert_eq!(h.now, MonoTime(5_000_000 + FRAME_US));
    }
}

/// 生成した駅の配置で全部の駅を出し、どの駅にもカーソルが届く(右の 8 列を含む。IMP-805)。
#[test]
fn cursor_reaches_every_station() {
    use crate::layout::{MAP_COLS, MAP_ROWS};
    use tm_core::{BalanceTable, Game, GameConfig, GridPos};
    let mut spec = BalanceTable::builtin().spec().clone();
    for p in &mut spec.params {
        p.base_yen = 1;
        p.cost_ratio = 1;
        p.cost_steps.clear();
    }
    spec.economy.initial_money_yen = 1_000_000_000_000_000;
    spec.company.gauge_num = 1;
    let table = std::sync::Arc::new(BalanceTable::from_spec(spec).expect("検証を通る表"));
    const OLD_COLS: i32 = MAP_COLS - 8;
    let mut right = 0;
    for seed in [1, 42] {
        let game = Game::new(GameConfig {
            seed,
            balance: std::sync::Arc::clone(&table),
        });
        let mut h = UiHarness::from_game(game, false);
        h.game
            .as_mut()
            .unwrap()
            .enqueue(super::create_line(&[0, 1, 2]));
        h.frames(2);
        for _ in 0..200 {
            if h.game.as_ref().unwrap().view().stations().len() == table.max_stations() as usize {
                break;
            }
            super::buy_all(&mut h);
            h.frames(2);
        }
        let view = h.game.as_ref().unwrap().view();
        let stations: Vec<GridPos> = view.stations().map(|s| s.pos).collect();
        assert_eq!(stations.len(), table.max_stations() as usize);
        assert_eq!(map_tiles(&h), (MAP_COLS, MAP_ROWS));
        right += stations.iter().filter(|p| p.x >= OLD_COLS).count();
        for p in stations {
            h.move_cursor_to(p);
            assert_eq!(h.ui.map_cursor(), Some(p), "seed {seed}");
        }
    }
    assert!(right > 0, "広げた右の 8 列にも駅がある");
}

/// カーソルの範囲と初期位置は `GameView::map_size()` から決める(表示域 40 × 18 の定数ではない)。
#[test]
fn cursor_follows_the_map_size_of_the_game() {
    use tm_core::{BalanceTable, Game, GameConfig, GridPos};
    let mut spec = BalanceTable::builtin().spec().clone();
    spec.map.width = 24;
    spec.map.height = 14;
    let table = BalanceTable::from_spec(spec).expect("検証を通る表");
    let plan: Vec<GridPos> = (0..table.max_stations() as i32)
        .map(|i| GridPos {
            x: 1 + (i % 7) * 3,
            y: 1 + (i / 7) * 4,
        })
        .collect();
    let game = Game::with_station_plan(
        GameConfig {
            seed: 1,
            balance: std::sync::Arc::new(table),
        },
        plan,
    );
    let mut h = UiHarness::from_game(game, false);
    h.frames(1);
    assert_eq!(map_tiles(&h), (24, 14));
    assert_eq!(h.ui.map_cursor(), Some(GridPos { x: 12, y: 7 }), "中央");
    let corner = GridPos { x: 23, y: 13 };
    h.move_cursor_to(corner);
    h.press(Action::MoveRight);
    h.press(Action::MoveDown);
    assert_eq!(h.ui.map_cursor(), Some(corner), "マップの右下で止まる");
}

/// カメラの範囲はマップの大きさで決まる(32 マスなら、パネルを開いて 0〜608、閉じて 0〜384)。
#[test]
fn camera_range_follows_the_map_width_of_the_game() {
    use crate::layout::{MAP_H, MAP_W, MAP_W_PANEL};
    use tm_core::{BalanceTable, Game, GameConfig, GridPos};
    let mut spec = BalanceTable::builtin().spec().clone();
    spec.map.width = 32;
    let table = BalanceTable::from_spec(spec).expect("検証を通る表");
    let plan: Vec<GridPos> = (0..table.max_stations() as i32)
        .map(|i| GridPos {
            x: 1 + (i % 7) * 3,
            y: 1 + (i / 7) * 4,
        })
        .collect();
    let game = Game::with_station_plan(
        GameConfig {
            seed: 1,
            balance: std::sync::Arc::new(table),
        },
        plan,
    );
    let mut h = UiHarness::from_game(game, false);
    h.frames(1);
    h.move_cursor_to(GridPos { x: 31, y: 17 });
    h.frames(30);
    let cam = h.ui.play().unwrap().camera.pos;
    assert_eq!(
        (cam.x, cam.y),
        (32 * TILE - MAP_W, 18 * TILE - MAP_H),
        "右下の端"
    );
    h.press(Action::TabNext);
    h.frames(30);
    assert_eq!(h.ui.play().unwrap().camera.pos.x, 32 * TILE - MAP_W_PANEL);
}
