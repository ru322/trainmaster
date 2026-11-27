use proptest::prelude::*;

use super::*;
use crate::command::CommandKind;
use crate::geom::GridPos;
use crate::ids::{CompanyLevel, TrainId};
use crate::param::{BuyMode, ParamKey, UpgradeScope};
use crate::sim::boarding::fixtures::{in_system, put_waiting, tick};
use crate::sim::economy::fixtures::{bal, custom, set_station, sid, world, yen};
use crate::sim::topology::fixtures::line_x;
use crate::state::{Direction, Line, Train, TrainPhase, lay_track};
use crate::time::Ticks;

fn create(ids: &[u32], bends: usize) -> Command {
    Command::CreateLine {
        stations: ids.iter().map(|&i| sid(i)).collect(),
        bends: vec![Bend::StraightFirst; bends],
    }
}

fn extend(l: u32, end: LineEnd, s: u32, bend: Bend) -> Command {
    Command::ExtendLine {
        line: LineId(l),
        end,
        station: sid(s),
        bend,
    }
}

fn apply(st: &mut GameState, b: &BalanceTable, cmd: &Command) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    apply_command(st, b, cmd, &mut SimScratch::default(), &mut ev);
    ev
}

/// 駅 S0〜S3(S0–S1 は路線 L0)。会社 Lv0。
fn base() -> GameState {
    world(
        bal(),
        &[(2, 5, 0), (6, 5, 0), (10, 8, 0), (4, 12, 0)],
        &[(&[0, 1], 0)],
        0,
    )
}

fn empty() -> GameState {
    world(bal(), &[(2, 5, 0), (6, 5, 0), (10, 8, 0)], &[], 0)
}

fn err(st: &GameState, b: &BalanceTable, cmd: &Command) -> CommandError {
    check_command(st, b, cmd).expect_err("拒否される")
}

#[test]
fn create_line_rejections_in_order() {
    let (st, b) = (empty(), bal());
    assert_eq!(
        err(&st, b, &create(&[0, 9], 0)),
        CommandError::UnknownStation(sid(9))
    );
    assert_eq!(
        err(&st, b, &create(&[9], 0)),
        CommandError::UnknownStation(sid(9))
    );
    assert_eq!(
        err(&st, b, &create(&[0], 0)),
        CommandError::TooFewStations { got: 1 }
    );
    assert_eq!(
        err(&st, b, &create(&[], 0)),
        CommandError::TooFewStations { got: 0 }
    );
    assert_eq!(
        err(&st, b, &create(&[0, 1, 2, 1, 0], 0)),
        CommandError::DuplicateStation(sid(1))
    );
    assert_eq!(
        err(&st, b, &create(&[0, 0], 0)),
        CommandError::DuplicateStation(sid(0))
    );
    assert_eq!(
        err(&st, b, &create(&[0, 1, 2], 1)),
        CommandError::BendCountMismatch {
            expected: 2,
            got: 1
        }
    );
    let used = base();
    assert_eq!(
        err(&used, b, &create(&[1, 2], 0)),
        CommandError::BendCountMismatch {
            expected: 1,
            got: 0
        }
    );
    assert_eq!(
        err(&used, b, &create(&[1, 2], 1)),
        CommandError::NoFreeLineSlot { used: 1, slots: 1 }
    );
}

#[test]
fn extend_line_rejections_in_order() {
    let (st, b) = (base(), bal());
    let e = |l, s| extend(l, LineEnd::Tail, s, Bend::StraightFirst);
    assert_eq!(err(&st, b, &e(3, 9)), CommandError::UnknownLine(LineId(3)));
    assert_eq!(err(&st, b, &e(0, 9)), CommandError::UnknownStation(sid(9)));
    assert_eq!(
        err(&st, b, &e(0, 0)),
        CommandError::StationAlreadyOnLine {
            line: LineId(0),
            station: sid(0)
        }
    );
    let late = custom(|s| s.features[0].unlock = 1);
    let st2 = world(
        &late,
        &[(2, 5, 0), (6, 5, 0), (10, 8, 0)],
        &[(&[0, 1], 0)],
        0,
    );
    assert_eq!(
        err(&st2, &late, &e(0, 1)),
        CommandError::StationAlreadyOnLine {
            line: LineId(0),
            station: sid(1)
        }
    );
    assert_eq!(
        err(&st2, &late, &e(0, 2)),
        CommandError::Locked {
            feature: Feature::LineExtend,
            required: CompanyLevel(1)
        }
    );
}

