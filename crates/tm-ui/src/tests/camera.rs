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
            d.y + d.h as i32 - MAP_Y + crate::layout::RING,
            "上のダイアログの帯を除く"
        );
    } else {
        assert_eq!(
            vp.bottom,
            d.y - MAP_Y - crate::layout::RING,
            "下のダイアログの帯を除く"
        );
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
        "前提: 19 より多い駅が路線にある"
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
    assert_eq!(arrows, shown.len(), "見えている駅の分だけ矢印を出す");

    let mut all = super::budget::busiest_on_screen();
    let now = all.now;
    play_mut(&mut all)
        .anims
        .level_up(now, tm_core::UpgradeScope::AllStations);
    all.frames(1);
    let arrows = all
        .draw_text()
        .lines()
        .filter(|l| l.starts_with("Sprite ico_up ") && l.contains(" 20x20 "))
        .count();
    let max = tm_core::BalanceTable::builtin().max_stations() as usize;
    assert_eq!(
        arrows, max,
        "27 駅がすべて見えていれば 27 駅ぶん(上限は駅の最大数)"
    );
    assert_eq!(crate::anim::LEVELUP_TARGETS, max);
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

/// 止める説明の箱の矩形と、カーソルの絵(マスから `RING` はみ出す)が重なっているか。
fn box_hides_cursor(h: &UiHarness) -> Option<(Rect, bool)> {
    let p = play(h);
    let s = p.explain.shown?;
    if !s.boxed || s.under_menu {
        return None;
    }
    let c = crate::layout::tile_ring(cursor(h), p.camera.pos);
    Some((s.area, overlaps(s.area, c)))
}

/// 1 マスずつ下へ動かしても、説明の箱は跳ばず(戻ってこない)、カーソルも隠さない。
/// 箱が動くのは、マップの下の端でカメラがもう下がれず、箱がカーソルを隠すときの 1 回だけ。
#[test]
fn the_explanation_box_stays_put_while_the_cursor_steps() {
    use tm_tutorial::TutorialStageId;
    for pauses in [true, false] {
        for (x, moves) in [(12, 1), (30, 0)] {
            let mut h = harness_with_line();
            let mut d = fake(6, DirectiveKind::Explanation, None);
            d.stage = TutorialStageId(6);
            d.pauses_game = pauses;
            play_mut(&mut h).fake_directive = Some(d);
            h.frames(2);
            h.move_cursor_to(GridPos { x, y: 4 });
            wait(&mut h, 600);
            let mut areas = vec![box_hides_cursor(&h).expect("箱").0];
            let mut moved_at = None;
            for step in 0..13 {
                h.key_down(Action::MoveDown);
                for f in 0..40 {
                    h.frames(1);
                    if f == 0 {
                        h.key_up(Action::MoveDown);
                    }
                    let (area, hidden) = box_hides_cursor(&h).expect("箱");
                    let what = format!("pauses={pauses} x={x} {step} 歩目の {f} フレーム");
                    assert!(!hidden, "{what}: カーソルを隠す");
                    if *areas.last().unwrap() != area {
                        areas.push(area);
                        moved_at = Some((cursor(&h).y, play(&h).camera.target.y));
                    }
                }
            }
            assert_eq!(cursor(&h).y, 17);
            assert_eq!(
                areas.len() - 1,
                moves,
                "pauses={pauses} x={x}: 箱の動き {areas:?}"
            );
            if let Some((_, cam_y)) = moved_at {
                assert_eq!(cam_y, WORLD_H - MAP_H, "動くのはカメラが下の端に着いてから");
            }
        }
    }
}

/// 本物のチュートリアルの #1 を読みながら下へ 3 マス動かしても、箱は跳ばない(シード 1〜30)。
#[test]
fn the_first_explanation_box_does_not_jump_on_any_seed() {
    for seed in 1..=30 {
        let mut h = UiHarness::with_seed(seed, true, tm_platform_api::MonoTime(0));
        h.frames(30);
        let (first, _) = box_hides_cursor(&h).expect("#1 の箱");
        for _ in 0..3 {
            h.key_down(Action::MoveDown);
            h.frames(1);
            h.key_up(Action::MoveDown);
            for f in 0..20 {
                let (area, hidden) = box_hides_cursor(&h).expect("箱");
                assert_eq!(area, first, "シード {seed} {f}: 箱が跳ぶ");
                assert!(!hidden, "シード {seed} {f}: カーソルを隠す");
                h.frames(1);
            }
        }
    }
}

/// 始めのカメラは、説明を出しても始めから動かない。始めの 1 つ目の駅はいつもすべて見える(入りきらないシードでも)。
#[test]
fn the_first_screen_holds_still_and_shows_the_first_station() {
    for seed in 1..=40 {
        for explain in [false, true] {
            let mut h = UiHarness::with_seed(seed, explain, tm_platform_api::MonoTime(0));
            h.frames(1);
            let first = h
                .game
                .as_ref()
                .unwrap()
                .view()
                .stations()
                .next()
                .unwrap()
                .pos;
            let cam = play(&h).camera.pos;
            assert!(tile_in_view(first, cam, MAP_W), "seed {seed}: 1 つ目の駅");
            for f in 0..120 {
                h.frames(1);
                assert_eq!(
                    play(&h).camera.pos,
                    cam,
                    "seed {seed} explain={explain} {f}: 動く"
                );
            }
        }
    }
}

/// 最初の説明の箱が始めの駅を隠すなら、隠さずに済むときはカメラを(動きを見せずに)少しずらす。
#[test]
fn the_first_box_moves_the_camera_off_the_starting_stations_when_it_can() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[0] = g(14, 5);
    p[1] = g(20, 7);
    p[2] = g(24, 10);
    let mut h = UiHarness::with_game(p.clone(), true);
    h.frames(1);
    let s = play(&h).explain.shown.expect("#1");
    assert!(s.boxed);
    let cam = play(&h).camera.pos;
    for &q in &p[..3] {
        assert!(tile_in_view(q, cam, MAP_W), "{q:?} が見える");
        assert!(
            !overlaps(s.area, crate::layout::tile_ring(q, cam)),
            "{q:?} を箱で隠さない: {:?}",
            s.area
        );
    }
    let (_, y) = tile_screen(p[2], Cam::at(cam.x, cam.y - 24));
    assert!(
        y + TILE + crate::layout::RING > s.area.y,
        "前提: 外接矩形の中心のままなら (24, 10) が箱に隠れる"
    );
    h.frames(60);
    assert_eq!(play(&h).camera.pos, cam, "置き直した後は動かない");
}

