use std::sync::Arc;

use tm_assets::text::txt;
use tm_core::{
    BalanceTable, Bend, CommandKind, CompanyLevel, Game, GameConfig, GridPos, LineEnd, LineId,
    Money, ParamCategory, People, Ticks, UpgradeScope,
};

use super::*;
use crate::anchor::SettingsItem;

const S2: ParamKey = ParamKey::S2Fare;

fn game() -> Game {
    Game::new(GameConfig {
        seed: 1,
        balance: Arc::new(BalanceTable::builtin()),
    })
}

fn create(stations: &[u32]) -> Command {
    Command::CreateLine {
        stations: stations.iter().copied().map(StationId).collect(),
        bends: vec![Bend::StraightFirst; stations.len() - 1],
    }
}

fn extend(station: u32) -> Command {
    Command::ExtendLine {
        line: LineId(0),
        end: LineEnd::Tail,
        station: StationId(station),
        bend: Bend::StraightFirst,
    }
}

fn level_up(param: ParamKey, scope: UpgradeScope, mode: BuyMode) -> Command {
    Command::LevelUp { param, scope, mode }
}

fn sample_cmds() -> Vec<Command> {
    vec![
        create(&[0, 1]),
        create(&[0, 1, 2]),
        extend(2),
        level_up(S2, UpgradeScope::AllStations, BuyMode::One),
        level_up(ParamKey::S1Spawn, UpgradeScope::AllStations, BuyMode::Max),
        level_up(ParamKey::R2LineSlots, UpgradeScope::Company, BuyMode::Max),
    ]
}

fn permitted(t: &Tutorial) -> Vec<bool> {
    sample_cmds().iter().map(|c| t.permits(c)).collect()
}

fn purchased(param: ParamKey, mode: BuyMode) -> GameEvent {
    GameEvent::Purchased {
        param,
        scope: UpgradeScope::AllStations,
        mode,
        steps: 1,
        raised: 1,
        cost: Money::from_u64(1),
    }
}

fn level(n: u8) -> GameEvent {
    GameEvent::CompanyLevelUp {
        new_level: CompanyLevel(n),
        all_features_unlocked: n >= 4,
    }
}

fn line_created(station_count: u32) -> GameEvent {
    GameEvent::LineCreated {
        line: LineId(0),
        station_count,
        cost: Money::ZERO,
        new_tiles: 0,
    }
}

fn extended() -> GameEvent {
    GameEvent::LineExtended {
        line: LineId(0),
        end: LineEnd::Tail,
        station: StationId(2),
        cost: Money::ZERO,
        new_tiles: 0,
    }
}

fn arrived(yen: u64) -> GameEvent {
    GameEvent::TrainArrived {
        train: TrainId(0),
        line: LineId(0),
        station: StationId(1),
        riders: People(1),
        alighted: People(1),
        boarded: People(0),
        income: Money::from_u64(yen),
        dwell: Ticks(60),
    }
}

fn goal() -> GameEvent {
    GameEvent::GoalReached {
        total_earned: Money::from_u64(1),
    }
}

const ACK: UiEvent = UiEvent::MessageAcknowledged;
const FANFARE_DONE: UiEvent = UiEvent::FanfareFinished;
const SETTINGS: MenuAnchor = MenuAnchor::SettingsMenu;

fn on_station(i: u32) -> UiEvent {
    UiEvent::CursorOn(UiAnchor::Station(StationId(i)))
}

fn menu(m: MenuAnchor) -> Option<UiAnchor> {
    Some(UiAnchor::Menu(m))
}

fn item(p: ParamKey) -> Option<UiAnchor> {
    menu(MenuAnchor::UpgradeItem(p))
}

fn idx(stage: u8) -> usize {
    MAIN_STAGES
        .iter()
        .position(|s| s.id.0 == stage)
        .expect("本線の段階")
}

fn d(t: &Tutorial) -> TutorialDirective {
    *t.directive().expect("directive がある")
}

/// (段階, 種類, ページ, ページ数, 止めるか)
fn shape(t: &Tutorial) -> Option<(u8, DirectiveKind, u8, u8, bool)> {
    t.directive()
        .map(|d| (d.stage.0, d.kind, d.page, d.page_count, d.pauses_game))
}

use DirectiveKind::{Explanation as E, Guide as G};

fn ack_all(t: &mut Tutorial) {
    while t.directive().is_some_and(|d| d.kind == E) {
        t.observe_ui(&ACK);
    }
}

/// 実際の `Game` と組にして tm-app と同じ順で回す。
struct Rig {
    game: Game,
    t: Tutorial,
    events: Vec<GameEvent>,
}

impl Rig {
    fn new(show: bool) -> Rig {
        Rig {
            game: game(),
            t: Tutorial::new(show),
            events: Vec::new(),
        }
    }

    fn paused(&self) -> bool {
        self.t.directive().is_some_and(|d| d.pauses_game)
    }

