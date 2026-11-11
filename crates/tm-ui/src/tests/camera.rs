//! カメラの追従(縦横、どの入力でも、パネル、路線の確認ダイアログ、マップの端)。

use tm_assets::sprite;
use tm_core::{GridPos, StationId};
use tm_input::Action;
use tm_platform_api::{DrawCmd, Rect};
use tm_tutorial::{DirectiveKind, UiAnchor};

use super::explain::fake;
use super::screens::{play, play_mut, wait};
use super::{cheap_game, create_line, harness_with_line, plan};
use crate::anchor::{Mark, Toward};
use crate::anim::overlaps;
use crate::draw::DrawBuilder;
use crate::draw::map;
use crate::layout::{
    Cam, MAP_H, MAP_W, MAP_W_PANEL, MAP_X, MAP_Y, TILE, WORLD_H, WORLD_W, tile_center,
    tile_in_view, tile_screen,
};
use crate::scene::PlayMode;
use crate::testing::{FRAME_US, UiHarness};

/// 画面に描かれた表示域の縁の矢印(2 倍の絵)の向きと、縁に沿った位置(左右なら三角の y の中心、上下なら矢印の先の x)。
/// 縁から 2〜6px(はねる分)の所に、その縁を向く絵があるものだけを数える。
pub(super) fn drawn_edge(h: &UiHarness) -> Option<(Toward, i32)> {
    let right = MAP_X + play(h).map_w();
    let bottom = MAP_Y + MAP_H;
    let near = |v: i32, at: i32| (at..=at + 4).contains(&v);
    let found: Vec<(Toward, i32)> = h
        .draw_list()
        .cmds
        .iter()
        .filter_map(|c| match *c {
            DrawCmd::Sprite { id, dst, .. } => {
                let (x, y, w, hh) = (dst.x, dst.y, dst.w as i32, dst.h as i32);
                if id == sprite::ICO_TRI_LEFT && w == 16 && near(x, MAP_X + 2) {
                    Some((Toward::Left, y + hh / 2))
                } else if id == sprite::ICO_TRI_RIGHT && w == 16 && near(right - 2 - (x + w), 0) {
                    Some((Toward::Right, y + hh / 2))
                } else if id == sprite::FX_ARROW_UP && w == 22 && near(y, MAP_Y + 2) {
                    Some((Toward::Up, x + 10))
                } else if id == sprite::FX_ARROW_DOWN && w == 22 && near(bottom - 2 - (y + hh), 0) {
                    Some((Toward::Down, x + 10))
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect();
    assert!(found.len() <= 1, "縁の矢印は 1 つ: {found:?}");
    found.first().copied()
}

fn cursor(h: &UiHarness) -> GridPos {
    h.ui.map_cursor().expect("ゲーム中")
}

/// カーソルが表示域に入っていて、カメラがマップの範囲の中にある。
fn assert_cursor_visible(h: &UiHarness, what: &str) {
    let p = play(h);
    let cam = p.camera.pos;
    assert!(
        tile_in_view(cursor(h), cam, p.map_w()),
        "{what}: カーソル {:?} が表示域の外(カメラ {cam:?})",
        cursor(h)
    );
    assert!(
        (0..=WORLD_W - p.map_w()).contains(&cam.x) && (0..=WORLD_H - MAP_H).contains(&cam.y),
        "{what}: カメラ {cam:?} がマップの外"
    );
}

#[test]
fn holding_a_direction_with_keyboard_or_gamepad_keeps_the_cursor_in_view() {
    for gamepad in [false, true] {
        let mut h = harness_with_line();
        if gamepad {
            h.use_gamepad();
            h.frames(1);
        }
        let start = play(&h).camera.pos;
        for (a, ms) in [
            (Action::MoveRight, 5_000),
            (Action::MoveDown, 3_000),
            (Action::MoveLeft, 5_000),
            (Action::MoveUp, 3_000),
        ] {
            h.key_down(a);
            for _ in 0..ms * 1000 / FRAME_US {
                h.frames(1);
                assert_cursor_visible(&h, &format!("{a:?} gamepad={gamepad}"));
            }
            h.key_up(a);
            wait(&mut h, 500);
            let cam = play(&h).camera.pos;
            match a {
                Action::MoveRight => assert_eq!(cam.x, WORLD_W - MAP_W, "右の端で止まる"),
                Action::MoveDown => assert_eq!(cam.y, WORLD_H - MAP_H, "下の端で止まる"),
                Action::MoveLeft => assert_eq!(cam.x, 0, "左の端で止まる"),
                _ => assert_eq!(cam.y, 0, "上の端で止まる"),
            }
        }
        assert_ne!(start, play(&h).camera.pos, "縦にも横にも動いた");
        assert_eq!(cursor(&h), GridPos { x: 0, y: 0 });
    }
}

#[test]
fn single_steps_scroll_only_past_the_two_tile_margin() {
    let mut h = harness_with_line();
    let start = play(&h).camera.target;
    let (x0, _) = tile_screen(cursor(&h), play(&h).camera.pos);
    let steps = (MAP_W - 2 * TILE - TILE - x0) / TILE;
    for _ in 0..steps {
        h.press(Action::MoveRight);
    }
    assert_eq!(
        play(&h).camera.target,
        start,
        "右の余白(2 マス)までは動かない"
    );
    h.press(Action::MoveRight);
    let t = play(&h).camera.target;
    assert!(t.x > start.x);
    assert_eq!(
        (cursor(&h).x + 1) * TILE - t.x,
        MAP_W - 2 * TILE,
        "カーソルの右に 2 マスを残すところまで動く"
    );
    assert_eq!(t.y, start.y, "横に動いても縦は動かない");
}

#[test]
fn the_panel_keeps_the_cursor_visible_when_it_narrows_the_view() {
    let mut h = harness_with_line();
    h.move_cursor_to(GridPos { x: 24, y: 9 });
    wait(&mut h, 500);
    let p = play(&h);
    let (x, _) = tile_screen(cursor(&h), p.camera.pos);
    assert!(
        x + TILE > crate::layout::MAP_W_PANEL,
        "前提: パネルの下になる位置"
    );
    h.press(Action::TabNext);
    assert!(matches!(play(&h).mode, PlayMode::Panel(_)));
    wait(&mut h, 500);
    assert_cursor_visible(&h, "パネルを開いた");
    h.press(Action::Cancel);
    assert_cursor_visible(&h, "パネルを閉じた直後");
    wait(&mut h, 500);
    assert_cursor_visible(&h, "パネルを閉じた");
}

/// 確認ダイアログの矩形と、カーソルのマスの画面の矩形。
fn dialog_and_cursor(h: &UiHarness) -> (Rect, Rect) {
    let p = play(h);
    let PlayMode::Build(b) = &p.mode else {
        panic!("ろせんモード")
    };
    let d = b.dialog_area().expect("確認ダイアログ");
    let (x, y) = tile_screen(cursor(h), p.camera.pos);
    (d, crate::draw::rect(x, y, TILE as u32, TILE as u32))
}

#[test]
fn the_line_confirm_dialog_never_covers_the_cursor() {
    for route in [[0, 1, 2], [2, 1, 0], [1, 2, 0]] {
        let mut h = UiHarness::from_game(cheap_game(), false);
        h.frames(2);
        h.press(Action::BuildMode);
        h.press(Action::Confirm);
        for &s in &route {
            h.move_cursor_to(plan()[s]);
            h.press(Action::Confirm);
        }
        h.press(Action::Confirm);
        for t in [0, 30] {
            h.frames(t);
            let (d, c) = dialog_and_cursor(&h);
            assert!(!overlaps(d, c), "{route:?} {t}: {d:?} / {c:?}");
            assert_cursor_visible(&h, &format!("{route:?} {t}"));
        }
    }
}

#[test]
fn camera_motion_is_deterministic() {
    let run = || {
        let mut h = harness_with_line();
        let mut trace = Vec::new();
        for a in [Action::MoveRight, Action::MoveDown, Action::TabNext] {
            h.hold(a, 700);
            for _ in 0..20 {
                h.frames(1);
                trace.push(play(&h).camera.pos);
            }
        }
        trace
    };
    assert_eq!(run(), run());
}

/// 始めのカメラは始めの駅の外接矩形の中心に置く(01 §6.1: 始めの 3 駅はマップの中央 16 × 10 マス。表示域は 20 × 9 マス)。
/// 外接矩形が表示域に入るシードでは 3 駅とカーソルがすべて見え、説明を出しても動かない。
#[test]
fn the_first_screen_shows_the_starting_stations() {
    let mut fitting = 0;
    for seed in 1..=40 {
        for explain in [false, true] {
            let mut h = UiHarness::with_seed(seed, explain, tm_platform_api::MonoTime(0));
            h.frames(1);
            let st: Vec<GridPos> = h
                .game
                .as_ref()
                .unwrap()
                .view()
                .stations()
                .map(|s| s.pos)
                .collect();
            assert_eq!(st.len(), 3);
            let span = |f: fn(&GridPos) -> i32| {
                st.iter().map(f).max().unwrap() - st.iter().map(f).min().unwrap() + 1
            };
            let fits = span(|p| p.x) <= MAP_W / TILE && span(|p| p.y) <= MAP_H / TILE;
            for t in [0, 120] {
                h.frames(t);
                assert_cursor_visible(&h, &format!("seed {seed} {t}"));
                let cam = play(&h).camera.pos;
                let seen = st.iter().filter(|&&p| tile_in_view(p, cam, MAP_W)).count();
                if fits {
                    assert_eq!(seen, 3, "seed {seed} explain={explain} {t}: {st:?} {cam:?}");
                } else {
                    assert!(seen >= 1, "seed {seed}: 入りきらなくても中心は見せる");
                }
            }
            fitting += usize::from(fits && !explain);
        }
    }
    assert!(
        fitting >= 35,
        "ほとんどのシードで外接矩形が入る: {fitting}/40"
    );
}

/// 始めの 3 駅の 1 つ目を `target` に置き、カーソルを `cursor` へ動かして、1 つ目の駅を案内で指す。
fn guide_to_far_station(target: GridPos, others: [GridPos; 2], cursor: GridPos) -> UiHarness {
    let mut p = plan();
    p[0] = target;
    p[1] = others[0];
    p[2] = others[1];
    let mut h = UiHarness::with_game(p, false);
    h.frames(1);
    h.move_cursor_to(cursor);
    wait(&mut h, 500);
    play_mut(&mut h).fake_directive = Some(fake(
        3,
        DirectiveKind::Guide,
        Some(UiAnchor::Station(StationId(0))),
    ));
    h.frames(30);
    h
}

#[test]
fn edge_arrows_are_drawn_on_the_side_of_a_hidden_target() {
    let g = |x, y| GridPos { x, y };
    for (target, cursor, toward) in [
        (g(2, 9), g(38, 9), Toward::Left),
        (g(37, 9), g(1, 9), Toward::Right),
        (g(20, 1), g(20, 16), Toward::Up),
        (g(20, 16), g(20, 1), Toward::Down),
    ] {
        let h = guide_to_far_station(target, [g(10, 13), g(30, 5)], cursor);
        let cam = play(&h).camera.pos;
        assert!(
            !tile_in_view(target, cam, MAP_W),
            "前提: {toward:?} の駅は表示域の外"
        );
        assert!(
            matches!(play(&h).explain.shown.and_then(|s| s.mark), Some(Mark::Edge { toward: t, .. }) if t == toward),
            "{toward:?}"
        );
        let (cx, cy) = tile_center(target, cam);
        let along = match toward {
            Toward::Left | Toward::Right => cy.clamp(MAP_Y + TILE / 2, MAP_Y + MAP_H - TILE / 2),
            Toward::Up | Toward::Down => cx.clamp(MAP_X + TILE / 2, MAP_X + MAP_W - TILE / 2),
        };
        assert_eq!(
            drawn_edge(&h),
            Some((toward, along)),
            "{toward:?}: 縁に、駅のほうを向く絵を、駅の位置に合わせて描く"
        );
        assert_cursor_visible(&h, &format!("{toward:?}"));
    }
}

#[test]
fn when_no_pick_is_visible_an_edge_arrow_points_to_the_nearest_one() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[0] = g(2, 9);
    p[1] = g(16, 15);
    p[2] = g(13, 3);
    let mut h = UiHarness::with_game(p.clone(), false);
    h.frames(1);
    h.move_cursor_to(g(38, 9));
    wait(&mut h, 500);
    h.press(Action::BuildMode);
    h.press(Action::Confirm);
    play_mut(&mut h).fake_directive = Some(fake(3, DirectiveKind::Guide, None));
    h.frames(2);
    let picks = play(&h).explain.picks().to_vec();
    assert_eq!(picks.len(), 3, "前提: 3 駅とも選べる");
    assert!(
        !h.draw_text().contains("Sprite fx_dot"),
        "前提: 丸は 1 つも見えない"
    );
    let cam = play(&h).camera.pos;
    let at = |q: GridPos| {
        tile_center(q, cam)
            .1
            .clamp(MAP_Y + TILE / 2, MAP_Y + MAP_H - TILE / 2)
    };
    let near = p[1];
    assert_eq!(near.chebyshev(g(38, 9)), 22, "いちばん近いのは (16, 15)");
    assert_ne!(
        at(near),
        at(p[0]),
        "前提: いちばん遠い駅とは矢印の高さが違う"
    );
    assert_eq!(
        drawn_edge(&h),
        Some((Toward::Left, at(near))),
        "カーソルにいちばん近い選べる駅のほうへ縁の矢印"
    );
    h.move_cursor_to(g(16, 12));
    wait(&mut h, 500);
    assert!(h.draw_text().contains("Sprite fx_dot"), "丸が見えたら");
    assert_eq!(drawn_edge(&h), None, "縁の矢印は消す");
}

#[test]
fn pick_dots_stay_inside_the_view() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    h.press(Action::BuildMode);
    h.press(Action::Confirm);
    play_mut(&mut h).fake_directive = Some(fake(3, DirectiveKind::Guide, None));
    let mut dots = 0;
    for y in [0, 4, 9, 13, 17] {
        for x in [0, 4, 12, 20, 39] {
            h.move_cursor_to(GridPos { x, y });
            h.frames(10);
            for c in &h.draw_list().cmds {
                if let DrawCmd::Sprite { id, dst, .. } = *c
                    && id == sprite::FX_DOT
                {
                    dots += 1;
                    assert!(
                        dst.x >= MAP_X
                            && dst.x + dst.w as i32 <= MAP_X + MAP_W
                            && dst.y >= MAP_Y
                            && dst.y + dst.h as i32 <= MAP_Y + MAP_H,
                        "HUD や情報の段の上に丸を描かない: {dst:?}"
                    );
                }
            }
        }
    }
    assert!(dots > 0);
}

#[test]
fn static_layers_are_drawn_only_when_they_touch_the_view() {
    let h = harness_with_line();
    let cache = play(&h).map.as_ref().unwrap();
    let drawn = |cam_x: i32, view_w: i32| -> Vec<u8> {
        let mut out = Vec::new();
        let mut b = DrawBuilder::emitting(&mut out, [0; 4]);
        map::draw_static(&mut b, cache, Cam::at(cam_x, 100), view_w);
        out.iter()
            .filter_map(|c| match c {
                DrawCmd::DrawCachedLayer { layer } => Some(layer.0),
                _ => None,
            })
            .collect()
    };
    for (cam_x, view_w, want, why) in [
        (0, MAP_W, &[0][..], "右の層がちょうど 0px かかる"),
        (1, MAP_W, &[0, 1][..], "右の層が 1px かかる"),
        (639, MAP_W, &[0, 1][..], "左の層が 1px かかる"),
        (640, MAP_W, &[1][..], "左の層がちょうど 0px かかる"),
        (
            224,
            MAP_W_PANEL,
            &[0][..],
            "パネル: 右の層がちょうど 0px かかる",
        ),
        (225, MAP_W_PANEL, &[0, 1][..], "パネル: 右の層が 1px かかる"),
        (639, MAP_W_PANEL, &[0, 1][..], "パネル: 左の層が 1px かかる"),
        (
            640,
            MAP_W_PANEL,
            &[1][..],
            "パネル: 左の層がちょうど 0px かかる",
        ),
    ] {
        assert_eq!(drawn(cam_x, view_w), want, "cam.x {cam_x}: {why}");
    }
}

fn arrive(h: &mut UiHarness, station: u32) {
    let now = h.now;
    h.ui.observe(
        now,
        &tm_core::GameEvent::TrainArrived {
            train: tm_core::TrainId(0),
            line: tm_core::LineId(0),
            station: StationId(station),
            riders: tm_core::People(1),
            alighted: tm_core::People(1),
            boarded: tm_core::People(1),
            income: tm_numeric::BigNum::from_u64(5),
            dwell: tm_core::Ticks(1),
        },
        &mut Vec::new(),
    );
}

#[test]
fn trains_dots_and_popups_outside_the_view_are_not_drawn() {
    let mut h = harness_with_line();
    h.frames(120);
    assert!(h.game.as_ref().unwrap().view().trains().len() > 0);
    let line_tiles: Vec<GridPos> = play(&h).map.as_ref().unwrap().paths[0].clone();
    for (cursor, inside) in [(GridPos { x: 38, y: 16 }, false), (plan()[0], true)] {
        h.move_cursor_to(cursor);
        wait(&mut h, 900);
        let cam = play(&h).camera.pos;
        let (sx, sy) = tile_center(plan()[2], cam);
        assert_eq!(
            crate::layout::near_view(sx, sy, MAP_W),
            inside,
            "前提: 駅 2 が表示域のまわりにあるか"
        );
        if !inside {
            assert!(
                line_tiles.iter().all(|&t| {
                    let (x, y) = tile_center(t, cam);
                    !crate::layout::near_view(x, y, MAP_W)
                }),
                "前提: 路線の全体が表示域のまわりより外"
            );
        }
        arrive(&mut h, 2);
        h.frames(1);
        let view = h.game.as_ref().unwrap().view();
        let placed = play(&h)
            .anims
            .popups
            .layout(view, h.now, cam, MAP_W)
            .into_iter()
            .flatten()
            .count();
        let text = h.draw_text();
        assert_eq!(
            placed,
            usize::from(inside),
            "外の駅のポップアップは置かない"
        );
        assert_eq!(text.contains("Sprite '+'12"), inside, "ポップアップ");
        assert_eq!(text.contains("Sprite trn_"), inside, "列車");
    }
}

#[test]
fn the_line_confirm_dialog_band_is_kept_free_for_the_cursor() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(2);
    h.press(Action::BuildMode);
    h.press(Action::Confirm);
    for &s in &[0, 1, 2] {
        h.move_cursor_to(plan()[s]);
        h.press(Action::Confirm);
    }
    h.press(Action::Confirm);
    let (d, _) = dialog_and_cursor(&h);
    let view = h.game.as_ref().unwrap().view();
    let vp = play(&h).viewport(view);
    if d.y == crate::layout::BOX_TOP_Y {
        assert_eq!(
            vp.top,
            d.y + d.h as i32 - MAP_Y + 4,
            "上のダイアログの帯を除く"
        );
    } else {
        assert_eq!(vp.bottom, d.y - MAP_Y - 4, "下のダイアログの帯を除く");
    }
    assert!(vp.soft.0 >= vp.top && vp.soft.1 <= vp.bottom);
}

#[test]
fn a_selected_panel_target_is_shown_together_with_the_highlight() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[0] = g(18, 9);
    p[1] = g(26, 9);
    p[2] = g(12, 12);
    let mut h = UiHarness::from_game(super::cheap_game_on(p.clone(), |_| {}), false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 2]));
    h.frames(2);
    h.move_cursor_to(p[0]);
    h.press(Action::Confirm);
    h.press(Action::MoveRight);
    let PlayMode::Panel(panel) = &play(&h).mode else {
        panic!("パネル")
    };
    assert_eq!(panel.target_station, Some(StationId(0)), "前提: この駅だけ");
    play_mut(&mut h).fake_directive = Some(fake(
        9,
        DirectiveKind::Guide,
        Some(UiAnchor::Station(StationId(1))),
    ));
    wait(&mut h, 800);
    let cam = play(&h).camera.pos;
    assert!(tile_in_view(p[0], cam, MAP_W_PANEL), "選んでいる駅");
    assert!(
        tile_in_view(p[1], cam, MAP_W_PANEL),
        "一緒に見せられる案内の駅も表示域に入れる: {cam:?}"
    );
    let mut c = crate::camera::Camera::default();
    c.jump(p[0], play(&h).viewport(h.game.as_ref().unwrap().view()));
    assert!(
        !tile_in_view(p[1], c.target, MAP_W_PANEL),
        "前提: 選んでいる駅を中央に置くだけでは案内の駅は見えない"
    );
}