/// 「作る」の丸は、駅の上が表示域に入らなければ駅の下に描く。
#[test]
fn pick_dots_go_below_a_station_at_the_top_of_the_view() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    h.press(Action::BuildMode);
    h.press(Action::Confirm);
    play_mut(&mut h).fake_directive = Some(fake(3, DirectiveKind::Guide, None));
    h.frames(2);
    let top = plan()[0];
    assert!(play(&h).explain.picks().contains(&top));
    let render = |h: &mut UiHarness, y: i32| {
        let cam = Cam::at(play(h).camera.pos.x, y);
        play_mut(h).camera.pos = cam;
        let mut out = tm_platform_api::DrawList::default();
        let view = h.game.as_ref().map(tm_core::Game::view);
        h.ui.render(view, h.tutorial.as_ref(), &mut out);
        (cam, crate::testing::draw_text(&out))
    };
    let d = TILE / 2 + 4 * crate::layout::MAP_SCALE;
    let (cam, text) = render(&mut h, top.y * TILE);
    let (cx, cy) = tile_center(top, cam);
    assert_eq!(cy - TILE / 2, MAP_Y, "前提: 駅は表示域のいちばん上の行");
    assert!(
        text.contains(&format!("Sprite fx_dot {},{} ", cx - 8, cy + d - 8)),
        "上に入らなければ駅の下に丸"
    );
    let (cam, text) = render(&mut h, top.y * TILE - 2 * TILE);
    let (cx, cy) = tile_center(top, cam);
    assert!(
        text.contains(&format!("Sprite fx_dot {},{} ", cx - 8, cy - d - 8)),
        "ふつうは駅の上に丸"
    );
}

