use super::*;
use crate::balance::BalanceSpec;
use crate::command::{Command, LineEnd};
use crate::event::GameEvent;
use crate::param::{BuyMode, ParamOwner, UpgradeScope};

const SEED_A: u64 = 1;
const SEED_B: u64 = 42;

fn table(edit: impl FnOnce(&mut BalanceSpec)) -> Arc<BalanceTable> {
    let mut spec = BalanceTable::builtin().spec().clone();
    edit(&mut spec);
    Arc::new(BalanceTable::from_spec(spec).expect("検証を通る表"))
}

/// コストと線路の値段を 1 円にし、所持金を最大にした表。ゴールは `10^goal_pow10`。
fn cheap(goal_pow10: u32) -> Arc<BalanceTable> {
    table(|s| {
        for p in &mut s.params {
            p.base_yen = 1;
            p.cost_ratio = 1;
            p.cost_steps.clear();
        }
        s.economy.initial_money_yen = u64::MAX;
        s.economy.goal_pow10 = goal_pow10;
        s.economy.track.c1_yen = 1;
        s.economy.track.level_ratio = 1;
        s.economy.track.tile_ratio = (1, 1);
    })
}

fn game(seed: u64, balance: Arc<BalanceTable>) -> Game {
    Game::new(GameConfig { seed, balance })
}

fn run(g: &mut Game, ticks: u64) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        g.step(&mut events);
    }
    events
}

fn apply(g: &mut Game, cmds: Vec<Command>) {
    cmds.into_iter().for_each(|c| g.enqueue(c));
    run(g, 1);
}

fn create(g: &Game, ids: &[u32]) -> Command {
    let v = g.view();
    let stations: Vec<StationId> = ids.iter().map(|&i| StationId(i)).collect();
    let bends = stations
        .windows(2)
        .map(|w| v.default_bend(w[0], w[1]))
        .collect();
    Command::CreateLine { stations, bends }
}

fn buy_max(p: ParamKey, scope: UpgradeScope) -> Command {
    Command::LevelUp {
        param: p,
        scope,
        mode: BuyMode::Max,
    }
}

/// S4 は駅 0・1 だけ、ほかは全体を MAX で買う(新しい駅のホームは Lv0 のまま)。
fn boost(g: &mut Game) {
    let mut cmds = Vec::new();
    for p in ParamKey::ALL {
        match (p, p.owner()) {
            (ParamKey::S4Platform, _) => {
                for s in [0, 1] {
                    cmds.push(buy_max(p, UpgradeScope::Station(StationId(s))));
                }
            }
            (_, ParamOwner::Station) => cmds.push(buy_max(p, UpgradeScope::AllStations)),
            (_, ParamOwner::Line) => cmds.push(buy_max(p, UpgradeScope::AllLines)),
            (_, ParamOwner::Company) => cmds.push(buy_max(p, UpgradeScope::Company)),
        }
    }
    apply(g, cmds);
}

/// 路線・列車・乗客と、1 回の強化のある序盤の状態(組み込みの表。所持金 < 累計収入)。
fn early() -> Game {
    let mut g = game(SEED_A, Arc::new(BalanceTable::builtin()));
    let c = create(&g, &[0, 1, 2]);
    apply(&mut g, vec![c]);
    let buy = Command::LevelUp {
        param: ParamKey::S1Spawn,
        scope: UpgradeScope::Station(StationId(1)),
        mode: BuyMode::One,
    };
    while g.validate(&buy).is_err() {
        assert!(g.now().0 < 20_000, "強化を買えない");
        run(&mut g, 60);
    }
    apply(&mut g, vec![buy]);
    run(&mut g, 300);
    let st = &g.state;
    assert!(st.trains.iter().any(|t| t.riders.0 > 0));
    assert_eq!(st.stations[1].levels.spawn, Level(1));
    assert_ne!(st.money, st.progress.total_earned);
    g
}