#[test]
fn level_up_uses_the_purchase_plan() {
    let (mut st, b) = (base(), bal());
    let lv = |param, scope| Command::LevelUp {
        param,
        scope,
        mode: BuyMode::One,
    };
    assert_eq!(
        err(&st, b, &lv(ParamKey::S1Spawn, UpgradeScope::AllLines)),
        CommandError::ScopeMismatch {
            param: ParamKey::S1Spawn,
            scope: UpgradeScope::AllLines
        }
    );
    assert_eq!(
        err(
            &st,
            b,
            &lv(ParamKey::S1Spawn, UpgradeScope::Station(sid(2)))
        ),
        CommandError::StationNotConnected(sid(2))
    );
    assert!(matches!(
        err(&st, b, &lv(ParamKey::S1Spawn, UpgradeScope::AllStations)),
        CommandError::InsufficientFunds { .. }
    ));
    st.money = yen(1000);
    let p = check_command(&st, b, &lv(ParamKey::S1Spawn, UpgradeScope::AllStations))
        .unwrap()
        .preview();
    assert_eq!(
        (p.cost, p.steps, p.raised, p.track_tiles),
        (yen(10), 1, 2, 0)
    );
}

#[test]
fn previews_of_line_commands() {
    let st = empty();
    let p = check_command(&st, bal(), &create(&[0, 1, 2], 2))
        .unwrap()
        .preview();
    assert_eq!(
        p,
        CommandPreview {
            cost: Money::ZERO,
            steps: 0,
            raised: 0,
            track_tiles: 4 + 4,
            new_track_tiles: 9,
            new_pass_through: 0,
            souls: 0,
        }
    );
    let b = custom(|s| s.params[8].unlock = 0);
    let st = world(&b, &[(2, 5, 1), (6, 5, 1), (12, 8, 0)], &[(&[0, 1], 2)], 0);
    let p = check_command(&st, &b, &extend(0, LineEnd::Tail, 2, Bend::DiagonalFirst))
        .unwrap()
        .preview();
    assert_eq!(
        (
            p.track_tiles,
            p.new_track_tiles,
            p.new_pass_through,
            p.steps
        ),
        (6, 6, 1, 0)
    );
    let p = check_command(&st, &b, &extend(0, LineEnd::Head, 2, Bend::DiagonalFirst))
        .unwrap()
        .preview();
    assert_eq!(
        (p.track_tiles, p.new_track_tiles, p.new_pass_through),
        (10, 10, 1)
    );
}

#[test]
fn line_draft_checks_existence_duplicates_and_slots() {
    let (st, b) = (empty(), bal());
    assert_eq!(check_line_draft(&st, b, &[], sid(0)), Ok(()));
    assert_eq!(check_line_draft(&st, b, &[sid(0)], sid(1)), Ok(()));
    assert_eq!(
        check_line_draft(&st, b, &[sid(0)], sid(5)),
        Err(CommandError::UnknownStation(sid(5)))
    );
    assert_eq!(
        check_line_draft(&st, b, &[sid(0), sid(1)], sid(0)),
        Err(CommandError::DuplicateStation(sid(0)))
    );
    assert_eq!(
        check_line_draft(&base(), b, &[sid(2)], sid(3)),
        Err(CommandError::NoFreeLineSlot { used: 1, slots: 1 })
    );
}

#[test]
fn create_line_builds_everything_and_orders_events() {
    let (mut st, b) = (empty(), bal());
    let rev = st.map_revision;
    let ev = apply(
        &mut st,
        b,
        &Command::CreateLine {
            stations: vec![sid(2), sid(0), sid(1)],
            bends: vec![Bend::DiagonalFirst, Bend::StraightFirst],
        },
    );
    assert_eq!(
        ev,
        [
            GameEvent::LineCreated {
                line: LineId(0),
                station_count: 3,
                cost: Money::ZERO,
                new_tiles: 9
            },
            GameEvent::TrainAdded {
                train: TrainId(0),
                line: LineId(0)
            },
        ]
    );
    let l = &st.lines[0];
    assert_eq!(l.stations, [sid(2), sid(0), sid(1)]);
    assert_eq!(l.segments[0].bend, Bend::DiagonalFirst);
    let mut want = Vec::new();
    lay_track(
        st.stations[2].pos,
        st.stations[0].pos,
        Bend::DiagonalFirst,
        &mut want,
    );
    assert_eq!(l.segments[0].tiles, want);
    assert_eq!(l.trains, [TrainId(0)]);
    assert_eq!(l.created_at, st.tick);
    for s in &st.stations {
        assert_eq!(s.serving_lines, [LineId(0)]);
    }
    assert!(!st.tracks.get(st.stations[2].pos).is_empty());
    assert!(st.routing.is_dirty());
    assert_eq!(st.map_revision, rev + 1);
    assert_eq!(st.onboard.rows(), 1);
}

