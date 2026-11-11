//! リプレイ(10 §9.1): 記録 → RON → `replay` で再生 → state_hash が一致する。v2 の `start`(続きから)も。

#[allow(dead_code, reason = "テストのファイルごとに使う補助が違う")]
mod common;

use common::{builtin, driver_recommend, record};
use tm_core::{
    Bend, CHECKPOINT_INTERVAL_TICKS, Checkpoint, Command, ReplayEntry, ReplayError, ReplayLog,
    SnapshotError, StationId, Tick, replay,
};

const TICKS: u64 = 3000;

fn session() -> (tm_core::Game, ReplayLog) {
    let (g, log, _) = record(21, &builtin(), TICKS, driver_recommend);
    assert!(
        log.entries.len() >= 3,
        "コマンドがある: {}",
        log.entries.len()
    );
    assert_eq!(
        log.checkpoints.len() as u64,
        TICKS / CHECKPOINT_INTERVAL_TICKS
    );
    (g, log)
}

#[test]
fn replay_reproduces_the_session() {
    let (g, log) = session();
    let text = log.to_ron();
    let back = ReplayLog::from_ron(&text).unwrap();
    assert_eq!(back, log);
    let mut events = Vec::new();
    let r = replay(&back, builtin(), Tick(TICKS), &mut events).unwrap();
    assert_eq!(r.now(), Tick(TICKS));
    assert_eq!(r.state_hash(), g.state_hash());
    assert!(!events.is_empty());

    let mut again = common::new_game(log.header.seed, &builtin());
    let mut direct = Vec::new();
    let mut entries = log.entries.iter().peekable();
    while again.now().0 < TICKS {
        let t = again.next_tick();
        while let Some(e) = entries.next_if(|e| e.tick == t) {
            again.enqueue(e.cmd.clone());
        }
        again.step(&mut direct);
    }
    assert_eq!(events, direct, "イベント列も同じ");
}

#[test]
fn replay_stops_at_until_and_ignores_later_checkpoints() {
    let (_, mut log) = session();
    let mid = Tick(1500);
    let r = replay(&log, builtin(), mid, &mut Vec::new()).unwrap();
    assert_eq!(r.now(), mid);
    let at_mid = log
        .checkpoints
        .iter()
        .find(|c| c.tick == Tick(1200))
        .unwrap()
        .state_hash;
    let r2 = replay(&log, builtin(), Tick(1200), &mut Vec::new()).unwrap();
    assert_eq!(r2.state_hash(), at_mid);
    log.checkpoints.last_mut().unwrap().state_hash ^= 1;
    assert!(replay(&log, builtin(), mid, &mut Vec::new()).is_ok());
}

#[test]
fn checkpoint_records_now_and_hash() {
    let g = common::new_game(3, &builtin());
    let mut log = ReplayLog::new(common::header(3, &builtin()));
    log.checkpoint(&g);
    assert_eq!(
        log.checkpoints,
        [Checkpoint {
            tick: Tick(0),
            state_hash: g.state_hash()
        }]
    );
    assert!(replay(&log, builtin(), Tick(0), &mut Vec::new()).is_ok());
    log.checkpoints[0].state_hash ^= 1;
    assert!(matches!(
        replay(&log, builtin(), Tick(0), &mut Vec::new()),
        Err(ReplayError::Diverged { tick: Tick(0), .. })
    ));
}

#[test]
fn divergence_is_reported_at_the_checkpoint() {
    let (_, log) = session();
    let mut bad = log.clone();
    let cp = bad.checkpoints[2];
    bad.checkpoints[2].state_hash ^= 0xff;
    assert_eq!(
        replay(&bad, builtin(), Tick(TICKS), &mut Vec::new()).map(|_| ()),
        Err(ReplayError::Diverged {
            tick: cp.tick,
            expected: cp.state_hash ^ 0xff,
            actual: cp.state_hash
        })
    );

    let mut changed = log.clone();
    assert!(matches!(changed.entries[0].cmd, Command::CreateLine { .. }));
    changed.entries[0] = ReplayEntry {
        tick: Tick(1),
        cmd: Command::CreateLine {
            stations: vec![StationId(0), StationId(1)],
            bends: vec![Bend::StraightFirst],
        },
    };
    assert!(matches!(
        replay(&changed, builtin(), Tick(TICKS), &mut Vec::new()),
        Err(ReplayError::Diverged {
            tick: Tick(600),
            ..
        })
    ));

    let mut other_seed = log.clone();
    other_seed.header.seed += 1;
    assert!(matches!(
        replay(&other_seed, builtin(), Tick(TICKS), &mut Vec::new()),
        Err(ReplayError::Diverged {
            tick: Tick(600),
            ..
        })
    ));
}

#[test]
fn header_and_order_are_checked() {
    let (_, log) = session();
    let check = |edit: fn(&mut ReplayLog)| {
        let mut l = log.clone();
        edit(&mut l);
        replay(&l, builtin(), Tick(TICKS), &mut Vec::new()).map(|_| ())
    };
    assert_eq!(
        check(|l| l.header.format += 1),
        Err(ReplayError::VersionMismatch { field: "format" })
    );
    assert_eq!(
        check(|l| l.header.rules_version += 1),
        Err(ReplayError::VersionMismatch {
            field: "rules_version"
        })
    );
    assert_eq!(
        check(|l| l.header.ticks_per_sec = 30),
        Err(ReplayError::VersionMismatch {
            field: "ticks_per_sec"
        })
    );
    let actual = builtin().config_hash();
    assert_eq!(
        check(|l| l.header.config_hash ^= 1),
        Err(ReplayError::ConfigMismatch {
            expected: actual ^ 1,
            actual
        })
    );
    assert!(matches!(
        replay(&log, common::rich(), Tick(TICKS), &mut Vec::new()),
        Err(ReplayError::ConfigMismatch { .. })
    ));
    assert_eq!(
        check(|l| l.entries[2].tick = Tick(0)),
        Err(ReplayError::UnorderedEntries { index: 2 })
    );
    assert_eq!(
        check(|l| l.entries[0].tick = Tick(0)),
        Err(ReplayError::UnorderedEntries { index: 0 })
    );
    assert_eq!(
        check(|l| l.checkpoints.swap(0, 1)),
        Err(ReplayError::UnorderedCheckpoints { index: 1 })
    );
    let mut same_tick = log.clone();
    same_tick.checkpoints.insert(1, same_tick.checkpoints[0]);
    assert!(replay(&same_tick, builtin(), Tick(TICKS), &mut Vec::new()).is_ok());
}

