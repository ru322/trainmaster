//! B1 の再発防止(13 §10.3 の決まった手順)。#1 で「しない」にして路線を作らずに「する」に戻しても詰まない。

use std::sync::Arc;

use tm_assets::text::txt;
use tm_core::{
    BalanceTable, Bend, BuyMode, Command, Game, GameConfig, GameEvent, LineEnd, LineId, ParamKey,
    StationId, UpgradeScope,
};
use tm_tutorial::{DirectiveKind, MenuAnchor, Tutorial, UiAnchor, UiEvent};

struct App {
    game: Game,
    t: Tutorial,
    events: Vec<GameEvent>,
}

impl App {
    fn new() -> App {
        App {
            game: Game::new(GameConfig {
                seed: 5,
                balance: Arc::new(BalanceTable::builtin()),
            }),
            t: Tutorial::new(true),
            events: Vec::new(),
        }
    }

    fn paused(&self) -> bool {
        self.t.directive().is_some_and(|d| d.pauses_game)
    }

    fn frame(&mut self) {
        self.t.observe_view(self.game.view());
        if !self.paused() {
            self.game.step(&mut self.events);
            for e in std::mem::take(&mut self.events) {
                self.t.observe_game(&e);
            }
        }
    }

    fn ui(&mut self, ev: UiEvent) {
        self.t.observe_ui(&ev);
    }

    fn toggle_in_settings(&mut self, on: bool) {
        self.ui(UiEvent::MenuOpened(MenuAnchor::SettingsMenu));
        self.t.set_show_explanations(on);
        self.ui(UiEvent::MenuClosed(MenuAnchor::SettingsMenu));
    }
}

fn all_kinds() -> Vec<Command> {
    vec![
        Command::CreateLine {
            stations: vec![StationId(0), StationId(1), StationId(2)],
            bends: vec![Bend::StraightFirst; 2],
        },
        Command::ExtendLine {
            line: LineId(0),
            end: LineEnd::Tail,
            station: StationId(2),
            bend: Bend::StraightFirst,
        },
        Command::LevelUp {
            param: ParamKey::S1Spawn,
            scope: UpgradeScope::AllStations,
            mode: BuyMode::One,
        },
    ]
}

fn two_station_line(a: &App) -> Command {
    Command::CreateLine {
        stations: vec![StationId(0), StationId(1)],
        bends: vec![a.game.view().default_bend(StationId(0), StationId(1))],
    }
}

fn b1(cursor_on_station: bool) -> u8 {
    let mut a = App::new();
    let d = *a.t.directive().expect("#1 が出る");
    assert_eq!(
        (d.stage.0, d.kind, d.pauses_game),
        (1, DirectiveKind::Explanation, true)
    );

    a.toggle_in_settings(false);
    assert_eq!(a.t.directive(), None);
    assert!(all_kinds().iter().all(|c| a.t.permits(c)));
    assert!(a.t.permits(&two_station_line(&a)));
    a.frame();
    assert_eq!(a.game.now().0, 1, "止まらない");

    if cursor_on_station {
        a.ui(UiEvent::CursorOn(UiAnchor::Station(StationId(2))));
    }
    for _ in 0..5 * 60 {
        a.frame();
    }
    assert!(a.game.view().lines().len() == 0);

    a.toggle_in_settings(true);
    let d = *a.t.directive().expect("説明が出る");
    assert_eq!(
        (d.kind, d.page, d.pauses_game),
        (DirectiveKind::Explanation, 0, true)
    );
    let replayed = d.stage.0;
    assert!(replayed == 2 || replayed == 3, "#{replayed}");
    assert!(!all_kinds().iter().any(|c| a.t.permits(c)));

    while a
        .t
        .directive()
        .is_some_and(|d| d.kind == DirectiveKind::Explanation)
    {
        a.ui(UiEvent::MessageAcknowledged);
    }
    if replayed == 2 {
        a.ui(UiEvent::CursorOn(UiAnchor::Station(StationId(0))));
        while a
            .t
            .directive()
            .is_some_and(|d| d.kind == DirectiveKind::Explanation)
        {
            a.ui(UiEvent::MessageAcknowledged);
        }
    }
    let d = *a.t.directive().expect("#3 の案内");
    assert_eq!((d.stage.0, d.kind), (3, DirectiveKind::Guide));
    let line = two_station_line(&a);
    assert!(
        a.t.permits(&line),
        "#3 の案内で 2 駅の CreateLine が許される"
    );
    assert!(!a.t.permits(&all_kinds()[0]), "3 駅は許さない");
    a.game.enqueue(line);
    a.frame();
    a.frame();
    let d = *a.t.directive().expect("#4 の待つ間の案内");
    assert_eq!(
        (d.stage.0, d.kind, d.message),
        (4, DirectiveKind::Guide, txt::TUT_S04_W)
    );
    for _ in 0..120 * 60 {
        if a.paused() {
            break;
        }
        a.frame();
    }
    let d = *a.t.directive().expect("#4 の説明");
    assert_eq!(
        (d.stage.0, d.kind, d.pauses_game),
        (4, DirectiveKind::Explanation, true)
    );
    replayed
}

#[test]
fn b1_off_at_1_then_on_without_a_line_does_not_stick() {
    assert_eq!(b1(false), 2);
}

#[test]
fn b1_with_cursor_on_a_station_replays_from_3() {
    assert_eq!(b1(true), 3);
}