#[test]
fn the_explanation_box_does_not_hide_the_cursor() {
    use tm_tutorial::TutorialStageId;
    for pauses in [true, false] {
        let mut h = harness_with_line();
        let mut d = fake(6, DirectiveKind::Explanation, None);
        d.stage = TutorialStageId(6);
        d.pauses_game = pauses;
        play_mut(&mut h).fake_directive = Some(d);
        h.frames(2);
        for y in [9, 11, 13, 15, 16, 17, 0, 1] {
            h.move_cursor_to(GridPos { x: 12, y });
            wait(&mut h, 500);
            let s = play(&h).explain.shown.expect("説明");
            assert!(s.boxed);
            let (x, cy) = tile_screen(cursor(&h), play(&h).camera.pos);
            let c = crate::draw::rect(x, cy, TILE as u32, TILE as u32);
            assert!(
                x >= s.area.x && x + TILE <= s.area.x + s.area.w as i32,
                "前提: カーソルは箱の左右の範囲"
            );
            assert!(
                !overlaps(s.area, c),
                "pauses={pauses} y={y}: 箱 {:?} がカーソル {c:?} を隠す",
                s.area
            );
        }
    }
}

#[test]
fn level_up_arrows_are_counted_among_the_stations_on_screen() {
    let mut h = super::budget::busiest();
    h.move_cursor_to(GridPos { x: 38, y: 6 });
    wait(&mut h, 900);
    let cam = play(&h).camera.pos;
    let view = h.game.as_ref().unwrap().view();
    let shown: Vec<u32> = view
        .stations()
        .filter(|s| s.connected)
        .filter(|s| {
            let (x, y) = tile_center(s.pos, cam);
            crate::layout::near_view(x, y, MAP_W)
        })
        .map(|s| s.id.0)
        .collect();
    assert!(
        view.stations().filter(|s| s.connected).count() > 19,
        "前提: 上限 19 より多い駅が路線にある"
    );
    assert!(
        shown.iter().any(|&i| i >= 19),
        "前提: 番号の大きい駅が見えている: {shown:?}"
    );
    let now = h.now;
    play_mut(&mut h)
        .anims
        .level_up(now, tm_core::UpgradeScope::AllStations);
    h.frames(1);
    let arrows = h
        .draw_text()
        .lines()
        .filter(|l| l.starts_with("Sprite ico_up ") && l.contains(" 20x20 "))
        .count();
    assert_eq!(
        arrows,
        shown.len().min(19),
        "見えている駅の分だけ矢印を出す"
    );
}