/// 通過駅・停車中の列車・乗り換え・2 本の路線がある状態。未接続の駅を残して会社レベルを上げるためゲージは 30%。
fn transit() -> Game {
    let bal = table(|s| {
        *s = cheap(68).spec().clone();
        s.company.gauge_num = 3;
    });
    let mut g = game(SEED_B, bal);
    let c = create(&g, &[0, 1]);
    apply(&mut g, vec![c]);
    for _ in 0..12 {
        boost(&mut g);
    }
    let free: Vec<StationId> = g
        .view()
        .stations()
        .filter(|s| !s.connected)
        .map(|s| s.id)
        .collect();
    assert!(free.len() >= 2, "つないでいない駅 {free:?}");
    let ext = Command::ExtendLine {
        line: LineId(0),
        end: LineEnd::Tail,
        station: free[0],
        bend: g.view().default_bend(StationId(1), free[0]),
    };
    let second = create(&g, &[0, free[1].0]);
    apply(&mut g, vec![ext, second]);
    run(&mut g, 700);
    let st = &g.state;
    assert_eq!(st.lines.len(), 2);
    assert!(st.lines[0].stops.contains(&false), "通過駅がある");
    assert!(
        st.trains
            .iter()
            .any(|t| matches!(t.phase, TrainPhase::Dwelling { .. })),
        "停車中の列車がある"
    );
    assert!(st.waiting.as_slice().iter().any(|&n| n > 0));
    g
}

/// ゴールの後の状態。
fn after_goal() -> Game {
    let mut g = game(SEED_A, cheap(4));
    let c = create(&g, &[0, 1, 2]);
    apply(&mut g, vec![c]);
    while !g.state.progress.goal_reached {
        assert!(g.now().0 < 30_000, "ゴールに届かない");
        boost(&mut g);
        run(&mut g, 59);
    }
    g
}

fn same_state(a: &Game, b: &Game) {
    let sa = GameState {
        #[cfg(feature = "test-support")]
        ledger: b.state.ledger,
        ..a.state.clone()
    };
    assert_eq!(sa, b.state);
}

/// 写し → RON → 読み → restore。状態(台帳を除く)と state_hash が一致する。
fn round_trip(g: &Game) -> Game {
    let snap = g.snapshot();
    let text = ron::to_string(&snap).expect("RON に書ける");
    let back: GameSnapshot = ron::from_str(&text).expect("RON を読める");
    assert_eq!(back, snap);
    let r = Game::restore(Arc::clone(&g.config.balance), &back).expect("restore できる");
    assert_eq!(r.state_hash(), g.state_hash());
    assert_eq!(r.config.seed, g.config.seed);
    assert!(r.pending().is_empty());
    same_state(&r, g);
    assert_eq!(r.snapshot(), snap);
    #[cfg(feature = "test-support")]
    assert!(r.pax_ledger().is_conserved(), "{:?}", r.pax_ledger());
    r
}

/// 写しから作り直したものが、続けて動かしたものと同じイベント列と state_hash で 600 tick 進む。
fn continues_identically(mut g: Game) {
    let mut r = round_trip(&g);
    let (mut eg, mut er) = (Vec::new(), Vec::new());
    for _ in 0..600 {
        eg.clear();
        er.clear();
        g.step(&mut eg);
        r.step(&mut er);
        assert_eq!(er, eg, "tick {}", g.now().0);
        assert_eq!(r.state_hash(), g.state_hash(), "tick {}", g.now().0);
    }
    same_state(&r, &g);
}

#[test]
fn initial_state_round_trips() {
    continues_identically(game(SEED_A, Arc::new(BalanceTable::builtin())));
}

