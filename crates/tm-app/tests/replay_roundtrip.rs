//! APP-T-13: 記録したリプレイを再生すると、同じ `state_hash` になる(16 §2.4)。

mod common;

use std::sync::Arc;

use common::{Rig, SEED_A, config, memory, with_memory};
use tm_app::{SeedPolicy, ShutdownReason};
use tm_core::{
    BalanceTable, CHECKPOINT_INTERVAL_TICKS, Command, LineId, ReplayLog, StationId, TICKS_PER_SEC,
    Tick, replay,
};
use tm_platform_api::HeadlessClock;

const PLAY_TICKS: u64 = 10_800;

/// 路線を作り、のばし、強化する台本。
fn play(r: &mut Rig) {
    r.build_extend_upgrade();
    let t = r.tick();
    r.frames(PLAY_TICKS - t);
    assert_eq!(r.tick(), PLAY_TICKS);
}

#[test]
fn app_t_13_replay_roundtrip() {
    let mem = memory();
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        with_memory(config(SeedPolicy::Fixed(SEED_A)), &mem),
    );
    play(&mut r);
    let hash = r.game().state_hash();
    let cfg_hash = r.game().config().balance.config_hash();
    r.app.shutdown(ShutdownReason::Quit);

    let logs = mem.lock().unwrap();
    assert_eq!(logs.len(), 1, "終了で 1 本書き出す");
    let log = ReplayLog::from_ron(&logs[0].to_ron()).expect("RON の往復");
    assert_eq!(log, logs[0]);
    assert_eq!(log.header.seed, SEED_A);
    assert_eq!(log.header.config_hash, cfg_hash);
    assert_eq!(log.header.ticks_per_sec, TICKS_PER_SEC);
    let kinds: Vec<_> = log
        .entries
        .iter()
        .map(|e| std::mem::discriminant(&e.cmd))
        .collect();
    for want in [
        Command::CreateLine {
            stations: vec![],
            bends: vec![],
        },
        Command::ExtendLine {
            line: LineId(0),
            end: tm_core::LineEnd::Head,
            station: StationId(0),
            bend: tm_core::Bend::StraightFirst,
        },
    ] {
        assert!(
            kinds.contains(&std::mem::discriminant(&want)),
            "{:?}",
            log.entries
        );
    }
    assert!(
        log.entries
            .iter()
            .any(|e| matches!(e.cmd, Command::LevelUp { .. })),
        "強化: {:?}",
        log.entries
    );
    let ticks: Vec<u64> = log.checkpoints.iter().map(|c| c.tick.0).collect();
    let want: Vec<u64> = (1..=PLAY_TICKS / CHECKPOINT_INTERVAL_TICKS)
        .map(|i| i * CHECKPOINT_INTERVAL_TICKS)
        .collect();
    assert_eq!(ticks, want, "600 tick ごと");

    let mut events = Vec::new();
    let game = replay(
        &log,
        Arc::new(BalanceTable::builtin()),
        Tick(PLAY_TICKS),
        &mut events,
    )
    .expect("再生");
    assert_eq!(game.now(), Tick(PLAY_TICKS));
    assert_eq!(game.state_hash(), hash);
    r.assert_audio_ok();
}

#[test]
fn checkpoints_every_600_ticks_with_several_ticks_per_frame() {
    let mem = memory();
    let mut cfg = with_memory(config(SeedPolicy::Fixed(SEED_A)), &mem);
    cfg.time_scale = 7;
    let mut r = Rig::new(HeadlessClock::Exact60, cfg);
    r.start_game();
    r.disable_explanations();
    while r.tick() < 3_000 {
        let t = r.tick();
        r.frame();
        assert_eq!(r.tick(), t + 7, "倍率 7 で 1 フレーム 7 tick");
    }
    let log = r.app.replay().expect("記録している");
    let ticks: Vec<u64> = log.checkpoints.iter().map(|c| c.tick.0).collect();
    assert_eq!(ticks, [600, 1_200, 1_800, 2_400, 3_000]);
    let hash = r.game().state_hash();
    let until = Tick(r.tick());
    let mut events = Vec::new();
    let g = replay(log, Arc::new(BalanceTable::builtin()), until, &mut events).expect("再生");
    assert_eq!(g.state_hash(), hash);
}

#[test]
fn replay_off_records_nothing() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.start_game();
    assert!(r.app.replay().is_none());
    r.app.shutdown(ShutdownReason::Quit);
    assert_eq!(r.app.stats().replays_written, 0);
}