    fn frame(&mut self) {
        self.t.observe_view(self.game.view());
        if self.paused() {
            return;
        }
        self.game.step(&mut self.events);
        for e in self.events.drain(..) {
            self.t.observe_game(&e);
        }
    }

    fn frames_until(&mut self, max: u32, f: impl Fn(&Tutorial) -> bool) {
        for _ in 0..max {
            if f(&self.t) {
                return;
            }
            self.frame();
        }
        assert!(
            f(&self.t),
            "{max} フレームで成り立たない: {:?}",
            shape(&self.t)
        );
    }

    fn cmd(&mut self, c: Command) {
        assert!(
            self.t.permits(&c),
            "{c:?} が許されない: {:?}",
            shape(&self.t)
        );
        assert!(self.game.view().validate(&c).is_ok(), "{c:?}");
        self.game.enqueue(c);
        self.frame();
    }

    fn ui(&mut self, ev: UiEvent) {
        self.t.observe_ui(&ev);
    }
}

#[test]
fn stages_1_to_6_follow_the_table() {
    let mut r = Rig::new(true);
    assert_eq!(shape(&r.t), Some((1, E, 0, 2, true)));
    assert_eq!(d(&r.t).message, txt::TUT_S01_P1);
    assert_eq!(d(&r.t).highlight, None);
    assert!(permitted(&r.t).iter().all(|&p| !p));
    r.frame();
    assert_eq!(r.game.now().0, 0, "止める説明の間は進まない");
    r.ui(ACK);
    assert_eq!(shape(&r.t), Some((1, E, 1, 2, true)));
    assert_eq!(d(&r.t).message, txt::TUT_S01_P2);
    r.ui(ACK);

    assert_eq!(shape(&r.t), Some((2, E, 0, 1, true)));
    assert_eq!(d(&r.t).highlight, Some(UiAnchor::Station(StationId(0))));
    r.ui(ACK);
    assert_eq!(shape(&r.t), Some((2, G, 0, 1, false)));
    assert_eq!(d(&r.t).message, txt::TUT_S02_G1);
    assert_eq!(d(&r.t).highlight, Some(UiAnchor::Station(StationId(0))));
    assert!(permitted(&r.t).iter().all(|&p| !p));
    r.frame();
    r.ui(UiEvent::CursorOn(UiAnchor::Line(LineId(0))));
    assert_eq!(d(&r.t).stage.0, 2);
    r.ui(on_station(1));

    assert_eq!(shape(&r.t), Some((3, E, 0, 3, true)));
    let hl: Vec<_> = (0..3)
        .map(|_| {
            let h = d(&r.t).highlight;
            r.ui(ACK);
            h
        })
        .collect();
    assert_eq!(hl, [None, menu(MenuAnchor::BuildMode), None]);
    assert_eq!(shape(&r.t), Some((3, G, 0, 1, false)));
    assert_eq!(d(&r.t).message, txt::TUT_S03_G1);
    assert_eq!(
        permitted(&r.t),
        [true, false, false, false, false, false],
        "#3 は 2 駅の CreateLine だけ"
    );
    assert_eq!(d(&r.t).highlight, menu(MenuAnchor::BuildMode));
    r.ui(UiEvent::MenuOpened(MenuAnchor::BuildMode));
    assert_eq!(d(&r.t).highlight, menu(MenuAnchor::BuildCreate));
    r.ui(UiEvent::MenuOpened(MenuAnchor::BuildCreate));
    assert_eq!(d(&r.t).highlight, None);
    r.ui(UiEvent::MenuOpened(MenuAnchor::LineConfirm));
    assert_eq!(d(&r.t).highlight, menu(MenuAnchor::LineConfirm));
    r.cmd(create(&[0, 1]));
    r.ui(UiEvent::MenuClosed(MenuAnchor::LineConfirm));
    r.ui(UiEvent::MenuClosed(MenuAnchor::BuildMode));

    r.frame();
    assert_eq!(shape(&r.t), Some((4, G, 0, 1, false)));
    assert_eq!(d(&r.t).message, txt::TUT_S04_W);
    assert_eq!(d(&r.t).highlight, Some(UiAnchor::Train(TrainId(0))));
    assert!(
        permitted(&r.t).iter().all(|&p| !p),
        "路線があれば #4 の待つ間は何も許さない"
    );
    r.frames_until(60 * 120, |t| t.directive().is_some_and(|d| d.kind == E));
    assert_eq!(shape(&r.t), Some((4, E, 0, 1, true)));
    assert_eq!(d(&r.t).highlight, Some(UiAnchor::MoneyDisplay));
    r.ui(ACK);

    assert_eq!(shape(&r.t), Some((5, E, 0, 2, true)));
    assert_eq!(d(&r.t).highlight, Some(UiAnchor::Station(StationId(2))));
    r.ui(ACK);
    assert_eq!(d(&r.t).highlight, menu(MenuAnchor::BuildMode));
    r.ui(ACK);
    assert_eq!(shape(&r.t), Some((5, G, 0, 1, false)));
    assert_eq!(permitted(&r.t), [false, false, true, false, false, false]);
    r.ui(UiEvent::MenuOpened(MenuAnchor::BuildMode));
    assert_eq!(d(&r.t).highlight, menu(MenuAnchor::BuildExtend));
    r.ui(UiEvent::MenuOpened(MenuAnchor::BuildExtend));
    assert_eq!(d(&r.t).highlight, Some(UiAnchor::Station(StationId(2))));
    r.ui(UiEvent::MenuClosed(MenuAnchor::BuildMode));
    r.cmd(extend(2));

    r.frame();
    assert_eq!(shape(&r.t), Some((6, G, 0, 1, false)));
    assert_eq!(d(&r.t).message, txt::TUT_S06_W);
    assert_eq!(d(&r.t).highlight, Some(UiAnchor::MoneyDisplay));
    assert!(permitted(&r.t).iter().all(|&p| !p));
    r.frames_until(60 * 600, |t| t.directive().is_some_and(|d| d.kind == E));
    assert!(
        r.game
            .view()
            .validate(&level_up(S2, UpgradeScope::AllStations, BuyMode::One))
            .is_ok()
    );
    assert_eq!(shape(&r.t), Some((6, E, 0, 2, true)));
    assert_eq!(d(&r.t).highlight, item(S2));
    r.ui(ACK);
    assert_eq!(d(&r.t).highlight, menu(MenuAnchor::ScopeToggle));
    r.ui(ACK);
    assert_eq!(shape(&r.t), Some((6, G, 0, 1, false)));
    assert_eq!(permitted(&r.t), [false, false, false, true, false, false]);
    assert!(r.t.permits(&level_up(
        S2,
        UpgradeScope::Station(StationId(0)),
        BuyMode::Max
    )));
    assert_eq!(d(&r.t).highlight, menu(MenuAnchor::UpgradePanel));
    r.ui(UiEvent::MenuOpened(MenuAnchor::UpgradePanel));
    r.ui(UiEvent::MenuOpened(MenuAnchor::UpgradeTab(
        ParamCategory::Train,
    )));
    assert_eq!(
        d(&r.t).highlight,
        menu(MenuAnchor::UpgradeTab(ParamCategory::Station))
    );
    r.ui(UiEvent::MenuOpened(MenuAnchor::UpgradeTab(
        ParamCategory::Station,
    )));
    assert_eq!(d(&r.t).highlight, item(S2));
    r.cmd(level_up(
        S2,
        UpgradeScope::Station(StationId(1)),
        BuyMode::One,
    ));

    assert_eq!(r.t.stage(), Some(TutorialStageId(7)));
    assert_eq!(r.t.directive(), None);
    assert!(permitted(&r.t).iter().all(|&p| p), "#7 以降はすべて許す");
    assert_eq!(
        r.t.shown_stages().iter().map(|s| s.0).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    assert_eq!(r.t.shown_messages().len(), 2 + 1 + 3 + 1 + 2 + 2);
}

#[test]
fn stages_7_to_14_follow_the_table() {
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(7));
    assert!(permitted(&t).iter().all(|&p| p));
    t.observe_game(&purchased(S2, BuyMode::One));
    assert_eq!(t.directive(), None);
    t.observe_game(&purchased(ParamKey::S1Spawn, BuyMode::One));
    assert_eq!(shape(&t), Some((7, E, 0, 2, true)));
    assert_eq!(d(&t).highlight, menu(MenuAnchor::BuyMaxHint));
    assert!(
        permitted(&t).iter().all(|&p| !p),
        "止める説明の間はすべて false"
    );
    t.observe_ui(&ACK);
    assert_eq!(d(&t).highlight, Some(UiAnchor::RecommendMark));
    t.observe_ui(&ACK);
    assert_eq!(shape(&t), Some((7, G, 0, 1, false)));
    assert_eq!(d(&t).message, txt::TUT_S07_G1);
    assert_eq!(d(&t).highlight, menu(MenuAnchor::UpgradePanel));
    t.observe_ui(&UiEvent::MenuOpened(MenuAnchor::UpgradePanel));
    assert_eq!(d(&t).highlight, menu(MenuAnchor::BuyMaxHint));
    assert!(permitted(&t).iter().all(|&p| p));
    t.observe_game(&purchased(ParamKey::T1Speed, BuyMode::One));
    assert_eq!(d(&t).stage.0, 7);
    t.observe_game(&purchased(ParamKey::T1Speed, BuyMode::Max));
    assert_eq!(t.stage(), Some(TutorialStageId(8)));
    assert_eq!(t.directive(), None);

    t.observe_game(&level(1));
    assert_eq!(t.directive(), None, "ジングルの間は出さない");
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((8, E, 0, 1, true)));
    assert_eq!(d(&t).highlight, Some(UiAnchor::CompanyGauge));
    t.observe_ui(&ACK);

    assert_eq!(shape(&t), Some((9, E, 0, 4, true)));
    t.observe_game(&GameEvent::StationSpawned {
        id: StationId(3),
        pos: GridPos { x: 1, y: 1 },
    });
    assert_eq!(d(&t).highlight, Some(UiAnchor::Station(StationId(3))));
    t.observe_ui(&ACK);
    assert_eq!(d(&t).highlight, item(ParamKey::R2LineSlots));
    t.observe_ui(&ACK);
    assert_eq!(d(&t).highlight, None);
    t.observe_ui(&ACK);
    assert_eq!(shape(&t), Some((9, E, 3, 4, true)));
    assert_eq!(d(&t).message, txt::TUT_S09_P4, "線路のお金は最後のページ");
    assert_eq!(d(&t).highlight, Some(UiAnchor::MoneyDisplay));
    t.observe_ui(&ACK);
    assert_eq!(shape(&t), Some((9, G, 0, 1, false)));
    assert_eq!(d(&t).message, txt::TUT_S09_G1);
    t.observe_game(&purchased(ParamKey::R2LineSlots, BuyMode::One));
    assert_eq!(d(&t).message, txt::TUT_S09_G2);
    assert_eq!(d(&t).highlight, menu(MenuAnchor::BuildMode));
    t.observe_game(&line_created(2));
    assert_eq!(t.stage(), Some(TutorialStageId(10)));

    t.observe_game(&level(2));
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((10, E, 0, 2, true)));
    assert_eq!(d(&t).highlight, item(ParamKey::T5Cars));
    t.observe_ui(&ACK);
    assert_eq!(d(&t).highlight, item(ParamKey::S4Platform));
    t.observe_ui(&ACK);
    assert_eq!(d(&t).message, txt::TUT_S10_G1);
    t.observe_game(&purchased(ParamKey::S4Platform, BuyMode::One));
    assert_eq!(d(&t).message, txt::TUT_S10_G2);
    t.observe_game(&purchased(ParamKey::T5Cars, BuyMode::One));
    assert_eq!(t.stage(), Some(TutorialStageId(11)));

    t.observe_game(&level(3));
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((11, E, 0, 2, true)));
    assert_eq!(d(&t).highlight, item(ParamKey::R1MaxTrains));
    t.observe_ui(&ACK);
    assert_eq!(d(&t).highlight, item(ParamKey::R3Attractiveness));
    t.observe_ui(&ACK);
    assert_eq!(d(&t).message, txt::TUT_S11_G1);
    t.observe_game(&purchased(ParamKey::R3Attractiveness, BuyMode::One));
    assert_eq!(
        t.stage(),
        Some(TutorialStageId(12)),
        "後の案内の done で飛ぶ"
    );

    t.observe_game(&level(4));
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((12, E, 0, 3, true)));
    let hl: Vec<_> = (0..3)
        .map(|_| {
            let h = d(&t).highlight;
            t.observe_ui(&ACK);
            h
        })
        .collect();
    assert_eq!(
        hl,
        [item(ParamKey::T3Boarding), item(ParamKey::T2Crowding), None]
    );
    assert_eq!(d(&t).message, txt::TUT_S12_G1);
    t.observe_game(&purchased(ParamKey::T3Boarding, BuyMode::One));
    assert_eq!(d(&t).message, txt::TUT_S12_G2);
    t.observe_game(&purchased(ParamKey::T2Crowding, BuyMode::Max));
    assert_eq!(t.stage(), Some(TutorialStageId(14)));
    assert_eq!(t.directive(), None);

    t.observe_game(&goal());
    assert_eq!(t.directive(), None, "お祝いの演出の間は出さない");
    assert!(!t.is_finished());
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((14, E, 0, 1, true)));
    assert_eq!(d(&t).message, txt::TUT_S14_P1);
    t.observe_ui(&ACK);
    assert!(t.is_finished());
    assert_eq!(t.directive(), None);
    assert!(permitted(&t).iter().all(|&p| p));
}