#[test]
fn extend_head_stores_the_reversed_bend_and_reports_pass_through() {
    let b = custom(|s| s.params[8].unlock = 0);
    let mut st = world(&b, &[(2, 5, 1), (6, 5, 1), (12, 8, 0)], &[(&[0, 1], 2)], 0);
    let rev = st.map_revision;
    st.routing = crate::routing::Routing::new(&st, &b);
    let ev = apply(
        &mut st,
        &b,
        &extend(0, LineEnd::Head, 2, Bend::DiagonalFirst),
    );
    assert_eq!(
        ev,
        [
            GameEvent::LineExtended {
                line: LineId(0),
                end: LineEnd::Head,
                station: sid(2),
                cost: Money::ZERO,
                new_tiles: 10
            },
            GameEvent::PassThroughAdded {
                line: LineId(0),
                station: sid(2)
            },
        ]
    );
    let l = &st.lines[0];
    assert_eq!(l.stations, [sid(2), sid(0), sid(1)]);
    assert_eq!(l.segments[0].bend, Bend::StraightFirst);
    let mut want = Vec::new();
    lay_track(
        st.stations[2].pos,
        st.stations[0].pos,
        Bend::StraightFirst,
        &mut want,
    );
    assert_eq!(l.segments[0].tiles, want);
    assert_eq!(l.stops, [false, true, true]);
    assert_eq!(st.stations[2].serving_lines, [LineId(0)]);
    assert!(st.routing.is_dirty());
    assert_eq!(st.map_revision, rev + 1);
}

#[test]
fn extend_tail_keeps_the_bend() {
    let (mut st, b) = (base(), bal());
    apply(
        &mut st,
        b,
        &extend(0, LineEnd::Tail, 2, Bend::DiagonalFirst),
    );
    let l = &st.lines[0];
    assert_eq!(l.stations, [sid(0), sid(1), sid(2)]);
    assert_eq!(l.segments[1].bend, Bend::DiagonalFirst);
    assert_eq!(l.stops, [true, true, true]);
}

#[test]
fn rejected_command_changes_nothing_and_skips_the_company_check() {
    let (mut st, b) = (base(), bal());
    for s in 0..2 {
        set_station(&mut st, s, ParamKey::S1Spawn, 4);
        set_station(&mut st, s, ParamKey::S2Fare, 4);
    }
    st.lines[0].levels.speed = Level(4);
    st.lines[0].levels.car_capacity = Level(4);
    let before = st.clone();
    let cmd = extend(0, LineEnd::Tail, 0, Bend::StraightFirst);
    let ev = apply(&mut st, b, &cmd);
    assert_eq!(
        ev,
        [GameEvent::CommandRejected {
            kind: CommandKind::ExtendLine,
            reason: CommandError::StationAlreadyOnLine {
                line: LineId(0),
                station: sid(0)
            }
        }]
    );
    assert_eq!(st, before);
}

#[test]
fn company_level_is_settled_right_after_a_successful_command() {
    let b = bal();
    let mut st = world(
        b,
        &[(2, 5, 0), (6, 5, 0), (10, 8, 0)],
        &[(&[0, 1, 2], 0)],
        0,
    );
    // Σ上限 32 で 10Σ ≥ 224 になる 23 で上がる(11 §7.5 の式)。
    for s in 0..3 {
        set_station(&mut st, s, ParamKey::S1Spawn, 4);
        set_station(&mut st, s, ParamKey::S2Fare, if s == 2 { 2 } else { 4 });
    }
    st.money = yen(1_000_000);
    let gauge = super::super::company::gauge(&st, b);
    assert_eq!(gauge, (22, 32));
    let ev = apply(
        &mut st,
        b,
        &Command::LevelUp {
            param: ParamKey::T4CarCapacity,
            scope: UpgradeScope::Line(LineId(0)),
            mode: BuyMode::One,
        },
    );
    assert!(matches!(ev[0], GameEvent::Purchased { .. }));
    assert_eq!(
        ev[1],
        GameEvent::CompanyLevelUp {
            new_level: CompanyLevel(1),
            all_features_unlocked: false
        }
    );
    assert!(matches!(ev.last(), Some(GameEvent::StationSpawned { .. })));
    assert_eq!(st.company.level, CompanyLevel(1));
}

/// 路線の上の位置・向き・状態の種類(停車中は残りの tick)。
fn place(l: &Line, tr: &Train, origin: StationId) -> (i64, Direction, Option<Ticks>) {
    let dwell = match tr.phase {
        TrainPhase::Dwelling { remaining, .. } => Some(remaining),
        TrainPhase::Running => None,
    };
    (line_x(l, tr, origin), tr.dir, dwell)
}

fn places(st: &GameState, origin: StationId) -> Vec<(i64, Direction, Option<Ticks>)> {
    st.trains
        .iter()
        .map(|t| place(&st.lines[t.line.index()], t, origin))
        .collect()
}

