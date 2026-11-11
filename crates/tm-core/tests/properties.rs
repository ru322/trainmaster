//! 11 §11.2 のプロパティテスト(ランダムなコマンド列、validate と適用の一致、決定論、セーブの決定論、restore の頑健さ、
//! 人数の保存、経路の健全さ)。

#[allow(dead_code, reason = "テストのファイルごとに使う補助が違う")]
mod common;

use std::sync::Arc;

use core::cmp::Ordering;
use proptest::prelude::*;

use tm_core::{
    BalanceTable, Bend, BuyMode, Command, CommandError, CompanyLevel, Game, GameEvent,
    GameSnapshot, LineEnd, LineId, Money, ParamKey, ReplayEntry, SnapPhase, SnapshotError,
    StationId, Tick, UpgradeScope, check_monotonic,
};

/// 操作。`Raw` 以外はいまの状態の駅・路線の数で添字を丸めてコマンドにする(有効なコマンドを多くする)。
#[derive(Clone, Debug)]
enum Op {
    Raw(Command),
    Create(Vec<u8>, Vec<bool>),
    Extend(u8, bool, u8, bool),
    Level(u8, bool, u8, bool),
    Boost(u8),
    Ticks(u16),
}

fn station() -> impl Strategy<Value = StationId> {
    prop_oneof![4 => 0..5u32, 1 => 0..22u32].prop_map(StationId)
}

fn line() -> impl Strategy<Value = LineId> {
    prop_oneof![4 => 0..2u32, 1 => 0..8u32].prop_map(LineId)
}

fn bend() -> impl Strategy<Value = Bend> {
    prop_oneof![Just(Bend::StraightFirst), Just(Bend::DiagonalFirst)]
}

fn scope() -> impl Strategy<Value = UpgradeScope> {
    prop_oneof![
        station().prop_map(UpgradeScope::Station),
        Just(UpgradeScope::AllStations),
        line().prop_map(UpgradeScope::Line),
        Just(UpgradeScope::AllLines),
        Just(UpgradeScope::Company),
    ]
}

fn raw_command() -> impl Strategy<Value = Command> {
    prop_oneof![
        (
            proptest::collection::vec(station(), 0..6),
            proptest::collection::vec(bend(), 0..6)
        )
            .prop_map(|(stations, bends)| Command::CreateLine { stations, bends }),
        (line(), any::<bool>(), station(), bend()).prop_map(|(line, head, station, bend)| {
            Command::ExtendLine {
                line,
                end: if head { LineEnd::Head } else { LineEnd::Tail },
                station,
                bend,
            }
        }),
        (0..12usize, scope(), any::<bool>()).prop_map(|(p, scope, max)| Command::LevelUp {
            param: ParamKey::ALL[p],
            scope,
            mode: if max { BuyMode::Max } else { BuyMode::One },
        }),
    ]
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    proptest::collection::vec(
        prop_oneof![
            2 => raw_command().prop_map(Op::Raw),
            2 => (proptest::collection::vec(any::<u8>(), 2..8), proptest::collection::vec(any::<bool>(), 7))
                .prop_map(|(s, b)| Op::Create(s, b)),
            3 => (any::<u8>(), any::<bool>(), any::<u8>(), any::<bool>())
                .prop_map(|(l, h, s, b)| Op::Extend(l, h, s, b)),
            6 => (0..12u8, any::<bool>(), any::<u8>(), any::<bool>())
                .prop_map(|(p, all, id, max)| Op::Level(p, all, id, max)),
            4 => (0..12u8).prop_map(Op::Boost),
            2 => (1..300u16).prop_map(Op::Ticks),
        ],
        1..80,
    )
}

fn pick(n: usize, k: u8) -> u32 {
    u32::try_from(usize::from(k) % n.max(1)).unwrap()
}

fn bend_of(b: bool) -> Bend {
    if b {
        Bend::DiagonalFirst
    } else {
        Bend::StraightFirst
    }
}