/// 待っている人の点は、駅で待てる人数に対する割合で数える(M10: 切り捨てで 1〜9 個、満員のときだけ 10 個。
/// 待てる人数 40 人なら 4 人ごとに 1 個増え、5〜7 人は 1 個)。切り上げと切り捨てで数が違う人数も確かめる。
#[test]
fn waiting_dots_count_the_share_of_each_stations_capacity() {
    use tm_core::{ParamKey, ValueFormula};
    let game = super::cheap_game_on(plan(), |s| {
        for p in &mut s.params {
            if p.key == ParamKey::S3StationCap {
                p.value = ValueFormula::Geometric { v0: 40, ratio: 2 };
            }
        }
    });
    let mut h = UiHarness::from_game(game, false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    h.move_cursor_to(plan()[1]);
    let (mut checked, mut split) = (0, Vec::new());
    for _ in 0..120 {
        h.frames(30);
        let view = h.game.as_ref().unwrap().view();
        let cam = play(&h).camera.pos;
        let text = h.draw_text();
        for st in view.stations() {
            let n = st.waiting.0;
            if n < 2 || !tile_in_view(st.pos, cam, MAP_W) {
                continue;
            }
            assert_eq!(st.capacity.0, 40, "前提: 待てる人数 40");
            let k = if n >= 40 { 10 } else { (n / 4).max(1) };
            let (cx, cy) = tile_center(st.pos, cam);
            let at = format!(" {},{} ", cx + 14, cy - 16);
            let drawn: Vec<&str> = text
                .lines()
                .filter(|l| l.starts_with("Sprite pax_dots_") && l.contains(&at))
                .collect();
            assert_eq!(drawn.len(), 1, "{:?}", st.pos);
            assert!(
                drawn[0].starts_with(&format!("Sprite pax_dots_{k} ")),
                "{n} 人 / 40 人は {k} 個: {}",
                drawn[0]
            );
            checked += 1;
            if n > 4 && n % 4 != 0 && n < 40 {
                split.push(n);
            }
        }
        if checked >= 3 && !split.is_empty() {
            break;
        }
    }
    assert!(checked >= 3, "2 人以上待っている駅を確かめた: {checked}");
    assert!(
        !split.is_empty(),
        "切り上げなら 1 個多くなる人数(4 の倍数でない 5 人以上)を確かめた"
    );
}

/// 新しい説明の箱は、カメラが止まる先で置き場所を決める(「印が下半分なら上」も止まる先の印で見る)。
/// 動いている途中で新しいページが出ても、途中の位置で下半分に見える印に合わせて上に置かない。
#[test]
fn a_new_box_is_placed_for_where_the_camera_stops() {
    let mut h = harness_with_line();
    let station = plan()[2];
    h.move_cursor_to(GridPos { x: 4, y: 10 });
    wait(&mut h, 600);
    let t = Cam::at(64, 220);
    play_mut(&mut h).camera = crate::camera::Camera {
        pos: Cam::at(64, 120),
        target: t,
    };
    play_mut(&mut h).fake_directive = Some(fake(
        6,
        DirectiveKind::Explanation,
        Some(UiAnchor::Station(StationId(2))),
    ));
    h.frames(1);
    assert_eq!(play(&h).camera.target, t, "前提: 止まる先は変わらない");
    let moving = Cam::at(64, 145);
    let low_half = |cam: Cam| 2 * tile_center(station, cam).1 >= crate::layout::SCREEN_H;
    assert!(
        !low_half(t) && low_half(moving),
        "前提: 印は止まる先では上半分、1 フレーム進んだ途中の位置では下半分"
    );
    let s = play(&h).explain.shown.expect("説明");
    let bottom = |r: Rect| r.y + r.h as i32;
    assert_eq!(
        bottom(s.area),
        crate::layout::BOX_BOTTOM,
        "止まる先の印が上半分なので下"
    );
    let first = s.area;
    for f in 0..60 {
        h.frames(1);
        let s = play(&h).explain.shown.expect("説明");
        assert_eq!(s.area, first, "{f}: 箱は動かない");
    }
    assert_eq!(play(&h).camera.pos, t);
    let s = play(&h).explain.shown.expect("説明");
    assert_eq!(
        s.mark,
        Some(Mark::Tile(crate::layout::tile_ring(station, t)))
    );
    assert!(
        !overlaps(s.area, crate::layout::tile_ring(station, t)),
        "印を隠さない"
    );
}

/// #2「駅を選んでみよう」では、案内の駅(1 つ目の駅)を表示域に入れて枠で示す。縁の矢印だけにしない。
/// 箱を閉じた後の案内でも同じ(シード 1〜100)。
#[test]
fn the_second_stage_frames_the_guided_station_on_every_seed() {
    use crate::layout::tile_ring;
    use tm_tutorial::TutorialStageId;
    for seed in 1..=100 {
        let mut h = UiHarness::with_seed(seed, true, tm_platform_api::MonoTime(0));
        h.frames(1);
        let first = h
            .game
            .as_ref()
            .unwrap()
            .view()
            .stations()
            .next()
            .unwrap()
            .pos;
        for guard in 0.. {
            let s = play(&h).explain.shown.expect("説明");
            if s.directive.stage != TutorialStageId(1) {
                break;
            }
            assert!(guard < 10, "seed {seed}: #1 が終わらない");
            wait(&mut h, 320);
            h.press(Action::Confirm);
        }
        for kind in [DirectiveKind::Explanation, DirectiveKind::Guide] {
            // カメラは案内の駅へ寄るとき、行って戻らない(軸ごとに向きが変わらない)
            let (mut last, mut dir) = (play(&h).camera.target, (0, 0));
            for f in 0..36 {
                h.frames(1);
                let t = play(&h).camera.target;
                let d = ((t.x - last.x).signum(), (t.y - last.y).signum());
                for (now, was) in [(d.0, &mut dir.0), (d.1, &mut dir.1)] {
                    assert!(
                        now == 0 || *was == 0 || now == *was,
                        "seed {seed} {kind:?} {f}: カメラが行って戻る"
                    );
                    if now != 0 {
                        *was = now;
                    }
                }
                last = t;
            }
            let s = play(&h).explain.shown.expect("#2");
            assert_eq!(
                (s.directive.stage, s.directive.kind),
                (TutorialStageId(2), kind)
            );
            let what = format!("seed {seed} {kind:?}");
            let cam = play(&h).camera.pos;
            let ring = tile_ring(first, cam);
            assert_eq!(s.mark, Some(Mark::Tile(ring)), "{what}: 案内の駅に枠");
            assert!(
                ring.y >= MAP_Y && ring.y + ring.h as i32 <= MAP_Y + MAP_H,
                "{what}: 枠が表示域に収まる {ring:?}"
            );
            assert!(
                h.draw_text()
                    .contains(&super::sprite_at("ui_sel_tl", ring.x, ring.y)),
                "{what}: 枠を描く"
            );
            assert_eq!(drawn_edge(&h), None, "{what}: 縁の矢印は出さない");
            if s.boxed {
                assert!(!overlaps(s.area, ring), "{what}: 箱で隠さない");
                assert!(
                    !overlaps(s.area, tile_ring(cursor(&h), cam)),
                    "{what}: カーソルも隠さない"
                );
                h.press(Action::Confirm);
            }
        }
    }
}

/// 始めの説明の箱が駅を隠し、縦に動かしても出せないときは、横に(3 マスより多くても、動きを見せずに)ずらして出す。
#[test]
fn the_first_box_moves_the_camera_sideways_off_the_starting_stations() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[0] = g(17, 8);
    p[1] = g(20, 4);
    p[2] = g(14, 12);
    let mut h = UiHarness::with_game(p.clone(), true);
    h.frames(1);
    let s = play(&h).explain.shown.expect("#1");
    assert!(s.boxed);
    let cam = play(&h).camera.pos;
    let centred = Cam::at(
        (14 + 20 + 1) * TILE / 2 - MAP_W / 2,
        (4 + 12 + 1) * TILE / 2 - MAP_H / 2,
    );
    let ring = |q: GridPos, c: Cam| crate::layout::tile_ring(q, c);
    assert!(
        overlaps(s.area, ring(p[2], centred)),
        "前提: 外接矩形の中心のままなら (14, 12) が箱に隠れる"
    );
    assert_eq!(
        (
            tile_screen(p[1], centred).1,
            tile_screen(p[2], centred).1 + TILE
        ),
        (MAP_Y, MAP_Y + MAP_H),
        "前提: 上と下の駅が表示域の上下の端にあり、縦には動かせない"
    );
    assert_eq!(cam.y, centred.y, "縦は動かさない");
    assert!(cam.x - centred.x > 3 * TILE, "横に 3 マスより多く: {cam:?}");
    for &q in &p[..3] {
        assert!(tile_in_view(q, cam, MAP_W), "{q:?} が見える");
        assert!(!overlaps(s.area, ring(q, cam)), "{q:?} を箱で隠さない");
    }
    let (x, _) = tile_screen(cursor(&h), cam);
    assert!(
        x - MAP_X >= 2 * TILE && x - MAP_X + TILE <= MAP_W - 2 * TILE,
        "カーソルの 2 マスの余白は保つ"
    );
    h.frames(60);
    assert_eq!(play(&h).camera.pos, cam, "置き直した後は動かない");
}