const SAVE_AT: u64 = 1200;

/// 「続きから」のログ(10 §9.1 の v2): `SAVE_AT` の写しから、その後のエントリとチェックポイントで再生する。
fn resumed() -> (tm_core::Game, ReplayLog, Vec<tm_core::GameEvent>) {
    let (whole, log) = session();
    let (at_save, _, _) = record(21, &builtin(), SAVE_AT, driver_recommend);
    let mut resumed = ReplayLog::new(log.header.clone());
    resumed.start = Some(at_save.snapshot());
    resumed.entries = log
        .entries
        .iter()
        .filter(|e| e.tick.0 > SAVE_AT)
        .cloned()
        .collect();
    resumed.checkpoints = log
        .checkpoints
        .iter()
        .filter(|c| c.tick.0 >= SAVE_AT)
        .copied()
        .collect();
    assert!(!resumed.entries.is_empty());
    let mut direct = Vec::new();
    let mut g = at_save;
    let mut entries = resumed.entries.iter().peekable();
    while g.now().0 < TICKS {
        let t = g.next_tick();
        while let Some(e) = entries.next_if(|e| e.tick == t) {
            g.enqueue(e.cmd.clone());
        }
        g.step(&mut direct);
    }
    assert_eq!(g.state_hash(), whole.state_hash());
    (whole, resumed, direct)
}

#[test]
fn resumed_log_replays_from_the_start_snapshot() {
    let (whole, log, direct) = resumed();
    let back = ReplayLog::from_ron(&log.to_ron()).unwrap();
    assert_eq!(back, log);
    let mut events = Vec::new();
    let r = replay(&back, builtin(), Tick(TICKS), &mut events).unwrap();
    assert_eq!(r.now(), Tick(TICKS));
    assert_eq!(r.state_hash(), whole.state_hash());
    assert_eq!(events, direct, "写しの後のイベント列");

    let early = replay(&log, builtin(), Tick(SAVE_AT - 1), &mut Vec::new()).unwrap();
    assert_eq!(
        early.now(),
        Tick(SAVE_AT),
        "写しより前の until は写しの状態"
    );

    let mut before = log.clone();
    before.checkpoints.insert(
        0,
        Checkpoint {
            tick: Tick(SAVE_AT - 600),
            state_hash: 0,
        },
    );
    assert!(
        replay(&before, builtin(), Tick(TICKS), &mut Vec::new()).is_ok(),
        "写しより前のチェックポイントは見ない"
    );

    let mut bad = log.clone();
    bad.checkpoints[1].state_hash ^= 1;
    assert!(matches!(
        replay(&bad, builtin(), Tick(TICKS), &mut Vec::new()),
        Err(ReplayError::Diverged { .. })
    ));
}

#[test]
fn start_snapshot_is_checked() {
    let (_, log, _) = resumed();
    let check = |edit: fn(&mut ReplayLog)| {
        let mut l = log.clone();
        edit(&mut l);
        replay(&l, builtin(), Tick(TICKS), &mut Vec::new()).map(|_| ())
    };
    assert!(matches!(
        check(|l| l.header.seed += 1),
        Err(ReplayError::Snapshot(SnapshotError::Malformed { .. }))
    ));
    assert_eq!(
        check(|l| l.entries[0].tick = Tick(SAVE_AT)),
        Err(ReplayError::UnorderedEntries { index: 0 })
    );
    assert!(!matches!(
        check(|l| l.entries[0].tick = Tick(SAVE_AT + 1)),
        Err(ReplayError::UnorderedEntries { .. })
    ));
    let snap_hash = log.start.as_ref().unwrap().state_hash;
    assert_eq!(
        check(|l| l.start.as_mut().unwrap().state_hash ^= 1),
        Err(ReplayError::Snapshot(SnapshotError::HashMismatch {
            expected: snap_hash ^ 1,
            actual: snap_hash
        }))
    );
    assert!(matches!(
        check(|l| l.start.as_mut().unwrap().config_hash ^= 1),
        Err(ReplayError::Snapshot(SnapshotError::ConfigMismatch { .. }))
    ));
    assert_eq!(
        check(|l| l.header.format = 1),
        Err(ReplayError::VersionMismatch { field: "format" })
    );
}

/// 前の版(M7 の `RULES_VERSION` 2、M8 の 3)のリプレイは再生しない。panic しない。
#[test]
fn previous_rules_replay_is_refused() {
    for (name, rules) in [("replay_rules2.ron", 2), ("replay_rules3.ron", 3)] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(name);
        let log = ReplayLog::from_ron(&std::fs::read_to_string(path).expect("前の版のログ"))
            .expect("形は読める");
        assert_eq!(log.header.rules_version, rules);
        assert_eq!(
            replay(&log, builtin(), Tick(600), &mut Vec::new()).map(|_| ()),
            Err(ReplayError::VersionMismatch {
                field: "rules_version"
            }),
            "{name}"
        );
    }
}