/// 操作をいまの状態でコマンドにする(`Ticks` は `None`)。
fn resolve(g: &Game, op: &Op) -> Option<Command> {
    let v = g.view();
    let (ns, nl) = (v.stations().len(), v.lines().len());
    Some(match op {
        Op::Raw(c) => c.clone(),
        Op::Create(picks, bends) => {
            let mut stations: Vec<StationId> = Vec::new();
            for &k in picks {
                let s = StationId(pick(ns, k));
                if !stations.contains(&s) {
                    stations.push(s);
                }
            }
            let n = stations.len().saturating_sub(1);
            let bends = bends.iter().take(n).map(|&b| bend_of(b)).collect();
            Command::CreateLine { stations, bends }
        }
        Op::Extend(l, head, s, b) => Command::ExtendLine {
            line: LineId(pick(nl, *l)),
            end: if *head { LineEnd::Head } else { LineEnd::Tail },
            station: StationId(pick(ns, *s)),
            bend: bend_of(*b),
        },
        Op::Level(p, all, id, max) => {
            let param = ParamKey::ALL[usize::from(*p)];
            let scope = match (common::all_scope(param), all) {
                (UpgradeScope::AllStations, false) => {
                    UpgradeScope::Station(StationId(pick(ns, *id)))
                }
                (UpgradeScope::AllLines, false) => UpgradeScope::Line(LineId(pick(nl, *id))),
                (s, _) => s,
            };
            Command::LevelUp {
                param,
                scope,
                mode: if *max { BuyMode::Max } else { BuyMode::One },
            }
        }
        Op::Boost(p) => {
            let param = ParamKey::ALL[usize::from(*p)];
            Command::LevelUp {
                param,
                scope: common::all_scope(param),
                mode: BuyMode::Max,
            }
        }
        Op::Ticks(_) => return None,
    })
}

fn table(k: u8) -> Arc<BalanceTable> {
    match k {
        0 => common::builtin(),
        1 => common::rich(),
        _ => common::cheap(),
    }
}

fn config(cases: u32) -> ProptestConfig {
    let mut c = ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    if matches!(c.rng_seed, proptest::test_runner::RngSeed::Random) {
        c.rng_seed = proptest::test_runner::RngSeed::Fixed(1);
    }
    c
}

/// 1 tick 進め、毎 step の後の検査(不変条件・減らせない・人数の保存)を行う。
fn checked_step(g: &mut Game, events: &mut Vec<GameEvent>) -> Result<(), TestCaseError> {
    let before = g.monotonic_snapshot();
    events.clear();
    g.step(events);
    if let Err(v) = g.check_invariants() {
        return Err(TestCaseError::fail(format!("{}: {}", v.id, v.detail)));
    }
    if let Err(v) = check_monotonic(&before, &g.monotonic_snapshot()) {
        return Err(TestCaseError::fail(format!("{}: {}", v.id, v.detail)));
    }
    let v = g.view();
    let factor = u64::from(v.balance().spec().transit.waiting_cap_factor);
    for s in v.stations() {
        prop_assert!(
            s.waiting.0 <= s.capacity.0.saturating_mul(factor),
            "{:?}",
            s.id
        );
    }
    #[cfg(feature = "test-support")]
    {
        let l = g.pax_ledger();
        prop_assert!(l.is_conserved(), "{l:?}");
    }
    Ok(())
}

/// 所持金 `before` から線路の値段を引き、この tick の運賃を足した額(11 §4.8)。
fn money_after_build(before: Money, cost: Money, events: &[GameEvent]) -> Money {
    let mut m = before.checked_sub(cost).expect("所持金は負にならない");
    for e in events {
        if let GameEvent::TrainArrived { income, .. } = e {
            m = m.saturating_add(*income);
        }
    }
    m
}

/// コマンドを 1 つだけ積んで 1 tick 進め、`validate` の結果と適用の結果が一致するかを確かめる。
fn apply_one(g: &mut Game, cmd: Command, events: &mut Vec<GameEvent>) -> Result<(), TestCaseError> {
    let pre = g.validate(&cmd);
    let kind = cmd.kind();
    let money = g.view().money();
    g.enqueue(cmd.clone());
    checked_step(g, events)?;
    prop_assert!(g.view().money().total_cmp(&Money::ZERO) != Ordering::Less);
    match pre {
        Ok(p) => {
            let first = events[0];
            match (&cmd, first) {
                (
                    Command::CreateLine { .. },
                    GameEvent::LineCreated {
                        cost, new_tiles, ..
                    },
                )
                | (
                    Command::ExtendLine { .. },
                    GameEvent::LineExtended {
                        cost, new_tiles, ..
                    },
                ) => {
                    prop_assert_eq!((cost, new_tiles), (p.cost, p.new_track_tiles));
                    prop_assert_eq!(g.view().money(), money_after_build(money, cost, events));
                }
                (
                    Command::LevelUp { .. },
                    GameEvent::Purchased {
                        steps,
                        raised,
                        cost,
                        ..
                    },
                ) => prop_assert_eq!((steps, raised, cost), (p.steps, p.raised, p.cost)),
                _ => prop_assert!(false, "validate は Ok だが {first:?}"),
            }
        }
        Err(e) => prop_assert_eq!(events[0], GameEvent::CommandRejected { kind, reason: e }),
    }
    Ok(())
}