#[test]
fn reincarnation_state_round_trips() {
    let bal = Arc::new(BalanceTable::builtin());
    let rules = Arc::new(ReincarnationRules::builtin());
    let mut meta = ReincarnationState::new(rules.node_count());
    meta.souls = 7;
    meta.lifetime_souls = 11;
    meta.spent_souls = 4;
    meta.rebirths = 2;
    meta.raise(rules.node_id("seed_capital").unwrap());
    let g = Game::new_with_reincarnation_state(
        GameConfig {
            seed: SEED_A,
            balance: Arc::clone(&bal),
        },
        Arc::clone(&rules),
        meta.clone(),
    );
    let snap = g.snapshot();
    assert!(!snap.reincarnation.is_empty());
    assert_eq!(snap.reincarnation_hash, meta.hash());
    assert_eq!(snap.reincarnation_config_hash, rules.config_hash());
    let back = Game::restore_with_reincarnation(Arc::clone(&bal), Arc::clone(&rules), &snap)
        .expect("転生の状態も作り直せる");
    assert_eq!(back.reincarnation(), &meta);
    assert_eq!(back.state_hash(), g.state_hash());
    assert_eq!(back.state.bonuses.start_money_yen, 1000);

    let mut bad = snap.clone();
    bad.reincarnation_hash ^= 1;
    assert!(matches!(
        Game::restore_with_reincarnation(Arc::clone(&bal), Arc::clone(&rules), &bad),
        Err(SnapshotError::ReincarnationHashMismatch { .. })
    ));

    let mut spec = rules.spec().clone();
    spec.gain.base += 1;
    let other = Arc::new(ReincarnationRules::from_spec(spec).unwrap());
    assert!(matches!(
        Game::restore_with_reincarnation(bal, other, &snap),
        Err(SnapshotError::ReincarnationConfigMismatch { .. })
    ));
}

#[test]
fn early_state_round_trips() {
    continues_identically(early());
}

#[test]
fn transit_state_round_trips() {
    continues_identically(transit());
}

#[test]
fn state_after_goal_round_trips() {
    let g = after_goal();
    assert!(g.state.progress.goal_reached);
    continues_identically(g);
}

#[test]
fn delivered_is_split_into_two_words() {
    let mut g = early();
    g.state.stats.delivered = (7u128 << 64) | 5;
    let snap = g.snapshot();
    assert_eq!((snap.stats.delivered_lo, snap.stats.delivered_hi), (5, 7));
    assert_eq!(round_trip(&g).state.stats.delivered, (7u128 << 64) | 5);
}

#[test]
fn snapshot_writes_the_state_hash_fields() {
    let g = transit();
    let s = g.snapshot();
    let st = &g.state;
    assert_eq!(
        (s.format, s.rules_version, s.seed, s.tick),
        (SNAPSHOT_FORMAT_VERSION, RULES_VERSION, SEED_B, st.tick.0)
    );
    assert_eq!(s.config_hash, g.config.balance.config_hash());
    assert_eq!(s.state_hash, g.state_hash());
    assert_eq!(s.money_bits, st.money.to_bits());
    assert_eq!(s.station_plan.len(), st.station_plan.len());
    assert_eq!(s.lines[0].bends.len(), st.lines[0].segments.len());
    assert_eq!(s.waiting.data, st.waiting.as_slice());
    assert_eq!((s.onboard.rows, s.onboard.cols), (st.onboard.rows(), 27));
    assert_eq!(s.passenger_dest_words, st.rng.passenger_dest.words());
    assert!(s.passenger_dest_words > 0);
}

#[test]
#[should_panic(expected = "キューが空")]
fn snapshot_panics_when_commands_are_queued() {
    let mut g = early();
    let c = create(&g, &[0, 2]);
    g.enqueue(c);
    g.snapshot();
}

fn restore(g: &Game, edit: impl FnOnce(&mut GameSnapshot)) -> Result<Game, SnapshotError> {
    let mut s = g.snapshot();
    edit(&mut s);
    Game::restore(Arc::clone(&g.config.balance), &s)
}

