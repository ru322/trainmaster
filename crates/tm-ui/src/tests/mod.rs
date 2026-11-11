//! `UiHarness` を通したテスト(13 §10.2)。

mod audio;
mod budget;
mod build;
mod camera;
mod devices;
mod explain;
mod flow;
mod map;
mod numbers;
mod panel;
mod raster;
mod readability;
mod save_quit;
mod screens;
mod snapshots;
mod spawn;
mod tutorial_run;

use std::sync::Arc;

use tm_core::{
    BalanceTable, Bend, BuyMode, Command, Game, GameConfig, GridPos, ParamKey, StationId,
    UpgradeScope,
};

use crate::testing::UiHarness;

/// `draw_text()` の `Fill` の行の頭(位置と大きさ)。
pub(crate) fn fill_at(r: tm_platform_api::Rect) -> String {
    format!("Fill {},{} {}x{} ", r.x, r.y, r.w, r.h)
}

/// `draw_text()` の `Sprite` の行の頭(名前と位置)。
pub(crate) fn sprite_at(name: &str, x: i32, y: i32) -> String {
    format!("Sprite {name} {x},{y} ")
}

/// 今のゲームのマップのマス数 (横, 縦)。
pub(crate) fn map_tiles(h: &UiHarness) -> (i32, i32) {
    crate::scene::map_tiles(h.game.as_ref().expect("ゲーム").view())
}

/// 駅の出現順の位置(`max_stations` 個)。最初の 3 駅は (4, 4)、(12, 4)、(12, 10)。20 駅目からは右の列。
pub(crate) fn plan() -> Vec<GridPos> {
    let mut v = vec![
        GridPos { x: 4, y: 4 },
        GridPos { x: 12, y: 4 },
        GridPos { x: 12, y: 10 },
    ];
    let mut i = 0;
    while v.len() < 19 {
        let p = GridPos {
            x: 2 + (i * 7) % 28,
            y: 2 + (i * 5) % 14,
        };
        if v.iter().all(|q| q.chebyshev(p) >= 2) {
            v.push(p);
        }
        i += 1;
    }
    let n = BalanceTable::builtin().max_stations() as i32;
    v.extend((0..n - 19).map(|j| GridPos {
        x: 30 + 3 * (j % 3),
        y: 2 + 4 * (j / 3),
    }));
    v
}

/// 路線を作るコマンド(曲がり方はすべて「まっすぐが先」)。
pub(crate) fn create_line(stations: &[u32]) -> Command {
    Command::CreateLine {
        stations: stations.iter().map(|&s| StationId(s)).collect(),
        bends: vec![Bend::StraightFirst; stations.len() - 1],
    }
}

/// 最初の 3 駅をつなぐ路線を 1 本作ったゲームの画面。
pub(crate) fn harness_with_line() -> UiHarness {
    let mut h = UiHarness::with_game(plan(), false);
    let g = h.game.as_mut().expect("ゲーム");
    let cmd = create_line(&[0, 1, 2]);
    g.validate(&cmd).expect("最初の路線は作れる");
    g.enqueue(cmd);
    h.frames(2);
    h
}

/// コストを 1 円・倍率 1 にし、所持金を多くした表(終盤の規模にすぐ届かせる)。
pub(crate) fn cheap_game() -> Game {
    cheap_game_with(|_| {})
}

/// `cheap_game` のゲージのしきい値を 10% にした表(未接続の駅を残したまま会社レベルを上げる)。
pub(crate) fn cheap_fast_game() -> Game {
    cheap_game_with(|s| s.company.gauge_num = 1)
}

fn cheap_game_with(edit: impl FnOnce(&mut tm_core::BalanceSpec)) -> Game {
    cheap_game_on(plan(), edit)
}

/// `cheap_game_with` の駅の位置を `plan` にしたもの。
pub(crate) fn cheap_game_on(
    plan: Vec<GridPos>,
    edit: impl FnOnce(&mut tm_core::BalanceSpec),
) -> Game {
    let mut spec = BalanceTable::builtin().spec().clone();
    for p in &mut spec.params {
        p.base_yen = 1;
        p.cost_ratio = 1;
        p.cost_steps.clear();
    }
    spec.economy.initial_money_yen = 1_000_000_000_000_000;
    spec.economy.track.c1_yen = 1;
    spec.economy.track.level_ratio = 1;
    spec.economy.track.tile_ratio = (1, 1);
    edit(&mut spec);
    let table = BalanceTable::from_spec(spec).expect("検証を通る表");
    Game::with_station_plan(
        GameConfig {
            seed: 7,
            balance: Arc::new(table),
        },
        plan,
    )
}

/// 買えるものをすべて MAX で積む(R2 は会社、駅は「ぜんぶ」、ほかは「ぜんぶの路線」)。
pub(crate) fn buy_all(h: &mut UiHarness) {
    let g = h.game.as_mut().expect("ゲーム");
    for p in ParamKey::ALL {
        let scope = match p {
            ParamKey::S1Spawn
            | ParamKey::S2Fare
            | ParamKey::S3StationCap
            | ParamKey::S4Platform => UpgradeScope::AllStations,
            ParamKey::R2LineSlots => UpgradeScope::Company,
            _ => UpgradeScope::AllLines,
        };
        let cmd = Command::LevelUp {
            param: p,
            scope,
            mode: BuyMode::Max,
        };
        if g.validate(&cmd).is_ok() {
            g.enqueue(cmd);
        }
    }
}

/// 安い表のゲームで最初の路線を作り、会社レベルを `level` 以上にした画面。
pub(crate) fn cheap_with_company(level: u8) -> UiHarness {
    let mut h = UiHarness::from_game(cheap_fast_game(), false);
    h.game
        .as_mut()
        .expect("ゲーム")
        .enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    for _ in 0..60 {
        if h.game.as_ref().expect("ゲーム").view().company().level.0 >= level {
            break;
        }
        buy_all(&mut h);
        h.frames(2);
    }
    assert!(h.game.as_ref().expect("ゲーム").view().company().level.0 >= level);
    h
}