fn run(g: &mut Game, ops: &[Op], events: &mut Vec<GameEvent>) -> Result<(), TestCaseError> {
    for op in ops {
        match (op, resolve(g, op)) {
            (Op::Ticks(n), _) => {
                for _ in 0..*n {
                    checked_step(g, events)?;
                }
            }
            (_, Some(c)) => apply_one(g, c, events)?,
            (_, None) => unreachable!(),
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(config(64))]

    /// ランダムなコマンド列(有効・無効の混在)とランダムな tick 数で、毎 step の後に不変条件と
    /// 「減らせない」が成り立ち、`validate` と適用が一致する(11 §11.2)。
    #[test]
    fn random_commands_keep_invariants(seed in 0..1000u64, k in 0..3u8, prefix in 0..30u32, ops in ops()) {
        let mut g = common::new_game(seed, &table(k));
        g.check_invariants().map_err(|v| TestCaseError::fail(v.detail))?;
        let mut events = Vec::new();
        if k == 2 {
            for _ in 0..prefix {
                for c in common::driver_full_build(&g.view()) {
                    g.enqueue(c);
                }
                checked_step(&mut g, &mut events)?;
            }
        }
        run(&mut g, &ops, &mut events)?;
    }
}

proptest! {
    #![proptest_config(config(12))]

    /// 同じシード・同じコマンド列なら state_hash とイベント列が一致する。途中で複製しても一致する。
    #[test]
    fn same_inputs_give_same_results(seed in 0..1000u64, k in 0..3u8, ops in ops()) {
        let bal = table(k);
        let (mut a, mut b) = (common::new_game(seed, &bal), common::new_game(seed, &bal));
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        let half = ops.len() / 2;
        for op in &ops[..half] {
            step_op(&mut a, op, &mut ea);
            step_op(&mut b, op, &mut eb);
            prop_assert_eq!(&ea, &eb);
            prop_assert_eq!(a.state_hash(), b.state_hash());
        }
        let mut c = a.clone();
        let mut ec = ea.clone();
        for op in &ops[half..] {
            step_op(&mut a, op, &mut ea);
            step_op(&mut c, op, &mut ec);
            prop_assert_eq!(&ea, &ec);
            prop_assert_eq!(a.state_hash(), c.state_hash());
        }
    }
}

/// 1 tick ごとの (state_hash, イベント列)。
type Trace = Vec<(u64, Vec<GameEvent>)>;

/// コマンドを積んで 1 tick 進め、積んだコマンドと結果を記録する。
fn traced_step(g: &mut Game, cmds: Vec<Command>, log: &mut Vec<ReplayEntry>, trace: &mut Trace) {
    for cmd in cmds {
        log.push(ReplayEntry {
            tick: g.next_tick(),
            cmd: cmd.clone(),
        });
        g.enqueue(cmd);
    }
    let mut events = Vec::new();
    g.step(&mut events);
    trace.push((g.state_hash(), events));
}

/// 表 `k`(2 なら `prefix` tick だけ上限の規模へ作る)で `ops` を流し、コマンドの記録と結果を返す。
fn traced_run(g: &mut Game, k: u8, prefix: u32, ops: &[Op]) -> (Vec<ReplayEntry>, Trace) {
    let (mut log, mut trace) = (Vec::new(), Vec::new());
    if k == 2 {
        for _ in 0..prefix {
            let cmds = common::driver_full_build(&g.view());
            traced_step(g, cmds, &mut log, &mut trace);
        }
    }
    for op in ops {
        match (op, resolve(g, op)) {
            (Op::Ticks(n), _) => {
                for _ in 0..*n {
                    traced_step(g, Vec::new(), &mut log, &mut trace);
                }
            }
            (_, Some(c)) => traced_step(g, vec![c], &mut log, &mut trace),
            (_, None) => unreachable!(),
        }
    }
    (log, trace)
}

/// 写しを RON に書いて読む。
fn through_ron(snap: &GameSnapshot) -> GameSnapshot {
    ron::from_str(&ron::to_string(snap).expect("RON に書ける")).expect("RON を読める")
}

proptest! {
    #![proptest_config(config(24))]

    /// セーブの決定論(11 §11.2、17 §C.6): 任意の tick で snapshot → RON → restore して続けたものが、続けて動かした
    /// ものと以後のすべての tick で同じ state_hash とイベント列になり、最後の写しも一致する。
    #[test]
    fn restored_game_continues_identically(
        seed in 0..1000u64,
        k in 0..3u8,
        prefix in 0..400u32,
        ops in ops(),
        cut in 0..=100u64,
    ) {
        let bal = table(k);
        let mut a = common::new_game(seed, &bal);
        let (log, trace) = traced_run(&mut a, k, prefix, &ops);
        let total = a.now().0;
        let save_at = total * cut / 100;
        let mut b = common::new_game(seed, &bal);
        let mut entries = log.iter().peekable();
        while b.now().0 < save_at {
            let t = b.next_tick();
            while let Some(e) = entries.next_if(|e| e.tick == t) {
                b.enqueue(e.cmd.clone());
            }
            b.step(&mut Vec::new());
        }
        let snap = through_ron(&b.snapshot());
        prop_assert_eq!(&snap, &b.snapshot());
        let mut c = Game::restore(Arc::clone(&bal), &snap)
            .map_err(|e| TestCaseError::fail(format!("tick {save_at}: {e:?}")))?;
        prop_assert_eq!(c.state_hash(), b.state_hash());
        let mut events = Vec::new();
        for (hash, want) in &trace[save_at as usize..] {
            let t = c.next_tick();
            while let Some(e) = entries.next_if(|e| e.tick == t) {
                c.enqueue(e.cmd.clone());
            }
            events.clear();
            c.step(&mut events);
            prop_assert_eq!(&events, want, "tick {}", t.0);
            prop_assert_eq!(c.state_hash(), *hash, "tick {}", t.0);
        }
        prop_assert_eq!(c.now(), Tick(total));
        prop_assert_eq!(c.snapshot(), a.snapshot());
    }
}

/// 値: 小さい数(有効な添字やレベルに当たりやすい)と任意の数。
fn value() -> impl Strategy<Value = u64> {
    prop_oneof![
        4 => 0..4u64,
        4 => 0..64u64,
        4 => any::<u64>(),
        1 => (0..400u64).prop_map(|d| u64::MAX - d),
        1 => proptest::sample::select(vec![1 << 24, 1 << 36, 1 << 40, 1 << 50, (1 << 50) + 1]),
    ]
}

/// 書き換える項目。行のあふれ(32・33)とカウンタ(31・34)を多めに選ぶ。
fn field() -> impl Strategy<Value = u8> {
    prop_oneof![6 => any::<u8>(), 1 => 31..35u8]
}

fn pick_mut<T>(v: &mut [T], i: u64) -> Option<&mut T> {
    let n = v.len() as u64;
    if n == 0 {
        None
    } else {
        v.get_mut((i % n) as usize)
    }
}

fn resize<T: Clone>(v: &mut Vec<T>, grow: bool) {
    if grow {
        if let Some(x) = v.last().cloned() {
            v.push(x);
        }
    } else {
        v.pop();
    }
}

/// 写しの数値・長さ・ID の 1 つを書き換える。
fn mutate(s: &mut GameSnapshot, field: u8, i: u64, v: u64) {
    let small = v as u8;
    match field % 35 {
        0 => s.format = v as u32,
        1 => s.rules_version = v as u32,
        2 => s.config_hash = v,
        3 => s.seed = v,
        4 => s.state_hash = v,
        5 => s.tick = v,
        6 => s.money_bits = v,
        7 => s.total_earned_bits = v,
        8 => s.company.level = small,
        9 => s.company.line_slots = small,
        10 => s.company.track_tiles = v as u32,
        11 => {
            if let Some(p) = pick_mut(&mut s.station_plan, i) {
                *p = ((v & 0x3f) as i32 - 2, ((v >> 6) & 0x1f) as i32 - 2);
            }
        }
        12 => resize(&mut s.station_plan, v & 1 == 0),
        13 => {
            if let Some(st) = pick_mut(&mut s.stations, i) {
                st.levels[(v % 4) as usize] = (v >> 2) as u8;
            }
        }
        14 => {
            if let Some(st) = pick_mut(&mut s.stations, i) {
                st.spawn_acc = v;
            }
        }
        15 => resize(&mut s.stations, v & 1 == 0),
        16 => {
            if let Some(l) = pick_mut(&mut s.lines, i)
                && let Some(x) = pick_mut(&mut l.stations, v >> 8)
            {
                *x = (v & 0xff) as u32;
            }
        }
        17 => {
            if let Some(l) = pick_mut(&mut s.lines, i) {
                match v % 3 {
                    0 => resize(&mut l.bends, true),
                    1 => resize(&mut l.bends, false),
                    _ => resize(&mut l.stations, v & 4 == 0),
                }
            }
        }
        18 => {
            if let Some(l) = pick_mut(&mut s.lines, i) {
                l.levels[(v % 7) as usize] = (v >> 3) as u8;
            }
        }
        19 => {
            if let Some(l) = pick_mut(&mut s.lines, i) {
                match v % 3 {
                    0 => resize(&mut l.trains, true),
                    1 => resize(&mut l.trains, false),
                    _ => {
                        if let Some(t) = pick_mut(&mut l.trains, v >> 2) {
                            *t = (v >> 10) as u32 % 64;
                        }
                    }
                }
            }
        }
        20 => resize(&mut s.lines, v & 1 == 0),
        21 => {
            if let Some(t) = pick_mut(&mut s.trains, i) {
                t.line = (v % 8) as u32;
            }
        }
        22 => {
            if let Some(t) = pick_mut(&mut s.trains, i) {
                t.section = v as u16;
            }
        }
        23 => {
            if let Some(t) = pick_mut(&mut s.trains, i) {
                t.dir = small;
            }
        }
        24 => {
            if let Some(t) = pick_mut(&mut s.trains, i) {
                t.pos = v;
            }
        }
        25 => {
            if let Some(t) = pick_mut(&mut s.trains, i) {
                t.phase = if v & 1 == 0 {
                    SnapPhase::Running
                } else {
                    SnapPhase::Dwelling {
                        at: (v >> 1) as u16,
                        remaining: (v >> 17) as u32,
                    }
                };
            }
        }
        26 => resize(&mut s.trains, v & 1 == 0),
        27 | 28 => {
            let m = if field % 35 == 27 {
                &mut s.waiting
            } else {
                &mut s.onboard
            };
            match v % 4 {
                0 => m.rows = (v >> 2) as u32 % 64,
                1 => m.cols = (v >> 2) as u32 % 64,
                2 => resize(&mut m.data, v & 4 == 0),
                _ => {
                    if let Some(x) = pick_mut(&mut m.data, i) {
                        *x = v >> 2;
                    }
                }
            }
        }
        29 => match v % 4 {
            0 => s.progress.unlocked = (v >> 2) as u32,
            1 => s.progress.upgraded_once = (v >> 2) as u16,
            2 => s.progress.max_money_unit = (v >> 2) as u8,
            _ => s.progress.goal_reached = !s.progress.goal_reached,
        },
        30 => match v % 4 {
            0 => {
                if let Some(x) = pick_mut(&mut s.stats.income_window_bits, i) {
                    *x = v;
                }
            }
            1 => resize(&mut s.stats.income_window_bits, v & 4 == 0),
            2 => s.stats.window_head = (v >> 2) as u8,
            _ => s.stats.income_this_sec_bits = v,
        },
        31 => match v % 3 {
            0 => s.passenger_dest_words = v,
            1 => s.map_revision = v,
            _ => s.stats.delivered_hi = v,
        },
        32 | 33 => {
            let m = if field % 35 == 32 {
                &mut s.waiting
            } else {
                &mut s.onboard
            };
            let (c, n) = (m.cols as usize, m.data.len());
            let v = if i & 1 == 1 { v | 1 << 63 } else { v };
            if c > 0 && n > 0 {
                let start = (i as usize % n) / c * c;
                m.data.iter_mut().skip(start).take(c).for_each(|x| *x = v);
            }
        }
        _ => match (i % 6, if i & 8 == 8 { u64::MAX - v % 400 } else { v }) {
            (0, v) => s.tick = v,
            (1, v) => s.passenger_dest_words = v,
            (2, v) => s.stats.arrivals = v,
            (3, v) => s.map_revision = v,
            (4, v) => s.stats.delivered_hi = v,
            (_, v) => s.stats.purchases = v as u32,
        },
    }
}

proptest! {
    #![proptest_config(config(256))]

    /// restore は panic しない(11 §11.2): 正しい写しをランダムに書き換えた写しで、`Ok`(整合していた場合)か `Err` を返す。
    /// hash だけが合わない写しは hash を直して `Ok` になり、`Ok` の写しは 300 tick 進めても止まらない。
    #[test]
    fn restore_never_panics(
        seed in 0..1000u64,
        k in 0..3u8,
        prefix in 0..300u32,
        ops in ops().prop_map(|mut v| { v.truncate(12); v }),
        edits in proptest::collection::vec((field(), any::<u64>(), value()), 1..4),
    ) {
        let bal = table(k);
        let mut g = common::new_game(seed, &bal);
        traced_run(&mut g, k, prefix, &ops);
        let mut snap = g.snapshot();
        for &(field, i, v) in &edits {
            mutate(&mut snap, field, i, v);
        }
        let restored = match Game::restore(Arc::clone(&bal), &snap) {
            Ok(r) => Some(r),
            Err(SnapshotError::HashMismatch { actual, .. }) => {
                snap.state_hash = actual;
                let r = Game::restore(Arc::clone(&bal), &snap)
                    .map_err(|e| TestCaseError::fail(format!("hash を直しても {e:?}")))?;
                Some(r)
            }
            Err(_) => None,
        };
        if let Some(mut r) = restored {
            prop_assert_eq!(r.state_hash(), snap.state_hash);
            prop_assert!(r.check_invariants().is_ok());
            prop_assert!(Game::restore(Arc::clone(&bal), &r.snapshot()).is_ok());
            let mut events = Vec::new();
            for _ in 0..300 {
                events.clear();
                r.step(&mut events);
            }
            prop_assert!(r.check_invariants().is_ok());
        }
    }
}

/// 路線のコマンドの値段(`cost` か `required`)。ほかの拒否は `None`。
fn track_price(g: &Game, cmd: &Command) -> Option<Money> {
    match g.validate(cmd) {
        Ok(p) => Some(p.cost),
        Err(CommandError::InsufficientFunds { required, .. }) => Some(required),
        Err(_) => None,
    }
}

proptest! {
    #![proptest_config(config(32))]

    /// 線路の値段は待っている間に変わらない。強化だけをして会社レベルが同じなら変わらない(10 TD-41、11 §11.2)。
    #[test]
    fn track_price_does_not_change_while_waiting(
        seed in 0..1000u64,
        k in 0..2u8,
        ops in ops(),
        probes in proptest::collection::vec(prop_oneof![
            (proptest::collection::vec(any::<u8>(), 2..6), proptest::collection::vec(any::<bool>(), 5))
                .prop_map(|(s, b)| Op::Create(s, b)),
            (any::<u8>(), any::<bool>(), any::<u8>(), any::<bool>())
                .prop_map(|(l, h, s, b)| Op::Extend(l, h, s, b)),
        ], 1..6),
        wait in 1..900u16,
        boosts in proptest::collection::vec(0..12u8, 0..4),
    ) {
        let mut g = common::new_game(seed, &table(k));
        let mut events = Vec::new();
        for op in &ops {
            step_op(&mut g, op, &mut events);
        }
        let cmds: Vec<Command> = probes.iter().filter_map(|op| resolve(&g, op)).collect();
        let before: Vec<Option<Money>> = cmds.iter().map(|c| track_price(&g, c)).collect();
        for _ in 0..wait {
            g.step(&mut events);
        }
        let waited: Vec<Option<Money>> = cmds.iter().map(|c| track_price(&g, c)).collect();
        prop_assert_eq!(&waited, &before);
        let level: CompanyLevel = g.view().company().level;
        for p in &boosts {
            step_op(&mut g, &Op::Boost(*p), &mut events);
        }
        if g.view().company().level == level {
            for (c, b) in cmds.iter().zip(&before) {
                if let (Some(now), Some(b)) = (track_price(&g, c), b) {
                    prop_assert_eq!(now, *b, "{:?}", c);
                }
            }
        }
    }
}

fn step_op(g: &mut Game, op: &Op, all: &mut Vec<GameEvent>) {
    match (op, resolve(g, op)) {
        (Op::Ticks(n), _) => {
            for _ in 0..*n {
                g.step(all);
            }
        }
        (_, Some(c)) => {
            g.enqueue(c);
            g.step(all);
        }
        (_, None) => unreachable!(),
    }
}

#[test]
fn rejected_in_the_same_tick_after_money_is_spent() {
    let mut g = common::new_game(1, &common::custom(|s| s.economy.initial_money_yen = 10));
    let line = common::create_line(&g.view(), vec![StationId(0), StationId(1)]);
    g.enqueue(line);
    g.step(&mut Vec::new());
    let buy = Command::LevelUp {
        param: ParamKey::S2Fare,
        scope: UpgradeScope::AllStations,
        mode: BuyMode::One,
    };
    assert!(g.validate(&buy).is_ok());
    g.enqueue(buy.clone());
    g.enqueue(buy.clone());
    let mut ev = Vec::new();
    g.step(&mut ev);
    assert!(matches!(ev[0], GameEvent::Purchased { .. }));
    assert!(ev.iter().any(|e| matches!(
        e,
        GameEvent::CommandRejected {
            reason: CommandError::InsufficientFunds { .. },
            ..
        }
    )));
}

/// チュートリアル #3・#5(会社 Lv0)の最初の路線と延伸は、所持金 0 円でもただでできる(10 TD-41)。
#[test]
fn first_line_and_extension_are_free_at_level_0() {
    let bal = common::builtin();
    assert!(bal.initial_money().is_zero());
    for seed in 1..=3 {
        let g = common::new_game(seed, &bal);
        let ids: Vec<StationId> = g.view().stations().map(|s| s.id).collect();
        for &a in &ids {
            for &b in ids.iter().filter(|&&b| b != a) {
                for bend in [Bend::StraightFirst, Bend::DiagonalFirst] {
                    let cmd = Command::CreateLine {
                        stations: vec![a, b],
                        bends: vec![bend],
                    };
                    let p = g.validate(&cmd).expect("ただで作れる");
                    assert!(p.cost.is_zero() && p.new_track_tiles > 0);
                    let mut g2 = g.clone();
                    g2.enqueue(cmd);
                    g2.step(&mut Vec::new());
                    let c = ids.iter().copied().find(|s| ![a, b].contains(s)).unwrap();
                    for end in [LineEnd::Head, LineEnd::Tail] {
                        let ext = Command::ExtendLine {
                            line: LineId(0),
                            end,
                            station: c,
                            bend,
                        };
                        let p = g2.validate(&ext).expect("ただで延ばせる");
                        assert!(p.cost.is_zero(), "シード {seed}");
                    }
                }
            }
        }
    }
}

/// 経路の健全さ(11 §11.2): 乗り換えのある網で、到着の回数あたりの目的駅への到着が 0 でない。
/// 割合は M8 で 1/5(駅で待てる人数が増えて、駅で待つ人が増えた。シード 1〜3 で 0.34・0.26・0.24)。
#[cfg(feature = "test-support")]
#[test]
fn passengers_reach_their_destinations() {
    for seed in [1, 2, 3] {
        let (g, _, _) = common::record(seed, &common::builtin(), 27_000, common::driver_recommend);
        let l = g.pax_ledger();
        assert!(l.is_conserved(), "{l:?}");
        assert!(l.delivered * 5 > l.spawned, "シード {seed}: {l:?}");
    }
    let mut g = common::full_build(4);
    let start = g.pax_ledger();
    let mut ev = Vec::new();
    for _ in 0..3600 {
        g.step(&mut ev);
    }
    let l = g.pax_ledger();
    assert!(l.is_conserved(), "{l:?}");
    assert!(l.delivered > start.delivered, "{l:?}");
}
