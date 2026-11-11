//! 11 §11.4 の長時間テスト(ゲーム時間 24 時間 × 2 通り。`#[ignore]`、`mise run test:long` の release-check で回す)。
//! 不変条件は `step` が毎 tick 検査する(debug-assertions)。ここでは毎秒、要素数・保存・「減らせない」を見て、
//! 1 時間ごとに写し → RON → 作り直しを元と同じ入力で 1 時間進めて `state_hash` を比べる。

use std::sync::{Arc, Mutex};
use std::time::Instant;

use tm_core::{BalanceTable, Checkpoint, Game, GameSnapshot, ReplayLog, replay};
use tm_simrun::check::{self, Checker};
use tm_simrun::scenario::{Action, TPS, replay_header};
use tm_simrun::{Idle, Pause, Player, Runner, Scenario, csv_header};

const SEED_A: u64 = 1;
const HOUR: u64 = 3600 * TPS;
const HOURS: u64 = 24;
/// 状態が頭打ちになった後の常駐メモリの増加の許容(11 §10.2)。(a) は 1 時間目から、ゴールまで遊ぶ (b) は
/// 放置に入ってから測る(遊んでいる間は駅・列車・経路表とテストが持つコマンドの記録が正当に増える)。
const RSS_TOLERANCE_KIB: u64 = 512;

static SERIAL: Mutex<()> = Mutex::new(());

/// 写しを RON に書いて読み、作り直す。
fn restore_via_ron(bal: &Arc<BalanceTable>, snap: &GameSnapshot) -> Game {
    let text = ron::to_string(snap).expect("RON に書ける");
    let back: GameSnapshot = ron::from_str(&text).expect("RON を読める");
    assert_eq!(&back, snap);
    Game::restore(Arc::clone(bal), &back).expect("作り直せる")
}

/// 写しから作り直したゲームを、元が積んだコマンド(tick が `until` 以下)で `until` まで進める。
fn follow(mut copy: Game, actions: &[Action], until: u64) -> Game {
    let start = copy.now();
    let mut it = actions.iter().filter(|a| a.tick > start).peekable();
    let mut events = Vec::new();
    while copy.now().0 < until {
        let t = copy.next_tick();
        while let Some(a) = it.next_if(|a| a.tick == t) {
            copy.enqueue(a.cmd.clone());
        }
        events.clear();
        copy.step(&mut events);
    }
    copy
}

fn long_run(name: &str, scenario: Scenario, idle_after_goal: bool) {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let bal = Arc::new(BalanceTable::builtin());
    let started = Instant::now();
    let mut r = Runner::new(SEED_A, Arc::clone(&bal), scenario);
    let mut checker = Checker::new(r.game());
    let mut rss = Vec::with_capacity(HOURS as usize);
    let mut steady_from = None;
    let mut hourly = Vec::with_capacity(HOURS as usize);
    let mut pending: Option<GameSnapshot> = None;
    let mut copies = 0;
    let mut copy_secs = 0.0;
    while r.game().now().0 < HOURS * HOUR {
        let mut snap = None;
        r.tick_observing(|g| {
            if g.now().0.is_multiple_of(HOUR) {
                snap = Some(g.snapshot());
            }
        });
        let now = r.game().now();
        if now.is_second_boundary() {
            checker
                .check(r.game())
                .unwrap_or_else(|e| panic!("{name}: tick {}: {e}", now.0));
        }
        if let Some(s) = snap {
            if let Some(prev) = pending.take() {
                let t = Instant::now();
                let copy = follow(restore_via_ron(&bal, &prev), r.actions(), now.0);
                assert_eq!(
                    copy.state_hash(),
                    r.game().state_hash(),
                    "{name}: tick {}",
                    now.0
                );
                copies += 1;
                copy_secs += t.elapsed().as_secs_f64();
            }
            pending = Some(s);
            if now.0 >= HOUR {
                if steady_from.is_none() && (!idle_after_goal || r.scenario().is_idle(u64::MAX)) {
                    steady_from = Some(rss.len());
                }
                rss.extend(check::rss_kib());
                hourly.push(Checkpoint {
                    tick: now,
                    state_hash: r.game().state_hash(),
                });
            }
        }
        if idle_after_goal && r.milestones().goal.is_some() && !r.scenario().is_idle(u64::MAX) {
            r.idle_from(u32::try_from(r.wall() / TPS).expect("秒は u32"));
        }
    }
    let run_secs = started.elapsed().as_secs_f64();
    let report = r.report();
    let g = report.game.as_ref().expect("実際のゲーム");
    assert_eq!(checker.checks, HOURS * 3600);
    assert_eq!(copies, HOURS - 1);
    assert!(
        !idle_after_goal || report.milestones.goal.is_some(),
        "{}",
        report.csv_row()
    );
    let steady = &rss[steady_from.unwrap_or(rss.len()).min(rss.len())..];
    assert!(steady.len() >= 2 || check::rss_kib().is_none(), "{rss:?}");
    assert!(g.ledger.is_conserved());
    assert_eq!(g.rejected, 0);
    check::check_growth(steady, RSS_TOLERANCE_KIB).unwrap_or_else(|e| panic!("{name}: {e}"));

    let replay_started = Instant::now();
    let mut log = ReplayLog::new(replay_header(SEED_A, &bal));
    for a in r.actions() {
        log.record(a.tick, &a.cmd);
    }
    log.checkpoints = hourly;
    let end = r.game().now();
    let again = replay(&log, bal, end, &mut Vec::new()).expect("毎時の state_hash が一致");
    assert_eq!(again.state_hash(), r.game().state_hash());
    let replay_secs = replay_started.elapsed().as_secs_f64();

    let max_level = r.game().view().company().max_level.0;
    eprintln!(
        "{name}: {}\n{name}: {}",
        csv_header(max_level),
        report.csv_row()
    );
    eprintln!(
        "{name}: VS-04 waiting_cap_loss {} / delivered {} (spawned {}, capacity_loss {}), waiting_peak_ratio {:.4}",
        g.ledger.waiting_cap_loss,
        g.ledger.delivered,
        g.ledger.spawned,
        g.ledger.capacity_loss,
        g.waiting_peak_ratio
    );
    eprintln!(
        "{name}: VS-08 {} tick: {run_secs:.1} s(うち写しの 1 時間 × {copies}: {copy_secs:.1} s。毎 tick の不変条件 {}、毎秒の検査 {} 回)、再生 {replay_secs:.1} s、RSS {rss:?} KiB(1 時間目からの増加 {} KiB、頭打ちの後 {} 個で検査)",
        end.0,
        check::STEP_CHECKS_INVARIANTS,
        checker.checks,
        rss.iter().max().zip(rss.first()).map_or(0, |(m, f)| m - f),
        steady.len()
    );
}

#[test]
#[ignore = "24 時間分。mise run test:long(release-check)で回す"]
fn long_run_a_first_line_then_idle() {
    let scenario = Scenario {
        idle: vec![Idle::after(0)],
        ..Scenario::new(Player::B, Pause::NONE)
    };
    long_run("(a)", scenario, false);
}

#[test]
#[ignore = "24 時間分。mise run test:long(release-check)で回す"]
fn long_run_b_plays_to_goal_then_idle() {
    long_run("(b)", Scenario::new(Player::B, Pause::NONE), true);
}
