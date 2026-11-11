//! 続きから(13 §6.8 UI-D-37、§10.3 の「続きから」の行)。

use super::*;
use crate::save::{SavedLatches, SavedSide, SavedStep, TutorialSave};

fn reload(t: &Tutorial, show: bool, g: &Game) -> Tutorial {
    let mut r = Tutorial::restore(t.save(), show);
    r.observe_view(g.view());
    r
}

fn to_guide_of_3(t: &mut Tutorial) {
    ack_all(t);
    t.observe_ui(&on_station(0));
    ack_all(t);
}

#[test]
fn restore_at_a_guide_continues_where_it_was() {
    let g = game();
    let mut t = Tutorial::new(true);
    to_guide_of_3(&mut t);
    t.observe_view(g.view());
    let r = reload(&t, true, &g);
    assert_eq!(r.directive(), t.directive());
    assert_eq!(shape(&r), Some((3, G, 0, 1, false)));
    assert_eq!(permitted(&r), permitted(&t));
    assert_eq!(r.shown_messages(), t.shown_messages());
    assert_eq!(r.shown_stages(), t.shown_stages());
    assert_eq!(r.save(), t.save());
}

#[test]
fn restore_inside_an_explanation_starts_from_page_1() {
    let g = game();
    let mut t = Tutorial::new(true);
    ack_all(&mut t);
    t.observe_ui(&on_station(0));
    t.observe_ui(&ACK);
    t.observe_ui(&ACK);
    assert_eq!(shape(&t), Some((3, E, 2, 3, true)));
    let r = Tutorial::restore(t.save(), true);
    assert_eq!(r.directive(), None, "directive は observe_view で作る");
    assert!(
        permitted(&r).iter().all(|&p| !p),
        "作る前も止める説明の扱い"
    );
    let r = reload(&t, true, &g);
    assert_eq!(shape(&r), Some((3, E, 0, 3, true)));
    assert_eq!(d(&r).message, txt::TUT_S03_P1);
    assert_eq!(r.shown_messages(), t.shown_messages());
}

#[test]
fn restore_while_15_is_showing_shows_it_again_from_page_1() {
    let g = game();
    let mut t = Tutorial::new(true);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    t.observe_ui(&ACK);
    assert_eq!(shape(&t), Some((15, E, 1, 3, false)));
    let mut r = reload(&t, true, &g);
    assert_eq!(shape(&r), Some((1, E, 0, 2, true)), "せってい は閉じている");
    r.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    assert_eq!(shape(&r), Some((15, E, 0, 3, false)));

    t.observe_ui(&UiEvent::MenuClosed(SETTINGS));
    let mut r = reload(&t, true, &g);
    r.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    assert_eq!(d(&r).stage.0, 1, "見終わった #15 は出さない");
}

#[test]
fn restore_drops_the_fanfare_wait() {
    let g = game();
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(8));
    t.observe_game(&level(1));
    t.observe_view(g.view());
    assert!(t.fanfare_pending());
    assert_eq!(t.directive(), None);
    let r = reload(&t, true, &g);
    assert!(!r.fanfare_pending());
    assert_eq!(shape(&r), Some((8, E, 0, 1, true)));
}

#[test]
fn restore_uses_the_show_argument() {
    let g = game();
    let mut t = Tutorial::new(true);
    ack_all(&mut t);
    t.observe_ui(&on_station(0));
    t.observe_ui(&ACK);
    let mut r = reload(&t, false, &g);
    assert_eq!(r.directive(), None);
    assert!(permitted(&r).iter().all(|&p| p));
    assert_eq!(r.stage(), Some(TutorialStageId(3)), "#3 は路線まで進まない");
    r.set_show_explanations(true);
    assert_eq!(
        shape(&r),
        Some((3, E, 0, 3, true)),
        "見せていない説明を出し直す"
    );

    let mut t = Tutorial::new(false);
    t.observe_ui(&on_station(0));
    assert_eq!(t.stage(), Some(TutorialStageId(3)));
    let r = reload(&t, true, &g);
    assert_eq!(
        shape(&r),
        Some((3, E, 0, 3, true)),
        "「する」で読み込めば出す"
    );
}

#[test]
fn restore_while_the_replay_waits_for_the_fanfare_replays_at_once() {
    let g = game();
    let mut t = Tutorial::new(false);
    t.observe_ui(&on_station(0));
    t.observe_game(&line_created(2));
    t.observe_game(&arrived(1));
    t.observe_game(&level(1));
    t.set_show_explanations(true);
    assert_eq!(shape(&t), Some((5, G, 0, 1, false)));
    let r = reload(&t, true, &g);
    assert_eq!(shape(&r), Some((5, E, 0, 2, true)));
}