#[test]
fn b1_step5_wait_of_4_without_line_allows_only_two_station_create() {
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(4));
    assert_eq!(permitted(&t), [true, false, false, false, false, false]);
    t.observe_game(&line_created(2));
    assert!(permitted(&t).iter().all(|&p| !p));
}

#[test]
fn three_station_line_skips_5() {
    let mut t = Tutorial::new(false);
    t.observe_ui(&on_station(0));
    t.observe_game(&line_created(3));
    t.set_show_explanations(true);
    assert_eq!(t.stage(), Some(TutorialStageId(4)));
    t.observe_game(&arrived(1));
    assert_eq!(shape(&t), Some((4, E, 0, 1, true)));
    t.observe_ui(&ACK);
    assert_eq!(t.stage(), Some(TutorialStageId(6)));
    assert!(!t.shown_stages().contains(&TutorialStageId(5)));
}

#[test]
fn all_stations_connected_skips_5() {
    let mut r = Rig::new(false);
    r.ui(on_station(0));
    r.cmd(create(&[0, 1]));
    r.cmd(extend(2));
    assert_eq!(r.t.stage(), Some(TutorialStageId(4)));
    r.t.set_show_explanations(true);
    r.frames_until(60 * 120, |t| t.directive().is_some_and(|d| d.kind == E));
    assert_eq!(d(&r.t).stage.0, 4);
    r.ui(ACK);
    r.frame();
    assert_eq!(r.t.stage(), Some(TutorialStageId(6)));
    assert!(!r.t.shown_stages().contains(&TutorialStageId(5)));
}

