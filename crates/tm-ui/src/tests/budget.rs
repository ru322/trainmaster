//! 描画要素の予算(13 §9、§10.2): 最多の場面で 1500 以下、超えたら決めた順に省く。

use tm_core::{GameEvent, LineId, StationId};
use tm_numeric::BigNum;
use tm_platform_api::DrawList;

use tm_input::Action;

use super::screens::play_mut;
use super::{buy_all, cheap_fast_game, cheap_game_on, create_line};
use crate::draw::{Shed, UI_DRAW_BUDGET};
use crate::scene::PlayMode;
use crate::scene::build::Step;
use crate::testing::UiHarness;

/// 27 駅を、マップの中央の 16 × 9 マス(cols 11〜26、rows 5〜13)に置く。始めの 3 駅の外接矩形が全体の外接矩形と同じなので、
/// 始めのカメラ(その中心)で全部が表示域に入る。
fn dense_plan() -> Vec<tm_core::GridPos> {
    let mut v: Vec<tm_core::GridPos> = (0..27)
        .map(|k| tm_core::GridPos {
            x: 11 + 3 * (k % 6),
            y: 5 + 2 * (k / 6),
        })
        .collect();
    v.swap(1, 23);
    v.swap(2, 26);
    v
}

/// 駅 27、路線 6、列車たくさん、ポップアップ 12、帯とトーストと紙ふぶきのある場面(駅はマップ全体に散らばる)。
pub(super) fn busiest() -> UiHarness {
    busiest_with(cheap_fast_game(), None)
}

/// `busiest` の駅をすべて表示域の中に置いた場面。
pub(super) fn busiest_on_screen() -> UiHarness {
    busiest_with(dense_game(), None)
}

fn dense_game() -> tm_core::Game {
    cheap_game_on(dense_plan(), |s| s.company.gauge_num = 1)
}

/// 重い場面に足す下書き。
#[derive(Clone, Copy, Debug)]
enum Draft {
    /// のばす: はしの駅を選ぶ(全部のはしに枠)。
    PickEnd,
    /// のばす: つなぐ駅まで点線の完成図。
    PickNew,
}

/// のばすの下書きにする(はしを選び、いちばん遠い駅へカーソル)。
fn start_draft(h: &mut UiHarness, d: Draft) {
    h.press(Action::BuildMode);
    let PlayMode::Build(b) = &crate::tests::screens::play(h).mode else {
        panic!("ろせんモード")
    };
    if matches!(b.step, Step::Choose { sel: 0 }) {
        h.press(Action::MoveDown);
    }
    h.press(Action::Confirm);
    let view = h.game.as_ref().unwrap().view();
    let (_, end) = view.line_ends(LineId(0))[0];
    let from = view.station(end).unwrap().pos;
    let far = view
        .stations()
        .filter(|s| !view.line(LineId(0)).unwrap().stations.contains(&s.id))
        .max_by_key(|s| s.pos.chebyshev(from))
        .map(|s| s.pos);
    if let Draft::PickNew = d {
        h.move_cursor_to(from);
        h.press(Action::Confirm);
        if let Some(p) = far {
            h.move_cursor_to(p);
        }
    }
    h.frames(2);
}

/// 光(完成した路線の全部)と下書きを足した、いちばん重い場面。
fn heaviest(game: tm_core::Game, d: Draft) -> UiHarness {
    busiest_with(game, Some(d))
}