#[test]
fn versions_and_config_are_checked_first() {
    let g = early();
    assert_eq!(
        restore(&g, |s| s.format = 1).map(|_| ()),
        Err(SnapshotError::Format { found: 1 })
    );
    assert_eq!(
        restore(&g, |s| s.rules_version = 2).map(|_| ()),
        Err(SnapshotError::RulesVersion { found: 2 })
    );
    let other = cheap(68);
    let snap = g.snapshot();
    assert_eq!(
        Game::restore(Arc::clone(&other), &snap).map(|_| ()),
        Err(SnapshotError::ConfigMismatch {
            expected: snap.config_hash,
            actual: other.config_hash()
        })
    );
    assert!(matches!(
        restore(&g, |s| {
            s.format = 9;
            s.config_hash ^= 1;
        }),
        Err(SnapshotError::Format { .. })
    ));
}

#[test]
fn state_hash_is_compared_last() {
    let g = early();
    let want = g.state_hash();
    assert_eq!(
        restore(&g, |s| s.state_hash ^= 1).map(|_| ()),
        Err(SnapshotError::HashMismatch {
            expected: want ^ 1,
            actual: want
        })
    );
    type Edit = fn(&mut GameSnapshot);
    let edits: [(&str, Edit); 7] = [
        ("tick", |s| s.tick += 1),
        ("arrivals", |s| s.stats.arrivals += 1),
        ("purchases", |s| s.stats.purchases += 1),
        ("delivered_hi", |s| s.stats.delivered_hi += 1),
        ("words", |s| s.passenger_dest_words += 1),
        ("map_revision", |s| s.map_revision += 1),
        ("created_at", |s| s.lines[0].created_at += 1),
    ];
    for (name, edit) in edits {
        assert!(
            matches!(restore(&g, edit), Err(SnapshotError::HashMismatch { .. })),
            "{name}"
        );
    }
}

fn is_malformed(r: Result<Game, SnapshotError>) -> bool {
    matches!(r, Err(SnapshotError::Malformed { .. }))
}