#[test]
fn fare_bought_first_skips_6() {
    let mut r = Rig::new(false);
    r.ui(on_station(0));
    r.cmd(create(&[0, 1]));
    r.frames_until(60 * 120, |t| t.stage() == Some(TutorialStageId(5)));
    r.cmd(extend(2));
    assert_eq!(r.t.stage(), Some(TutorialStageId(6)));
    let one = level_up(S2, UpgradeScope::Station(StationId(0)), BuyMode::One);
    for _ in 0..60 * 600 {
        if r.game.view().validate(&one).is_ok() {
            break;
        }
        r.frame();
    }
    r.cmd(one);
    r.t.set_show_explanations(true);
    r.frames_until(60 * 600, |t| t.stage() != Some(TutorialStageId(6)));
    assert!(!r.t.shown_stages().contains(&TutorialStageId(6)));
    assert_eq!(r.t.stage(), Some(TutorialStageId(7)));
}

#[test]
fn gauge_and_level_in_the_same_tick_still_show_8() {
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(8));
    t.observe_game(&level(1));
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((8, E, 0, 1, true)));
    t.observe_ui(&ACK);
    assert_eq!(d(&t).stage.0, 9);
}

#[test]
fn level_1_while_guide_7_supersedes_after_fanfare() {
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(7));
    t.observe_game(&purchased(S2, BuyMode::One));
    t.observe_game(&purchased(S2, BuyMode::One));
    ack_all(&mut t);
    assert_eq!(shape(&t), Some((7, G, 0, 1, false)));
    t.observe_game(&level(1));
    assert_eq!(
        shape(&t),
        Some((7, G, 0, 1, false)),
        "ジングルの間は案内のまま"
    );
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((8, E, 0, 1, true)));
}