/// 始め以外のページでも、新しい箱が駅を隠し、縦に動かしても出せないときは、カメラを横に(3 マスまで)動かして出す。
#[test]
fn a_new_box_moves_the_camera_sideways_off_a_station() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[0] = g(14, 3);
    p[1] = g(9, 11);
    p[2] = g(17, 7);
    let mut h = UiHarness::with_game(p.clone(), false);
    h.frames(1);
    h.move_cursor_to(g(20, 7));
    wait(&mut h, 600);
    let t = Cam::at(160, 96);
    play_mut(&mut h).camera = crate::camera::Camera { pos: t, target: t };
    h.frames(1);
    assert_eq!(
        play(&h).camera.target,
        t,
        "前提: カーソルを追っても動かない位置"
    );
    assert_eq!(
        (tile_screen(p[0], t).1, tile_screen(p[1], t).1 + TILE),
        (MAP_Y, MAP_Y + MAP_H),
        "前提: 上と下の駅が表示域の上下の端にあり、縦には動かせない"
    );
    play_mut(&mut h).fake_directive = Some(fake(6, DirectiveKind::Explanation, None));
    h.frames(1);
    let s = play(&h).explain.shown.expect("説明");
    let ring = |q: GridPos, c: Cam| crate::layout::tile_ring(q, c);
    assert!(
        overlaps(s.area, ring(p[1], t)),
        "前提: 動かさなければ (9, 11) が箱に隠れる"
    );
    assert_eq!(
        play(&h).camera.target,
        Cam::at(206, 96),
        "右へ 46px(1 マスより多く)"
    );
    wait(&mut h, 600);
    let cam = play(&h).camera.pos;
    assert_eq!(cam, Cam::at(206, 96));
    let s = play(&h).explain.shown.expect("説明");
    for &q in &p[..3] {
        assert!(tile_in_view(q, cam, MAP_W), "{q:?} が見える");
        assert!(!overlaps(s.area, ring(q, cam)), "{q:?} を箱で隠さない");
    }
}