#[test]
fn latches_and_shown_lists_survive() {
    let g = game();
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(7));
    t.observe_game(&GameEvent::StationSpawned {
        id: StationId(3),
        pos: GridPos { x: 1, y: 1 },
    });
    t.observe_game(&purchased(S2, BuyMode::One));
    t.observe_game(&purchased(ParamKey::T1Speed, BuyMode::Max));
    t.observe_game(&line_created(2));
    t.observe_game(&extended());
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    ack_all(&mut t);
    t.observe_ui(&UiEvent::MenuClosed(SETTINGS));
    ack_all(&mut t);
    t.observe_game(&level(1));
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((8, E, 0, 1, true)));
    t.observe_ui(&ACK);
    assert_eq!(d(&t).stage.0, 9);
    let mut r = reload(&t, true, &g);
    assert_eq!(r.shown_stages(), t.shown_stages());
    assert_eq!(r.shown_messages(), t.shown_messages());
    assert_eq!(d(&r).highlight, Some(UiAnchor::Station(StationId(3))));
    let l = r.latch;
    assert_eq!(
        (
            l.purchases,
            l.lines_created,
            l.max_line_station_count,
            l.company_seen
        ),
        (2, 1, 2, 1)
    );
    assert!(l.bought_max && l.extended && l.bought.contains(S2) && !l.first_income);
    ack_all(&mut r);
    assert_eq!(shape(&r), Some((9, G, 0, 1, false)));
}

#[test]
fn same_events_after_restore_give_the_same_directives() {
    let mut a = Rig::new(true);
    ack_all(&mut a.t);
    a.ui(on_station(1));
    ack_all(&mut a.t);
    a.cmd(create(&[0, 2]));
    for _ in 0..30 {
        a.frame();
    }
    let mut b = Rig {
        game: a.game.clone(),
        t: Tutorial::restore(a.t.save(), true),
        events: Vec::new(),
    };
    for i in 0..60 * 120 {
        if i % 97 == 0 {
            a.ui(ACK);
            b.ui(ACK);
        }
        a.frame();
        b.frame();
        assert_eq!(a.t.directive(), b.t.directive(), "frame {i}");
        assert_eq!(permitted(&a.t), permitted(&b.t));
    }
    assert_eq!(a.game.state_hash(), b.game.state_hash());
    assert!(a.t.shown_stages().len() >= 5, "{:?}", a.t.shown_stages());
}

#[test]
fn save_after_restore_is_the_same_unless_a_rule_applies() {
    let g = game();
    let mut t = Tutorial::new(true);
    to_guide_of_3(&mut t);
    let s = t.save();
    assert_eq!(Tutorial::restore(s.clone(), true).save(), s);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    let s = t.save();
    assert_eq!(s.side, SavedSide::Showing { page: 0 });
    assert_eq!(reload(&t, true, &g).save().side, SavedSide::NotYet);
}

#[test]
fn finished_tutorial_stays_finished() {
    let g = game();
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(14));
    t.observe_game(&goal());
    t.observe_ui(&FANFARE_DONE);
    t.observe_ui(&ACK);
    assert!(t.is_finished());
    let r = reload(&t, true, &g);
    assert!(r.is_finished());
    assert_eq!(r.directive(), None);
    assert!(permitted(&r).iter().all(|&p| p));
}

#[test]
fn out_of_range_values_are_clamped_or_dropped() {
    let latch = Tutorial::new(true).save().latch;
    let s = TutorialSave {
        stage_idx: 200,
        step: SavedStep::Guide { sub: 9 },
        unshown: false,
        replay_pending: false,
        latch: SavedLatches {
            bought: u16::MAX,
            ..latch
        },
        side: SavedSide::Showing { page: 7 },
        shown: vec![(1, 0), (13, 0), (9, 7), (1, 0), (15, 2)],
        shown_stages: vec![13, 1, 99, 1, 15],
    };
    let r = Tutorial::restore(s, true);
    assert!(r.is_finished());
    assert_eq!(r.step, Step::Wait);
    assert_eq!(r.side, SideState::NotYet);
    assert_eq!(r.shown_messages(), [txt::TUT_S01_P1, txt::TUT_S15_P3]);
    assert_eq!(r.shown_stages(), [TutorialStageId(1), TutorialStageId(15)]);

    let s = TutorialSave {
        stage_idx: idx(9) as u8,
        step: SavedStep::Guide { sub: 9 },
        ..Tutorial::new(true).save()
    };
    let mut r = Tutorial::restore(s, true);
    assert_eq!(r.step, Step::Guide { sub: 2 });
    r.observe_view(game().view());
    assert_eq!(r.stage(), Some(TutorialStageId(10)));
}