#[test]
fn two_levels_at_once_show_9_then_10_and_cut_the_guide_of_9() {
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(9));
    t.observe_game(&level(1));
    t.observe_game(&level(2));
    assert_eq!(t.directive(), None);
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((9, E, 0, 4, true)));
    for _ in 0..4 {
        t.observe_ui(&ACK);
    }
    assert_eq!(shape(&t), Some((10, E, 0, 2, true)));
    assert_eq!(
        t.shown_stages().iter().map(|s| s.0).collect::<Vec<_>>(),
        [1, 9, 10]
    );
}

#[test]
fn guides_of_2_3_5_6_are_not_cut_by_later_triggers() {
    for (stage, cut) in [(2, false), (3, false), (5, false), (6, false), (9, true)] {
        let mut t = Tutorial::new(true);
        t.jump_to_wait(idx(stage));
        t.latch.first_income = true;
        t.seen.can_afford = true;
        t.latch.purchases = 5;
        t.latch.company_seen = 15;
        t.latch.goal = true;
        t.advance();
        for _ in 0..MAIN_STAGES[idx(stage)].pages.len() {
            t.observe_ui(&ACK);
        }
        assert_eq!(d(&t).stage.0 != stage, cut, "#{stage}: {:?}", shape(&t));
    }
}

#[test]
fn fanfare_safety_net_after_3600_frames() {
    let g = game();
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(9));
    t.observe_game(&level(1));
    for _ in 0..FANFARE_FRAME_LIMIT - 1 {
        t.observe_view(g.view());
    }
    assert!(t.fanfare_pending());
    assert_eq!(t.directive(), None);
    t.observe_view(g.view());
    assert!(!t.fanfare_pending());
    assert_eq!(shape(&t), Some((9, E, 0, 4, true)));
}

#[test]
fn fanfare_count_restarts_on_each_level_up() {
    let g = game();
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(9));
    t.observe_game(&level(1));
    for _ in 0..FANFARE_FRAME_LIMIT - 1 {
        t.observe_view(g.view());
    }
    t.observe_game(&level(2));
    t.observe_view(g.view());
    assert!(t.fanfare_pending());
}