/// 11 §11.1: 駅の数、範囲外の ID、行列の大きさ、NaN のビット列、範囲外の section などは panic せずに `Malformed`。
#[test]
fn malformed_snapshots_are_rejected() {
    let g = transit();
    type Edit = fn(&mut GameSnapshot);
    let edits: Vec<(&str, Edit)> = vec![
        ("company.level", |s| s.company.level = 25),
        ("stations.pop", |s| {
            s.stations.pop();
        }),
        ("stations.push", |s| s.stations.push(s.stations[0])),
        ("stations.push_row", |s| {
            s.stations.push(s.stations[0]);
            s.waiting.rows += 1;
            s.waiting.data.extend([0; 27]);
        }),
        ("station_plan.pop", |s| {
            s.station_plan.pop();
        }),
        ("station_plan.outside", |s| s.station_plan[26] = (0, 5)),
        ("station_plan.bottom", |s| s.station_plan[26] = (5, 17)),
        ("station_plan.right", |s| s.station_plan[26] = (39, 5)),
        ("station_plan.dup", |s| {
            s.station_plan[26] = s.station_plan[3]
        }),
        ("line_slots", |s| s.company.line_slots = 200),
        ("station.level", |s| s.stations[0].levels[3] = 200),
        ("line.station", |s| s.lines[0].stations[1] = 99),
        ("line.dup", |s| {
            s.lines[0].stations[1] = s.lines[0].stations[0]
        }),
        ("line.short", |s| {
            s.lines[1].stations.truncate(1);
            s.lines[1].bends.clear();
        }),
        ("line.empty", |s| {
            s.lines[1].stations.clear();
            s.lines[1].bends.clear();
        }),
        ("line.bends", |s| {
            s.lines[0].bends.pop();
        }),
        ("line.level", |s| s.lines[0].levels[0] = 200),
        ("line.born_level", |s| {
            s.lines[1].born_level = s.company.level + 1
        }),
        ("lines.max", |s| {
            while s.lines.len() <= 6 {
                s.lines.push(s.lines[1].clone());
            }
        }),
        ("lines.max_with_trains", |s| {
            while s.lines.len() <= 6 {
                let (l, t) = (s.lines.len() as u32, s.trains.len() as u32);
                let mut line = s.lines[1].clone();
                let mut train = s.trains[line.trains[0] as usize];
                line.trains = vec![t];
                train.line = l;
                s.lines.push(line);
                s.trains.push(train);
                s.onboard.rows += 1;
                s.onboard.data.extend([0; 27]);
            }
        }),
        ("line.train", |s| s.lines[0].trains.push(999)),
        ("line.train_twice", |s| {
            let t = s.lines[0].trains[0];
            s.lines[0].trains[1] = t;
        }),
        ("line.train_extra", |s| {
            let t = s.lines[0].trains[0];
            s.lines[0].trains.push(t);
        }),
        ("line.other_train", |s| {
            let t = s.lines[1].trains[0];
            s.lines[0].trains.push(t);
        }),
        ("train.line", |s| s.trains[0].line = 9),
        ("train.unlisted", |s| s.lines[0].trains.clear()),
        ("train.dir", |s| s.trains[0].dir = 2),
        ("train.section", |s| s.trains[0].section = 999),
        ("waiting.rows", |s| s.waiting.rows += 1),
        ("waiting.cols", |s| s.waiting.cols -= 1),
        ("waiting.rows_short", |s| {
            s.waiting.rows -= 1;
            s.waiting.data.truncate(s.waiting.rows as usize * 27);
        }),
        ("waiting.row_sum", |s| {
            s.waiting.data[1] = u64::MAX;
            s.waiting.data[2] = 1;
        }),
        ("onboard.row_sum", |s| {
            s.onboard.data[0] = u64::MAX;
            s.onboard.data[1] = u64::MAX;
        }),
        ("waiting.over_limit", |s| s.waiting.data[1] = u64::MAX / 2),
        ("tick.max", |s| s.tick = MAX_TICK + 1),
        ("words.max", |s| s.passenger_dest_words = MAX_COUNT + 1),
        ("arrivals.max", |s| s.stats.arrivals = MAX_COUNT + 1),
        ("map_revision.max", |s| s.map_revision = MAX_COUNT + 1),
        ("delivered_hi.max", |s| {
            s.stats.delivered_hi = MAX_DELIVERED_HI + 1
        }),
        ("purchases.max", |s| s.stats.purchases = MAX_PURCHASES + 1),
        ("waiting.cols.data", |s| {
            s.waiting.cols = 26;
            s.waiting.data.truncate(s.waiting.rows as usize * 26);
        }),
        ("waiting.data", |s| {
            s.waiting.data.pop();
        }),
        ("onboard.data", |s| s.onboard.data.push(0)),
        ("onboard.rows", |s| {
            s.onboard.rows -= 1;
            s.onboard.data.truncate(s.onboard.data.len() - 27);
        }),
        ("money", |s| s.money_bits = f64::NAN.to_bits()),
        ("total_earned", |s| {
            s.total_earned_bits = (-1.0f64).to_bits()
        }),
        ("income_this_sec", |s| {
            s.stats.income_this_sec_bits = f64::INFINITY.to_bits()
        }),
        ("income_window", |s| {
            s.stats.income_window_bits[9] = f64::NAN.to_bits()
        }),
        ("income_window.len", |s| {
            s.stats.income_window_bits.pop();
        }),
        ("window_head", |s| s.stats.window_head = 10),
    ];
    for (name, edit) in edits {
        let r = restore(&g, edit);
        assert!(is_malformed(r.clone()), "{name}: {:?}", r.map(|_| ()));
    }
}