/// 箱を印の反対側から動かせない(もう一方は印に重なる)ときは、カメラがカーソルを箱の上へ出す。
#[test]
fn the_camera_lifts_the_cursor_above_a_box_that_cannot_move() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[1] = g(12, 5);
    let mut h = UiHarness::with_game(p, false);
    h.frames(1);
    let mut d = fake(
        6,
        DirectiveKind::Explanation,
        Some(UiAnchor::Station(StationId(1))),
    );
    d.pauses_game = true;
    d.message = tm_assets::txt::TUT_S09_P2;
    d.page_count = 3;
    play_mut(&mut h).fake_directive = Some(d);
    h.frames(2);
    let before = play(&h).camera.pos;
    let (_, y) = tile_screen(g(12, 9), before);
    let low = crate::scene::explain::page_rect(crate::layout::EXPLAIN_X, 2, false);
    assert!(
        y + TILE > low.y,
        "前提: いまのカメラでは (12, 9) は下の箱に隠れる"
    );
    h.move_cursor_to(g(12, 9));
    wait(&mut h, 500);
    assert!(
        y + TILE <= MAP_Y + MAP_H - 2 * TILE,
        "前提: 2 マスの余白は取れていて、余白だけではカメラは動かない"
    );
    assert_ne!(play(&h).camera.pos, before, "カメラが動く");
    let s = play(&h).explain.shown.expect("説明");
    let Some(Mark::Tile(m)) = s.mark else {
        panic!("駅の枠: {:?}", s.mark)
    };
    assert_eq!(s.area, low, "印が上なので箱は下");
    let (x, y) = tile_screen(cursor(&h), play(&h).camera.pos);
    let c = crate::draw::rect(x, y, TILE as u32, TILE as u32);
    assert!(
        !overlaps(s.area, c),
        "カーソルを箱の上へ出す: {:?} / {c:?}",
        s.area
    );
    assert!(!overlaps(s.area, m), "駅も隠さない");
    assert!(m.y >= MAP_Y, "駅の枠も表示域の中");
}