#[test]
fn every_latch_survives() {
    let mut t = Tutorial::new(true);
    t.latch = Latches {
        first_income: true,
        purchases: 7,
        bought_max: true,
        bought: ParamSet(0b1010),
        lines_created: 3,
        max_line_station_count: 4,
        extended: true,
        max_gauge_percent: 61,
        company_seen: 5,
        goal: true,
        newest_station: Some(StationId(9)),
    };
    let s = t.save();
    assert_eq!(
        s.latch,
        SavedLatches {
            first_income: true,
            purchases: 7,
            bought_max: true,
            bought: 0b1010,
            lines_created: 3,
            max_line_station_count: 4,
            extended: true,
            max_gauge_percent: 61,
            company_seen: 5,
            goal: true,
            newest_station: Some(9),
        }
    );
    assert_eq!(Tutorial::restore(s.clone(), false).save().latch, s.latch);
}

#[test]
fn ron_round_trip_keeps_everything() {
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(7));
    t.observe_game(&GameEvent::StationSpawned {
        id: StationId(3),
        pos: GridPos { x: 1, y: 1 },
    });
    t.observe_game(&purchased(S2, BuyMode::One));
    t.observe_game(&purchased(ParamKey::T1Speed, BuyMode::Max));
    t.observe_game(&line_created(2));
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    t.observe_ui(&ACK);
    let s = t.save();
    let text = ron::to_string(&s).expect("書ける");
    assert_eq!(ron::from_str::<TutorialSave>(&text).expect("読める"), s);
}

#[test]
fn ron_round_trip_with_no_field_at_its_default() {
    let mut t = Tutorial::new(true);
    ack_all(&mut t);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    ack_all(&mut t);
    t.observe_ui(&UiEvent::MenuClosed(SETTINGS));
    t.stage_idx = idx(9);
    t.step = Step::Guide { sub: 1 };
    t.unshown = true;
    t.replay_pending = true;
    t.latch = Latches {
        first_income: true,
        purchases: 7,
        bought_max: true,
        bought: ParamSet(0b1010),
        lines_created: 3,
        max_line_station_count: 4,
        extended: true,
        max_gauge_percent: 61,
        company_seen: 5,
        goal: true,
        newest_station: Some(StationId(9)),
    };
    let s = t.save();
    let d = Tutorial::new(true).save();
    assert_ne!(s.stage_idx, d.stage_idx);
    assert_ne!(s.step, d.step);
    assert_ne!(s.unshown, d.unshown);
    assert_ne!(s.replay_pending, d.replay_pending);
    assert_ne!(s.side, d.side);
    assert_ne!(s.shown, d.shown);
    assert_ne!(s.shown_stages, d.shown_stages);
    let l = (s.latch, d.latch);
    assert!(l.0.first_income != l.1.first_income && l.0.purchases != l.1.purchases);
    assert!(l.0.bought_max != l.1.bought_max && l.0.bought != l.1.bought);
    assert!(l.0.lines_created != l.1.lines_created && l.0.extended != l.1.extended);
    assert!(l.0.max_line_station_count != l.1.max_line_station_count);
    assert!(l.0.max_gauge_percent != l.1.max_gauge_percent && l.0.goal != l.1.goal);
    assert!(l.0.company_seen != l.1.company_seen && l.0.newest_station != l.1.newest_station);
    let text = ron::to_string(&s).expect("書ける");
    assert_eq!(ron::from_str::<TutorialSave>(&text).expect("読める"), s);
}

#[test]
fn restore_starts_with_every_menu_closed() {
    let mut t = Tutorial::new(true);
    for m in [
        MenuAnchor::UpgradePanel,
        MenuAnchor::UpgradeTab(ParamCategory::Line),
        MenuAnchor::BuildMode,
        SETTINGS,
    ] {
        t.observe_ui(&UiEvent::MenuOpened(m));
    }
    assert_ne!(t.loc, UiLocation::new());
    assert_eq!(Tutorial::restore(t.save(), true).loc, UiLocation::new());
}