fn config() -> ProptestConfig {
    let mut c = ProptestConfig {
        cases: 64,
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    if matches!(c.rng_seed, proptest::test_runner::RngSeed::Random) {
        c.rng_seed = proptest::test_runner::RngSeed::Fixed(1);
    }
    c
}

proptest! {
    #![proptest_config(config())]

    /// 11 §11.2「移し替えの保存」: 延伸の前後で列車の路線の上の位置と向きと停車の残りが変わらない。
    #[test]
    fn extension_keeps_train_positions(
        gaps in proptest::collection::vec(1..5i32, 2..5),
        trains in 0..4u8,
        ticks in 0..900u32,
        head in any::<bool>(),
        bend in any::<bool>(),
        dy in -3..4i32,
    ) {
        let b = bal();
        let mut x = 4;
        let mut cells = vec![(x, 8, 0)];
        for g in &gaps { x += g; cells.push((x, 8, 0)); }
        let n = cells.len() as u32;
        let order: Vec<u32> = (0..n).collect();
        let new = if head { (1, 8 + dy, 0) } else { (x + 3, 8 + dy, 0) };
        cells.push(new);
        let mut st = world(b, &cells, &[(&order, 0)], 3);
        crate::sim::economy::fixtures::set_line(&mut st, b, 0, ParamKey::R1MaxTrains, trains);
        st.routing = crate::routing::Routing::new(&st, b);
        put_waiting(&mut st, 0, n - 1, 50);
        let (mut arrived, mut ev) = (Vec::new(), Vec::new());
        for _ in 0..ticks { tick(&mut st, b, &mut arrived, &mut ev); }
        let before = places(&st, sid(0));
        let people = in_system(&st);
        let end = if head { LineEnd::Head } else { LineEnd::Tail };
        let bend = if bend { Bend::DiagonalFirst } else { Bend::StraightFirst };
        st.money = Money::pow10(60);
        let ev = apply(&mut st, b, &extend(0, end, n, bend));
        let extended = matches!(ev[0], GameEvent::LineExtended { .. });
        prop_assert!(extended);
        prop_assert_eq!(places(&st, sid(0)), before);
        prop_assert_eq!(in_system(&st), people);
    }

    /// ホーム長の強化で通過駅が停車駅になっても、列車の位置・向き・停車の残りは変わらない。
    #[test]
    fn new_stop_keeps_train_positions(
        gaps in proptest::collection::vec(1..5i32, 3..6),
        short in 1..3usize,
        trains in 0..4u8,
        ticks in 0..900u32,
    ) {
        let b = custom(|s| s.params[8].unlock = 0);
        let mut x = 3;
        let mut cells = vec![(x, 6, 1)];
        for g in &gaps { x += g; cells.push((x, 6, 1)); }
        cells[short].2 = 0;
        let order: Vec<u32> = (0..cells.len() as u32).collect();
        let mut st = world(&b, &cells, &[(&order, 2)], 2);
        crate::sim::economy::fixtures::set_line(&mut st, &b, 0, ParamKey::R1MaxTrains, trains);
        st.routing = crate::routing::Routing::new(&st, &b);
        prop_assert!(!st.lines[0].stops[short]);
        let (mut arrived, mut ev) = (Vec::new(), Vec::new());
        for _ in 0..ticks { tick(&mut st, &b, &mut arrived, &mut ev); }
        let before = places(&st, sid(0));
        st.money = yen(1_000_000_000);
        let ev = apply(&mut st, &b, &Command::LevelUp {
            param: ParamKey::S4Platform,
            scope: UpgradeScope::Station(sid(short as u32)),
            mode: BuyMode::One,
        });
        prop_assert_eq!(ev[1], GameEvent::PassThroughResolved { line: LineId(0), station: sid(short as u32) });
        prop_assert!(st.lines[0].stops[short]);
        prop_assert_eq!(places(&st, sid(0)), before);
    }
}

#[test]
fn creating_and_extending_in_one_tick_keeps_the_arriving_train() {
    let b = bal();
    let mut st = world(b, &[(2, 5, 0), (6, 5, 0), (10, 8, 0)], &[], 0);
    let mut sc = SimScratch::default();
    let mut ev = Vec::new();
    apply_command(&mut st, b, &create(&[0, 1], 1), &mut sc, &mut ev);
    apply_command(
        &mut st,
        b,
        &extend(0, LineEnd::Head, 2, Bend::StraightFirst),
        &mut sc,
        &mut ev,
    );
    let l = &st.lines[0];
    let t = &st.trains[0];
    assert_eq!(
        t.pos,
        l.sections[usize::from(t.section)].length,
        "着く直前のまま"
    );
    assert_eq!(super::super::movement::next_stop(l, t), sid(0));
    assert_eq!(st.stations[2].pos, GridPos { x: 10, y: 8 });
}
