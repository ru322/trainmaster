//! シナリオのゴールデン(11 §11.3): 600 tick ごとの state_hash とイベント列のハッシュ。ログは `tests/golden/` のリプレイ。
//! ルールを変えたら x86_64 で `TM_UPDATE_GOLDEN=1 cargo test -p tm-core --test golden_scenarios` を実行して作り直す。
//! `recommend` は台本を持たず、`tm-simrun record --player b --pause none --seed 2 --minutes 10 --out crates/tm-core/tests/golden/scenario_recommend` で作る(再生して比べるだけ)。

#[allow(dead_code, reason = "テストのファイルごとに使う補助が違う")]
mod common;

use std::path::PathBuf;
use std::sync::Arc;

use common::{EventHashes, play, record};
use tm_core::{BalanceTable, Command, GameView, ReplayLog, Tick, replay};

struct Scenario {
    name: &'static str,
    seed: u64,
    table: fn() -> Arc<BalanceTable>,
    ticks: u64,
    driver: Option<fn(&GameView<'_>) -> Vec<Command>>,
}

const SCENARIOS: [Scenario; 3] = [
    Scenario {
        name: "idle",
        seed: 1,
        table: common::builtin,
        ticks: 36_000,
        driver: Some(common::driver_idle),
    },
    Scenario {
        name: "recommend",
        seed: 2,
        table: common::builtin,
        ticks: 36_000,
        driver: None,
    },
    Scenario {
        name: "full_build",
        seed: 3,
        table: common::cheap,
        ticks: 7_200,
        driver: Some(common::driver_full_build),
    },
];

fn path(name: &str, ext: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("scenario_{name}.{ext}"))
}

fn events_text(h: &EventHashes) -> String {
    h.iter()
        .map(|(t, v)| format!("{} {v:016x}\n", t.0))
        .collect()
}

fn parse_events(text: &str) -> EventHashes {
    text.lines()
        .map(|l| {
            let (t, v) = l.split_once(' ').expect("「tick ハッシュ」の行");
            (
                Tick(t.parse().expect("tick")),
                u64::from_str_radix(v, 16).expect("16 進"),
            )
        })
        .collect()
}

fn check(sc: &Scenario) {
    let bal = (sc.table)();
    let (log_path, ev_path) = (path(sc.name, "ron"), path(sc.name, "events"));
    if let Some(driver) = sc.driver
        && std::env::var_os("TM_UPDATE_GOLDEN").is_some_and(|v| v == "1")
    {
        let (_, log, hashes) = record(sc.seed, &bal, sc.ticks, driver);
        let pretty = ron::ser::PrettyConfig::new().depth_limit(2);
        let text = ron::ser::to_string_pretty(&log, pretty).expect("RON に書ける");
        std::fs::write(&log_path, text + "\n").expect("ログを書く");
        std::fs::write(&ev_path, events_text(&hashes)).expect("ハッシュを書く");
    }
    let text = std::fs::read_to_string(&log_path)
        .unwrap_or_else(|_| panic!("{log_path:?} がない。TM_UPDATE_GOLDEN=1 で作る"));
    let log = ReplayLog::from_ron(&text).expect("ゴールデンのログ");
    let want_events = parse_events(&std::fs::read_to_string(&ev_path).expect("イベントのハッシュ"));
    assert_eq!(log.header.seed, sc.seed);
    assert_eq!(log.checkpoints.len() as u64, sc.ticks / 600);

    let (g, states, events) = play(&log, &bal, sc.ticks);
    let want_states: Vec<(Tick, u64)> = log
        .checkpoints
        .iter()
        .map(|c| (c.tick, c.state_hash))
        .collect();
    assert_eq!(states, want_states, "{}: state_hash", sc.name);
    assert_eq!(events, want_events, "{}: イベント列", sc.name);

    let r = replay(&log, Arc::clone(&bal), Tick(sc.ticks), &mut Vec::new()).expect("再生できる");
    assert_eq!(r.state_hash(), g.state_hash());

    if let Some(driver) = sc.driver {
        let (_, again, _) = record(sc.seed, &bal, sc.ticks, driver);
        assert_eq!(again.entries, log.entries, "{}: 台本のコマンド列", sc.name);
    }
}

#[test]
fn scenario_idle() {
    check(&SCENARIOS[0]);
}

#[test]
fn scenario_recommend() {
    check(&SCENARIOS[1]);
}

#[test]
fn scenario_full_build() {
    check(&SCENARIOS[2]);
}