#[test]
fn restored_state_must_keep_the_invariants() {
    let g = transit();
    type Edit = fn(&mut GameSnapshot);
    let edits: [(&str, &str, Edit); 6] = [
        ("INV-20", "track_tiles", |s| s.company.track_tiles += 1),
        ("INV-11", "spawn_acc", |s| s.stations[0].spawn_acc = 1000),
        ("INV-08", "waiting", |s| s.waiting.data[0] = 1),
        ("INV-05", "train order", |s| s.lines[0].trains.reverse()),
        ("INV-10", "train pos", |s| s.trains[0].pos = u64::MAX),
        ("INV-14", "unlocked", |s| s.progress.unlocked |= 1 << 20),
    ];
    for (id, name, edit) in edits {
        match restore(&g, edit) {
            Err(SnapshotError::Invariant(v)) => assert_eq!(v.id, id, "{name}: {}", v.detail),
            r => panic!("{name}: {:?}", r.map(|_| ())),
        }
    }
}

/// まだ出現していない駅の計画も検査する(後で出現したときに止まらないため)。
#[test]
fn unspawned_station_plan_is_checked() {
    let g = early();
    assert!(g.state.stations.len() < 26);
    type Edit = fn(&mut GameSnapshot);
    let edits: [(&str, Edit); 3] = [
        ("outside", |s| s.station_plan[26] = (0, 5)),
        ("right", |s| s.station_plan[26] = (39, 5)),
        ("dup", |s| s.station_plan[26] = s.station_plan[0]),
    ];
    for (name, edit) in edits {
        let r = restore(&g, edit);
        assert!(is_malformed(r.clone()), "{name}: {:?}", r.map(|_| ()));
    }
}

/// 列車の乗客の目的駅に行けない写しは、到着で止まらないように拒否する。
#[test]
fn onboard_passengers_need_a_route() {
    let mut g = game(SEED_A, Arc::new(BalanceTable::builtin()));
    let c = create(&g, &[0, 1]);
    apply(&mut g, vec![c]);
    run(&mut g, 600);
    assert!(!g.state.stations[2].is_connected());
    let r = restore(&g, |s| s.onboard.data[2] = 1);
    assert!(is_malformed(r.clone()), "{:?}", r.map(|_| ()));
    assert!(restore(&g, |s| s.onboard.data[1] += 1).is_err());
}

/// 上限ぴったりのカウンタは形の検査を通り、hash を合わせた写しは到着が起きるまで進めても止まらない。
#[test]
fn counters_at_the_limits_keep_stepping() {
    for g in [early(), transit()] {
        let mut s = g.snapshot();
        s.tick = MAX_TICK;
        s.passenger_dest_words = MAX_COUNT;
        s.stats.arrivals = MAX_COUNT;
        s.map_revision = MAX_COUNT;
        s.stats.delivered_hi = MAX_DELIVERED_HI;
        s.stats.purchases = MAX_PURCHASES;
        let bal = Arc::clone(&g.config.balance);
        match Game::restore(Arc::clone(&bal), &s) {
            Err(SnapshotError::HashMismatch { actual, .. }) => s.state_hash = actual,
            r => panic!("{:?}", r.map(|_| ())),
        }
        let mut r = Game::restore(bal, &s).expect("hash を合わせた写し");
        run(&mut r, 900);
        assert!(r.state.stats.arrivals > MAX_COUNT);
        assert!(r.state.rng.passenger_dest.words() > MAX_COUNT);
        assert_eq!(r.now().0, MAX_TICK + 900);
    }
}

#[test]
fn negative_zero_money_is_read_as_zero() {
    let g = game(SEED_A, Arc::new(BalanceTable::builtin()));
    assert!(g.state.money.is_zero());
    let r = restore(&g, |s| s.money_bits = (-0.0f64).to_bits()).expect("−0.0 は 0");
    assert_eq!(r.state_hash(), g.state_hash());
}