#[test]
fn off_hides_everything_and_permits_all() {
    let mut t = Tutorial::new(true);
    assert!(t.directive().is_some_and(|d| d.pauses_game));
    t.set_show_explanations(false);
    assert_eq!(
        t.directive(),
        None,
        "説明の途中で しない にしたら止まらない"
    );
    assert!(permitted(&t).iter().all(|&p| p));
    assert_eq!(t.shown_messages(), [txt::TUT_S01_P1]);
    assert_eq!(t.stage(), Some(TutorialStageId(2)));

    let mut t = Tutorial::new(false);
    assert_eq!(t.directive(), None);
    assert!(t.shown_messages().is_empty());
    assert_eq!(t.stage(), Some(TutorialStageId(2)), "#2 は操作まで進まない");
    t.observe_ui(&on_station(2));
    assert_eq!(t.stage(), Some(TutorialStageId(3)), "#3 は路線まで進まない");
    t.observe_game(&line_created(2));
    t.observe_game(&arrived(1));
    assert_eq!(t.stage(), Some(TutorialStageId(5)), "#5 は延伸まで進まない");
    t.observe_game(&extended());
    assert_eq!(t.stage(), Some(TutorialStageId(6)));
    t.seen.can_afford = true;
    t.advance();
    assert_eq!(
        t.stage(),
        Some(TutorialStageId(6)),
        "#6 は運賃の購入まで進まない"
    );
    t.observe_game(&purchased(S2, BuyMode::One));
    t.observe_game(&purchased(S2, BuyMode::One));
    assert_eq!(t.stage(), Some(TutorialStageId(8)), "#7 の案内は済んだ扱い");
    t.observe_game(&level(1));
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(
        t.stage(),
        Some(TutorialStageId(10)),
        "#9 の案内は済んだ扱い"
    );
    for n in 2..=4 {
        t.observe_game(&level(n));
        t.observe_ui(&FANFARE_DONE);
    }
    assert_eq!(t.stage(), Some(TutorialStageId(14)));
    t.observe_game(&goal());
    t.observe_ui(&FANFARE_DONE);
    assert!(t.is_finished());
    assert!(t.shown_messages().is_empty());
    assert!(t.shown_stages().is_empty());
    assert_eq!(t.directive(), None);
}

#[test]
fn on_again_replays_the_unshown_stage_from_page_1() {
    let mut t = Tutorial::new(true);
    t.observe_ui(&ACK);
    t.set_show_explanations(false);
    t.set_show_explanations(true);
    assert_eq!(shape(&t), Some((2, E, 0, 1, true)));
    ack_all(&mut t);
    t.observe_ui(&on_station(0));
    assert_eq!(shape(&t), Some((3, E, 0, 3, true)));
    t.observe_ui(&ACK);
    t.set_show_explanations(false);
    assert_eq!(t.directive(), None);
    t.set_show_explanations(true);
    assert_eq!(shape(&t), Some((3, E, 0, 3, true)), "1 ページ目から");

    ack_all(&mut t);
    t.set_show_explanations(false);
    t.set_show_explanations(true);
    assert_eq!(
        shape(&t),
        Some((3, G, 0, 1, false)),
        "読んだ説明は出し直さない"
    );
}

#[test]
fn passed_stages_are_not_shown_again() {
    let mut t = Tutorial::new(false);
    t.observe_ui(&on_station(0));
    t.observe_game(&line_created(3));
    t.observe_game(&arrived(1));
    t.seen.can_afford = true;
    t.observe_game(&purchased(S2, BuyMode::One));
    t.observe_game(&purchased(ParamKey::S1Spawn, BuyMode::One));
    for n in 1..=3 {
        t.observe_game(&level(n));
        t.observe_ui(&FANFARE_DONE);
    }
    assert_eq!(t.stage(), Some(TutorialStageId(12)));
    t.set_show_explanations(true);
    assert_eq!(t.directive(), None);
    t.observe_game(&level(4));
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((12, E, 0, 3, true)));
    assert_eq!(
        t.shown_stages().iter().map(|s| s.0).collect::<Vec<_>>(),
        [12]
    );
}

#[test]
fn on_again_waits_for_the_fanfare() {
    let mut t = Tutorial::new(false);
    t.observe_ui(&on_station(0));
    t.observe_game(&line_created(2));
    t.observe_game(&arrived(1));
    assert_eq!(t.stage(), Some(TutorialStageId(5)));
    t.observe_game(&level(1));
    t.set_show_explanations(true);
    assert_eq!(
        shape(&t),
        Some((5, G, 0, 1, false)),
        "ジングルの間は出さない"
    );
    assert_eq!(permitted(&t), [false, false, true, false, false, false]);
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((5, E, 0, 2, true)));
}