/// ゲームの画面を始めたフレームに highlight のある説明が出ていれば、案内の対象をカーソルと一緒に、動きを見せずに見せる。
#[test]
fn a_highlight_on_the_first_frame_is_shown_without_moving_the_camera() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[0] = g(5, 9);
    p[1] = g(20, 9);
    p[2] = g(34, 9);
    let mut h = UiHarness::with_game(p.clone(), false);
    play_mut(&mut h).fake_directive = Some(fake(
        6,
        DirectiveKind::Explanation,
        Some(UiAnchor::Station(StationId(2))),
    ));
    h.frames(1);
    let cam = play(&h).camera.pos;
    assert_eq!(cursor(&h), g(19, 9), "前提: 中央の駅の左");
    let s = play(&h).explain.shown.expect("説明");
    assert_eq!(
        s.mark,
        Some(Mark::Tile(crate::layout::tile_ring(p[2], cam))),
        "始めから案内の駅に枠"
    );
    assert!(tile_in_view(cursor(&h), cam, MAP_W), "カーソルも見える");
    for f in 0..60 {
        h.frames(1);
        assert_eq!(play(&h).camera.pos, cam, "{f}: 動かない");
    }
}

/// 新しい箱が駅を隠しても、出すために動かすと案内の駅が余白から出るなら、カメラは動かさない(動いて戻らない)。
#[test]
fn a_new_box_does_not_move_the_camera_off_the_guided_station() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[0] = g(3, 6);
    p[1] = g(11, 3);
    p[2] = g(12, 8);
    let mut h = UiHarness::with_game(p.clone(), false);
    h.frames(1);
    h.move_cursor_to(g(14, 6));
    wait(&mut h, 600);
    let t = Cam::at(64, 76);
    play_mut(&mut h).camera = crate::camera::Camera { pos: t, target: t };
    h.frames(1);
    assert_eq!(
        play(&h).camera.target,
        t,
        "前提: カーソルを追っても動かない位置"
    );
    let mut d = fake(
        6,
        DirectiveKind::Explanation,
        Some(UiAnchor::Station(StationId(1))),
    );
    d.message = tm_assets::txt::TUT_S09_P2;
    play_mut(&mut h).fake_directive = Some(d);
    for f in 0..40 {
        h.frames(1);
        assert_eq!(play(&h).camera.target, t, "{f}: 動かない");
    }
    let s = play(&h).explain.shown.expect("説明");
    let ring = |q: GridPos| crate::layout::tile_ring(q, t);
    assert_eq!(s.mark, Some(Mark::Tile(ring(p[1]))), "案内の駅に枠");
    assert_eq!(
        tile_screen(p[1], t).1 - MAP_Y,
        20,
        "前提: 案内の駅は上から 20px"
    );
    assert!(
        overlaps(s.area, ring(p[2])),
        "前提: (12, 8) は箱に隠れる(出すには下へ 10px 動かし、案内の駅が半マスの余白から出る)"
    );
}