fn busiest_with(game: tm_core::Game, draft: Option<Draft>) -> UiHarness {
    let mut h = UiHarness::from_game(game, false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    for _ in 0..40 {
        buy_all(&mut h);
        h.frames(2);
    }
    let lines: [&[u32]; 5] = [
        &[3, 4, 5, 19, 20],
        &[6, 7, 8, 21, 22],
        &[9, 10, 11, 0, 23],
        &[12, 13, 14, 1, 24],
        &[15, 16, 17, 18, 25, 26],
    ];
    for l in lines {
        let cmd = create_line(l);
        let g = h.game.as_mut().unwrap();
        if g.validate(&cmd).is_ok() {
            g.enqueue(cmd);
        }
        h.frames(1);
    }
    for _ in 0..20 {
        buy_all(&mut h);
        h.frames(30);
    }
    if let Some(d) = draft {
        start_draft(&mut h, d);
    }
    let now = h.now;
    let n = h.game.as_ref().unwrap().view().stations().len() as u32;
    for i in 0..n {
        h.ui.observe(
            now,
            &GameEvent::TrainArrived {
                train: tm_core::TrainId(0),
                line: LineId(0),
                station: StationId(i),
                riders: tm_core::People(1),
                alighted: tm_core::People(1),
                boarded: tm_core::People(1),
                income: BigNum::from_u64(123_456_789),
                dwell: tm_core::Ticks(1),
            },
            &mut Vec::new(),
        );
    }
    for ev in [
        GameEvent::CompanyLevelUp {
            new_level: tm_core::CompanyLevel(24),
            all_features_unlocked: true,
        },
        GameEvent::MoneyUnitReached { unit: 17 },
        GameEvent::GoalReached {
            total_earned: BigNum::ONE,
        },
    ] {
        h.ui.observe(now, &ev, &mut Vec::new());
    }
    h.ui.observe(
        now,
        &GameEvent::FeatureUnlocked {
            feature: tm_core::Feature::all_in_order().next().unwrap(),
            company_level: tm_core::CompanyLevel(24),
        },
        &mut Vec::new(),
    );
    if draft.is_some() {
        let lines = h.game.as_ref().unwrap().view().lines().len();
        for i in 0..lines {
            play_mut(&mut h).anims.line_done(now, LineId::from_index(i));
        }
    }
    h
}

fn render(h: &UiHarness) -> (DrawList, crate::draw::DrawStats) {
    let mut out = DrawList::default();
    let view = h.game.as_ref().map(tm_core::Game::view);
    let st = h.ui.render_stats(view, h.tutorial.as_ref(), &mut out);
    (out, st)
}

#[test]
fn heaviest_scenes_fit_the_budget() {
    let all = busiest_on_screen();
    let view = all.game.as_ref().unwrap().view();
    assert_eq!(view.stations().len(), 27, "駅 27");
    assert!(view.lines().len() >= 5, "路線 {}", view.lines().len());
    let cam = crate::tests::screens::play(&all).camera.pos;
    assert!(
        view.stations()
            .all(|s| crate::layout::tile_in_view(s.pos, cam, crate::layout::MAP_W)),
        "前提: 27 駅すべてが表示域の中"
    );
    let mut most = 0;
    for (name, h) in [
        ("全部が見える", busiest_on_screen()),
        (
            "全部が見える + 光 + はしを選ぶ",
            heaviest(dense_game(), Draft::PickEnd),
        ),
        (
            "全部が見える + 光 + 点線の完成図",
            heaviest(dense_game(), Draft::PickNew),
        ),
        (
            "散らばる + 光 + はしを選ぶ",
            heaviest(cheap_fast_game(), Draft::PickEnd),
        ),
        (
            "散らばる + 光 + 点線の完成図",
            heaviest(cheap_fast_game(), Draft::PickNew),
        ),
    ] {
        assert!(
            matches!(crate::tests::screens::play(&h).mode, PlayMode::Build(_))
                || name == "全部が見える"
        );
        let (out, st) = render(&h);
        assert_eq!(st.dropped, [0; 4], "{name}");
        assert!(st.counted <= UI_DRAW_BUDGET, "{name}: {} 個", st.counted);
        eprintln!(
            "{name}: 描画要素 {} 個、列車 {}、層の中身 {} 個",
            st.counted,
            h.game.as_ref().unwrap().view().trains().len(),
            out.cmds.len() - st.counted
        );
        most = most.max(st.counted);
    }
    eprintln!("いちばん重い場面: {most} 個 / {UI_DRAW_BUDGET}");
}

/// 重い場面を、カメラを 1 マスずつマップ全体に動かして描く。描画要素(キャッシュ層は 1 個)は 1500 以下、
/// 層の中身を含めた描画コマンドの列は 4096 以下(tm-app の soak の容量の上限)。
#[test]
fn heaviest_scenes_fit_the_budget_at_every_camera_position() {
    use crate::layout::{Cam, MAP_H, MAP_W, TILE, WORLD_H, WORLD_W};
    for (name, mut h) in [
        (
            "散らばる + 光 + はしを選ぶ",
            heaviest(cheap_fast_game(), Draft::PickEnd),
        ),
        (
            "散らばる + 光 + 点線の完成図",
            heaviest(cheap_fast_game(), Draft::PickNew),
        ),
        (
            "全部が見える + 光 + はしを選ぶ",
            heaviest(dense_game(), Draft::PickEnd),
        ),
    ] {
        let (mut most, mut longest, mut over_2000) = (0, 0, 0);
        let mut n = 0;
        for y in (0..=WORLD_H - MAP_H).step_by(TILE as usize) {
            for x in (0..=WORLD_W - MAP_W).step_by(TILE as usize) {
                play_mut(&mut h).camera.pos = Cam::at(x, y);
                let (out, st) = render(&h);
                assert!(
                    st.counted <= UI_DRAW_BUDGET,
                    "{name} ({x}, {y}): {} 個",
                    st.counted
                );
                assert_eq!(st.dropped, [0; 4], "{name} ({x}, {y})");
                assert!(
                    out.cmds.len() <= 4096,
                    "{name} ({x}, {y}): 列 {}",
                    out.cmds.len()
                );
                most = most.max(st.counted);
                longest = longest.max(out.cmds.len());
                over_2000 += usize::from(out.cmds.len() > 2000);
                n += 1;
            }
        }
        eprintln!(
            "{name}: カメラ {n} 位置、描画要素の最大 {most} / {UI_DRAW_BUDGET}、層の中身を含む列の最大 {longest}(2000 を超えたのは {over_2000} 位置)"
        );
    }
}

#[test]
fn over_budget_drops_popups_first() {
    let mut h = busiest_on_screen();
    let (_, full) = render(&h);
    assert!(h.ui.play().unwrap().anims.popups_len() == crate::anim::POPUP_MAX);
    h.set_draw_budget(full.wanted - 30);
    let (out, st) = render(&h);
    assert!(st.counted <= full.wanted - 30);
    assert_eq!(out.cmds.len() - st.counted, full_layer(&h));
    assert!(st.dropped[Shed::Popup as usize] > 0);
    assert_eq!(st.dropped[Shed::PeopleTag as usize], 0);
    assert_eq!(st.dropped[Shed::Confetti as usize], 0);
    assert_eq!(st.dropped[Shed::Toast as usize], 0);

    h.set_draw_budget(full.wanted - 200);
    let (_, st) = render(&h);
    assert!(st.counted <= full.wanted - 200);
    assert_eq!(
        st.dropped[Shed::Popup as usize],
        crate::anim::POPUP_MAX as u32
    );
    assert!(st.dropped[Shed::PeopleTag as usize] > 0 || st.dropped[Shed::Confetti as usize] > 0);
}

/// 表示域にかかる層の中身と Begin・End の数(予算に数えない分)。
fn full_layer(h: &UiHarness) -> usize {
    use crate::layout::{LAYER_W, MAP_W, MAP_X};
    let p = h.ui.play().unwrap();
    let cam = p.camera.pos;
    let map = p.map.as_ref().unwrap();
    map.chunks
        .iter()
        .enumerate()
        .filter(|(k, _)| {
            let x = MAP_X + *k as i32 * LAYER_W - cam.x;
            x < MAP_X + MAP_W && x + LAYER_W > MAP_X
        })
        .map(|(_, c)| c.len() + 2)
        .sum()
}

#[test]
fn people_tags_go_before_confetti() {
    let mut h = busiest_on_screen();
    let (_, full) = render(&h);
    let t = |k: Shed| full.totals[k as usize] as usize;
    assert!(t(Shed::PeopleTag) > 0 && t(Shed::Confetti) > 0 && t(Shed::Toast) > 0);
    h.set_draw_budget(full.wanted - t(Shed::Popup) - 1);
    let (_, st) = render(&h);
    assert_eq!(
        st.dropped[Shed::Popup as usize],
        crate::anim::POPUP_MAX as u32
    );
    assert!(st.dropped[Shed::PeopleTag as usize] >= 1);
    assert_eq!(st.dropped[Shed::Confetti as usize], 0, "札が先");
    assert_eq!(st.dropped[Shed::Toast as usize], 0);
    h.set_draw_budget(full.wanted - t(Shed::Popup) - t(Shed::PeopleTag) - 1);
    let (_, st) = render(&h);
    assert!(st.dropped[Shed::Confetti as usize] >= 1);
    assert_eq!(st.dropped[Shed::Toast as usize], 0, "トーストは最後");
    assert!(st.counted < full.wanted - t(Shed::Popup) - t(Shed::PeopleTag));
}