#[test]
fn replay_is_dropped_when_the_stage_completes_first() {
    let mut t = Tutorial::new(false);
    t.observe_ui(&on_station(0));
    t.observe_game(&line_created(2));
    t.observe_game(&arrived(1));
    t.observe_game(&level(1));
    t.set_show_explanations(true);
    t.observe_game(&extended());
    assert_eq!(t.stage(), Some(TutorialStageId(6)));
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((6, G, 0, 1, false)), "#6 の待つ間の案内");
}

#[test]
fn settings_explanation_15() {
    let mut t = Tutorial::new(true);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    assert_eq!(shape(&t), Some((15, E, 0, 3, false)));
    assert_eq!(
        d(&t).highlight,
        menu(MenuAnchor::SettingsItem(SettingsItem::SoundVolume))
    );
    assert!(
        permitted(&t).iter().all(|&p| !p),
        "本線の #1 で止まっている"
    );
    t.observe_ui(&ACK);
    assert_eq!(shape(&t), Some((15, E, 1, 3, false)));
    assert_eq!(
        d(&t).highlight,
        menu(MenuAnchor::SettingsItem(SettingsItem::Reset))
    );
    t.observe_ui(&ACK);
    assert_eq!(shape(&t), Some((15, E, 2, 3, false)));
    assert_eq!(d(&t).message, txt::TUT_S15_P3);
    assert_eq!(
        d(&t).highlight,
        menu(MenuAnchor::SettingsItem(SettingsItem::Quit))
    );
    t.observe_ui(&ACK);
    assert_eq!(
        shape(&t),
        Some((1, E, 0, 2, true)),
        "#1 は 1 ページ目のまま"
    );
    t.observe_ui(&UiEvent::MenuClosed(SETTINGS));
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    assert_eq!(d(&t).stage.0, 1, "#15 は 1 回だけ");
    assert_eq!(t.shown_stages(), [TutorialStageId(1), TutorialStageId(15)]);

    let mut t = Tutorial::new(true);
    t.observe_ui(&ACK);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    assert_eq!(d(&t).stage.0, 15);
    t.observe_ui(&UiEvent::MenuClosed(SETTINGS));
    assert_eq!(
        shape(&t),
        Some((1, E, 1, 2, true)),
        "閉じると本線の説明に戻る"
    );
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    assert_eq!(d(&t).stage.0, 1, "閉じたら #15 は終わり");

    let mut t = Tutorial::new(false);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    t.set_show_explanations(true);
    assert_ne!(d(&t).stage.0, 15, "しない の間に開いたら見せずに終わる");
    t.observe_ui(&UiEvent::MenuClosed(SETTINGS));
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    assert_ne!(d(&t).stage.0, 15);
}

#[test]
fn turning_off_in_settings_ends_15() {
    let mut t = Tutorial::new(true);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    t.set_show_explanations(false);
    t.set_show_explanations(true);
    assert_ne!(d(&t).stage.0, 15);
}

#[test]
fn rejected_commands_do_not_advance() {
    let mut t = Tutorial::new(true);
    ack_all(&mut t);
    t.observe_ui(&on_station(0));
    ack_all(&mut t);
    let before = shape(&t);
    t.observe_game(&GameEvent::CommandRejected {
        kind: CommandKind::CreateLine,
        reason: CommandError::NoFreeLineSlot { used: 1, slots: 1 },
    });
    assert_eq!(shape(&t), before);
    assert_eq!(t.stage(), Some(TutorialStageId(3)));
}

#[test]
fn a_new_tutorial_after_reset_starts_from_1_and_shows_15_again() {
    let mut t = Tutorial::new(true);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    ack_all(&mut t);
    t.observe_ui(&UiEvent::MenuClosed(SETTINGS));
    ack_all(&mut t);
    t = Tutorial::new(true);
    assert_eq!(shape(&t), Some((1, E, 0, 2, true)));
    assert!(t.shown_stages() == [TutorialStageId(1)]);
    t.observe_ui(&UiEvent::MenuOpened(SETTINGS));
    assert_eq!(d(&t).stage.0, 15);
}

#[test]
fn same_events_give_the_same_directives() {
    let run = || {
        let mut r = Rig::new(true);
        let mut out = Vec::new();
        for i in 0..60 * 90 {
            match i {
                30 | 60 | 90 => r.ui(ACK),
                120 => r.ui(on_station(1)),
                150 | 160 | 170 => r.ui(ACK),
                200 => r.cmd(create(&[0, 2])),
                _ => r.frame(),
            }
            out.push((r.t.directive().copied(), r.game.now()));
        }
        out
    };
    let a = run();
    assert_eq!(a, run());
    assert!(a.iter().any(|(d, _)| d.is_some_and(|d| d.stage.0 == 4)));
}

#[test]
fn an_extension_before_5_completes_its_guide() {
    let mut t = Tutorial::new(false);
    t.observe_ui(&on_station(0));
    t.observe_game(&line_created(2));
    t.observe_game(&extended());
    assert_eq!(t.stage(), Some(TutorialStageId(4)));
    t.observe_game(&arrived(1));
    assert_eq!(t.stage(), Some(TutorialStageId(6)));
}