/// 新しい箱の上下を選ぶとき、隠した駅をカメラで出せるかは案内の駅を余白に残したまま見る。
/// 上(印が下半分なので先に試す側)は、駅を出すと案内の駅が半マスの余白から出るので、出せる下を選ぶ。
#[test]
fn a_new_box_counts_only_fixes_that_keep_the_guided_station() {
    let g = |x, y| GridPos { x, y };
    let mut p = plan();
    p[0] = g(10, 3);
    p[1] = g(7, 6);
    p[2] = g(18, 8);
    let mut h = UiHarness::with_game(p.clone(), false);
    h.frames(1);
    h.move_cursor_to(g(13, 5));
    wait(&mut h, 600);
    let t = Cam::at(16, 16);
    play_mut(&mut h).camera = crate::camera::Camera { pos: t, target: t };
    h.frames(1);
    assert_eq!(
        play(&h).camera.target,
        t,
        "前提: カーソルを追っても動かない位置"
    );
    assert_eq!(
        tile_screen(p[2], t).1 + TILE,
        MAP_Y + MAP_H - 16,
        "前提: 案内の駅は下から半マス"
    );
    let mut d = fake(
        6,
        DirectiveKind::Explanation,
        Some(UiAnchor::Station(StationId(2))),
    );
    d.message = tm_assets::txt::TUT_S09_P2;
    play_mut(&mut h).fake_directive = Some(d);
    h.frames(1);
    let s = play(&h).explain.shown.expect("説明");
    let ring = |q: GridPos, c: Cam| crate::layout::tile_ring(q, c);
    let high = crate::scene::explain::page_rect(crate::layout::EXPLAIN_X, 2, true);
    assert!(
        overlaps(high, ring(p[0], t)),
        "前提: 上の箱は (10, 3) を隠す"
    );
    assert!(
        overlaps(s.area, ring(p[1], t)),
        "前提: 下の箱は (7, 6) を隠す"
    );
    assert_eq!(s.area.y + s.area.h as i32, crate::layout::BOX_BOTTOM, "下");
    wait(&mut h, 600);
    let cam = play(&h).camera.pos;
    assert_eq!(cam, Cam::at(16, 22), "下へ 6px 動かして (7, 6) を出す");
    let s = play(&h).explain.shown.expect("説明");
    assert_eq!(s.mark, Some(Mark::Tile(ring(p[2], cam))));
    for &q in &p[..3] {
        assert!(!overlaps(s.area, ring(q, cam)), "{q:?} を箱で隠さない");
    }
}