#[test]
fn gauge_50_seen_at_level_0_starts_8() {
    let mut r = Rig::new(false);
    r.ui(on_station(0));
    r.cmd(create(&[0, 1]));
    r.frames_until(60 * 120, |t| t.stage() == Some(TutorialStageId(5)));
    r.cmd(extend(2));
    r.t.set_show_explanations(true);
    let cheap = [
        (ParamKey::S1Spawn, UpgradeScope::AllStations),
        (S2, UpgradeScope::AllStations),
        (ParamKey::T1Speed, UpgradeScope::AllLines),
        (ParamKey::T4CarCapacity, UpgradeScope::AllLines),
    ];
    for _ in 0..60 * 60 * 20 {
        if r.t.directive().is_some_and(|d| d.stage.0 == 8) {
            break;
        }
        ack_all(&mut r.t);
        let buy = cheap
            .iter()
            .map(|&(p, s)| level_up(p, s, BuyMode::One))
            .find(|c| r.t.permits(c) && r.game.view().validate(c).is_ok());
        match buy {
            Some(c) => r.cmd(c),
            None => r.frame(),
        }
    }
    assert_eq!(shape(&r.t), Some((8, E, 0, 1, true)));
    let c = r.game.view().company();
    assert_eq!(c.level.0, 0);
    assert!(c.gauge.display_percent >= 50);
}

#[test]
fn off_again_during_the_fanfare_cancels_the_replay() {
    let mut t = Tutorial::new(false);
    t.observe_ui(&on_station(0));
    assert_eq!(t.stage(), Some(TutorialStageId(3)));
    t.observe_game(&level(1));
    t.set_show_explanations(true);
    t.set_show_explanations(false);
    t.observe_ui(&FANFARE_DONE);
    assert!(t.shown_stages().is_empty());
    assert!(t.shown_messages().is_empty());
    t.observe_game(&line_created(2));
    assert_eq!(t.stage(), Some(TutorialStageId(4)));
}

#[test]
fn goal_restarts_the_fanfare_count() {
    let g = game();
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(14));
    t.observe_game(&level(5));
    for _ in 0..FANFARE_FRAME_LIMIT {
        t.observe_view(g.view());
    }
    assert!(!t.fanfare_pending());
    t.observe_game(&goal());
    t.observe_view(g.view());
    assert!(t.fanfare_pending());
    assert_eq!(t.directive(), None, "お祝いの演出の間は出さない");
    t.observe_ui(&FANFARE_DONE);
    assert_eq!(shape(&t), Some((14, E, 0, 1, true)));
}

#[test]
fn stage_8_starts_at_gauge_50_not_49() {
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(8));
    t.latch.max_gauge_percent = 49;
    t.advance();
    assert_eq!(t.directive(), None);
    t.latch.max_gauge_percent = 50;
    t.advance();
    assert_eq!(shape(&t), Some((8, E, 0, 1, true)));
}

#[test]
fn unconnected_station_is_the_smallest_id() {
    let g = game();
    assert!(g.view().stations().len() >= 3);
    let mut t = Tutorial::new(true);
    t.jump_to_wait(idx(5));
    t.observe_view(g.view());
    assert_eq!(shape(&t), Some((5, E, 0, 2, true)));
    assert_eq!(d(&t).highlight, Some(UiAnchor::Station(StationId(0))));
}

#[test]
fn fare_bought_while_off_skips_6_without_waiting_for_money() {
    let mut r = Rig::new(false);
    r.ui(on_station(0));
    r.cmd(create(&[0, 1]));
    r.frames_until(60 * 120, |t| t.stage() == Some(TutorialStageId(5)));
    r.cmd(extend(2));
    let one = level_up(S2, UpgradeScope::Station(StationId(0)), BuyMode::One);
    for _ in 0..60 * 600 {
        if r.game.view().validate(&one).is_ok() {
            break;
        }
        r.frame();
    }
    r.cmd(one);
    let fare_all = level_up(S2, UpgradeScope::AllStations, BuyMode::One);
    assert!(
        matches!(
            r.game.view().validate(&fare_all),
            Err(tm_core::CommandError::InsufficientFunds { .. })
        ),
        "前提: #6 の開始条件はまだ成り立たない"
    );
    let all = level_up(ParamKey::S1Spawn, UpgradeScope::AllStations, BuyMode::One);
    r.t.set_show_explanations(true);
    assert_eq!(
        r.t.stage(),
        Some(TutorialStageId(7)),
        "お金を待たずに #6 を飛ばす"
    );
    assert!(r.t.directive().is_none());
    assert!(r.t.permits(&all), "#7 の待つ間はすべて許す");
}

mod save;
mod twins;
