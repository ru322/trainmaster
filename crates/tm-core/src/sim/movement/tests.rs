use proptest::prelude::*;

use super::*;
use crate::ids::Level;
use crate::sim::topology::fixtures::*;
use crate::state::{Section, Station};
use crate::time::POS_UNITS_PER_TILE as U;
use crate::units::Millis;

use Direction::{Backward, Forward};

const RUN: TrainPhase = TrainPhase::Running;

fn dwell_max() -> Ticks {
    crate::units::ms_to_ticks(Millis(bal().spec().transit.dwell_max_ms))
}

/// S0 —(3)— S1 —(4)— S2(すべて停車駅)。L = 7 マス、P = 14 マス。
fn three_stops() -> (Vec<Station>, Line) {
    let st = stations(&[(2, 5, 0), (5, 5, 0), (9, 5, 0)]);
    let l = line(0, &[0, 1, 2], &st, 0);
    (st, l)
}

/// 端と途中が通過駅: P0 —(2)— S1 —(3)— P2 —(4)— S3 —(1)— P4。停車駅は S1・S3 だけ。
fn pass_at_ends() -> Line {
    let st = stations(&[(1, 5, 0), (3, 5, 1), (6, 5, 0), (10, 5, 1), (11, 5, 0)]);
    line(0, &[0, 1, 2, 3, 4], &st, 2)
}

fn advance_one(l: &Line, tr: &mut Train) -> bool {
    let mut ts = [*tr];
    let mut arrived = Vec::new();
    advance_trains(core::slice::from_ref(l), &mut ts, &bal(), &mut arrived);
    *tr = ts[0];
    !arrived.is_empty()
}

fn ticks_to_arrive(l: &Line, tr: &mut Train) -> u32 {
    (1..=100_000)
        .find(|_| advance_one(l, tr))
        .expect("着かない")
}

/// 11 §5.9: 6 マス・速度 Lv0 で 360 tick、5 マス・Lv5 で 86 tick(85 tick 目は 297500)。
#[test]
fn arrival_ticks_match_5_9() {
    let st = stations(&[(2, 5, 0), (8, 5, 0), (13, 5, 0)]);
    let mut l = line(0, &[0, 1, 2], &st, 0);
    let mut tr = train(0, &l, 0, Forward, 0, RUN);
    assert_eq!(ticks_to_arrive(&l, &mut tr), 360);
    assert_eq!(tr.pos, PosUnits(360_000));

    l.levels.speed = Level(5);
    let mut tr = train(0, &l, 1, Forward, 0, RUN);
    for _ in 0..85 {
        assert!(!advance_one(&l, &mut tr));
    }
    assert_eq!(tr.pos, PosUnits(297_500));
    assert!(advance_one(&l, &mut tr));
    assert_eq!(tr.pos, PosUnits(300_000), "超過分は捨てる");
}

/// 到着判定 `pos + v‰ ≥ 区間長` の境界。
#[test]
fn arrival_boundary() {
    let (_, l) = three_stops();
    let len = 3 * U;
    let mut tr = train(0, &l, 0, Forward, len - 1001, RUN);
    assert!(!advance_one(&l, &mut tr));
    assert_eq!(tr.pos, PosUnits(len - 1));
    let mut tr = train(0, &l, 0, Forward, len - 1000, RUN);
    assert!(advance_one(&l, &mut tr));
    assert_eq!(tr.pos, PosUnits(len));
    let mut tr = train(0, &l, 0, Forward, len, RUN);
    assert!(advance_one(&l, &mut tr), "置いた直後の列車は手順3で着く");
}

/// SIM-D-04: Tick(100) に着いて 60 tick 停車なら、Tick(101)〜(159) は停車、Tick(160) に発車して v‰ 進む。
#[test]
fn dwell_then_depart_on_next_tick() {
    let (_, l) = three_stops();
    let mut tr = train(0, &l, 0, Forward, 3 * U - 500, RUN);
    let mut tick = 100;
    assert!(advance_one(&l, &mut tr));
    assert_eq!(turn_at_arrival(&l, &mut tr), StationId(2));
    begin_dwell(&l, &mut tr, Ticks(60));
    assert_eq!(
        tr.phase,
        TrainPhase::Dwelling {
            at: 1,
            remaining: Ticks(59)
        }
    );
    let mut dwelling_ticks = vec![tick];
    loop {
        tick += 1;
        let arrived = advance_one(&l, &mut tr);
        assert!(!arrived);
        check_inv10(&l, &tr, dwell_max(), false);
        if tr.phase == RUN {
            break;
        }
        assert_eq!(tr.pos, PosUnits(0));
        dwelling_ticks.push(tick);
    }
    assert_eq!(dwelling_ticks.len(), 60);
    assert_eq!(*dwelling_ticks.last().unwrap(), 159);
    assert_eq!(tick, 160);
    assert_eq!((tr.section, tr.dir, tr.pos), (1, Forward, PosUnits(1000)));
}

#[test]
fn one_tick_dwell_departs_next_tick() {
    let (_, l) = three_stops();
    let mut tr = train(0, &l, 0, Forward, 0, RUN);
    begin_dwell(&l, &mut tr, Ticks(1));
    assert_eq!(
        tr.phase,
        TrainPhase::Dwelling {
            at: 0,
            remaining: Ticks(0)
        }
    );
    assert!(!advance_one(&l, &mut tr));
    assert_eq!((tr.phase, tr.pos), (RUN, PosUnits(1000)));
}

#[test]
fn departure_leg_turns_only_at_end_stops() {
    let (_, l) = three_stops();
    assert_eq!(departure_leg(&l, 0, Forward), (1, Forward));
    assert_eq!(departure_leg(&l, 1, Forward), (1, Backward));
    assert_eq!(departure_leg(&l, 1, Backward), (0, Backward));
    assert_eq!(departure_leg(&l, 0, Backward), (0, Forward));
    assert_eq!(
        (
            arrival_index(&l, 1, Forward),
            arrival_index(&l, 1, Backward)
        ),
        (2, 1)
    );
    assert_eq!(
        (
            departure_index(&l, 1, Forward),
            departure_index(&l, 1, Backward)
        ),
        (1, 2)
    );
}

/// 端で折り返す: 向きは到着の tick に決まり、停車中は出発する区間の pos = 0 にいる。
#[test]
fn turnaround_decided_at_arrival() {
    let (_, l) = three_stops();
    let mut tr = train(0, &l, 1, Forward, 4 * U - 1, RUN);
    assert!(advance_one(&l, &mut tr));
    assert_eq!(turn_at_arrival(&l, &mut tr), StationId(1));
    assert_eq!((tr.section, tr.dir, tr.pos), (1, Backward, PosUnits(0)));
    begin_dwell(&l, &mut tr, Ticks(90));
    assert_eq!(
        tr.phase,
        TrainPhase::Dwelling {
            at: 2,
            remaining: Ticks(89)
        }
    );
    assert_eq!(next_stop(&l, &tr), StationId(1));
    check_inv10(&l, &tr, dwell_max(), false);
    for _ in 0..89 {
        advance_one(&l, &mut tr);
        assert_eq!((tr.section, tr.dir, tr.pos), (1, Backward, PosUnits(0)));
    }
    advance_one(&l, &mut tr);
    assert_eq!((tr.phase, tr.dir, tr.pos), (RUN, Backward, PosUnits(1000)));
}

/// 端の駅が通過駅なら、いちばん端の停車駅で折り返す(01 §6.4、10 TD-19)。
#[test]
fn turnaround_at_last_stop_when_end_is_pass_station() {
    let l = pass_at_ends();
    assert_eq!(l.sections.len(), 1);
    let mut tr = train(0, &l, 0, Forward, 0, RUN);
    assert_eq!(ticks_to_arrive(&l, &mut tr), 420);
    assert_eq!(turn_at_arrival(&l, &mut tr), StationId(1));
    assert_eq!((tr.section, tr.dir), (0, Backward));
    begin_dwell(&l, &mut tr, Ticks(60));
    assert_eq!(
        tr.phase,
        TrainPhase::Dwelling {
            at: 3,
            remaining: Ticks(59)
        }
    );
}

/// 往復を回して、停車駅を S0 → S1 → S2 → S1 → S0 → … の順に訪れ、毎 tick INV-10 が成り立つ。
#[test]
fn shuttles_back_and_forth() {
    let (_, l) = three_stops();
    let mut trains = vec![
        train(0, &l, 0, Backward, 3 * U, RUN),
        train(1, &l, 1, Backward, 1234, RUN),
    ];
    let mut visits: Vec<Vec<StationId>> = vec![Vec::new(), Vec::new()];
    let mut arrived = Vec::new();
    for _ in 0..5000 {
        advance_trains(core::slice::from_ref(&l), &mut trains, &bal(), &mut arrived);
        for &t in &arrived {
            let tr = &mut trains[t.index()];
            visits[t.index()].push(next_stop(&l, tr));
            turn_at_arrival(&l, tr);
            begin_dwell(&l, tr, Ticks(60 + t.0 * 7));
        }
        for tr in &trains {
            check_inv10(&l, tr, dwell_max(), false);
        }
    }
    let pattern = [0, 1, 2, 1].map(StationId);
    for (i, v) in visits[0].iter().enumerate() {
        assert_eq!(*v, pattern[i % 4], "T0 の {i} 回目");
    }
    let pattern = [1, 0, 1, 2].map(StationId);
    for (i, v) in visits[1].iter().enumerate() {
        assert_eq!(*v, pattern[i % 4], "T1 の {i} 回目");
    }
    assert!(visits[0].len() >= 8);
}

#[test]
fn place_at_examples() {
    let (_, l) = three_stops();
    assert_eq!(cycle_len(&l), 14 * U);
    let len0 = PosUnits(3 * U);
    let len1 = PosUnits(4 * U);
    let cases = [
        (0, (0, Backward, len0)),
        (U, (0, Forward, PosUnits(U))),
        (3 * U, (0, Forward, len0)),
        (5 * U, (1, Forward, PosUnits(2 * U))),
        (7 * U - 1, (1, Forward, PosUnits(4 * U - 1))),
        (7 * U, (1, Forward, len1)),
        (8 * U, (1, Backward, PosUnits(U))),
        (11 * U, (1, Backward, len1)),
        (13 * U, (0, Backward, PosUnits(2 * U))),
        (14 * U - 1, (0, Backward, PosUnits(3 * U - 1))),
    ];
    for (c, want) in cases {
        assert_eq!(place_at(&l, c), want, "c = {c}");
    }
}

#[test]
fn place_at_with_pass_stations() {
    let l = pass_at_ends();
    assert_eq!(cycle_len(&l), 14 * U);
    assert_eq!(place_at(&l, 0), (0, Backward, PosUnits(7 * U)));
    assert_eq!(place_at(&l, U), (0, Forward, PosUnits(U)));
    assert_eq!(
        place_at(&l, 3 * U),
        (0, Forward, PosUnits(3 * U)),
        "通過駅 P2 に重なっても止めない"
    );
    assert_eq!(place_at(&l, 7 * U), (0, Forward, PosUnits(7 * U)));
    assert_eq!(place_at(&l, 11 * U), (0, Backward, PosUnits(4 * U)));
}

fn placed(l: &Line, c: u64) -> Train {
    let (section, dir, pos) = place_at(l, c);
    train(0, l, section, dir, pos.0, RUN)
}

#[test]
fn cycle_coord_round_trips_place_at() {
    for l in [three_stops().1, pass_at_ends()] {
        let p = cycle_len(&l);
        for c in (0..p).step_by(997).chain([0, p / 2, p - 1]) {
            assert_eq!(cycle_coord(&l, &placed(&l, c)), c, "c = {c}");
        }
    }
}

#[test]
fn cycle_coord_of_dwelling_trains_is_station() {
    let (_, l) = three_stops();
    let mut tr = train(0, &l, 1, Backward, 0, RUN);
    begin_dwell(&l, &mut tr, Ticks(5));
    assert_eq!(cycle_coord(&l, &tr), 7 * U);
    let mut tr = train(0, &l, 0, Forward, 0, RUN);
    begin_dwell(&l, &mut tr, Ticks(5));
    assert_eq!(cycle_coord(&l, &tr), 0);
    let mut tr = train(0, &l, 0, Backward, 0, RUN);
    begin_dwell(&l, &mut tr, Ticks(5));
    assert_eq!(cycle_coord(&l, &tr), 11 * U);
}

#[test]
fn initial_coords_divide_cycle() {
    let (_, l) = three_stops();
    assert_eq!(initial_coord(&l, 0, 1), 0);
    let p = 14 * U;
    for k in 0..3 {
        assert_eq!(initial_coord(&l, k, 3), u64::from(k) * (p / 3));
    }
    assert_eq!(p % 3, 0);
    let st = stations(&[(2, 5, 0), (3, 5, 0)]);
    let short = line(0, &[0, 1], &st, 0);
    assert_eq!(initial_coord(&short, 6, 7), 6 * (120_000 / 7));
    assert_eq!(120_000 % 7, 6, "切り捨てが起きる例");
}

fn fleet(l: &mut Line, trains: &mut Vec<Train>, onboard: &mut PaxMatrix, c: u64) -> TrainId {
    push_train(trains, onboard, l, place_at(l, c))
}

fn add(l: &mut Line, trains: &mut Vec<Train>, onboard: &mut PaxMatrix) -> u64 {
    let c = added_coord(l, trains, &mut Vec::new());
    fleet(l, trains, onboard, c);
    c
}

/// 1 本 → 2 本 → 4 本で等間隔。同じ大きさの隙間は始まりの座標が小さい方。
#[test]
fn added_trains_fill_largest_gap() {
    let (_, mut l) = three_stops();
    let mut trains = Vec::new();
    let mut onboard = PaxMatrix::new(19);
    let p = cycle_len(&l);
    let c0 = initial_coord(&l, 0, 1);
    fleet(&mut l, &mut trains, &mut onboard, c0);
    assert_eq!(add(&mut l, &mut trains, &mut onboard), p / 2);
    assert_eq!(add(&mut l, &mut trains, &mut onboard), p / 4);
    assert_eq!(add(&mut l, &mut trains, &mut onboard), 3 * p / 4);
    let mut cs: Vec<u64> = trains.iter().map(|t| cycle_coord(&l, t)).collect();
    cs.sort_unstable();
    assert_eq!(cs, [0, p / 4, p / 2, 3 * p / 4]);
    assert_eq!(add(&mut l, &mut trains, &mut onboard), p / 8);
    assert_eq!(l.trains, (0..5).map(TrainId).collect::<Vec<_>>());
    assert_eq!(onboard.rows(), 5);
}

#[test]
fn added_train_with_coincident_and_odd_gaps() {
    let (_, mut l) = three_stops();
    let p = cycle_len(&l);
    let mut trains = Vec::new();
    let mut onboard = PaxMatrix::new(19);
    fleet(&mut l, &mut trains, &mut onboard, U);
    fleet(&mut l, &mut trains, &mut onboard, U);
    assert_eq!(
        add(&mut l, &mut trains, &mut onboard),
        U + p / 2,
        "同じ座標の列車の間の隙間は 0"
    );

    let (_, mut l) = three_stops();
    let mut trains = Vec::new();
    fleet(&mut l, &mut trains, &mut onboard, 0);
    fleet(&mut l, &mut trains, &mut onboard, 1);
    assert_eq!(
        add(&mut l, &mut trains, &mut onboard),
        1 + (p - 1) / 2,
        "隙間の真ん中は切り捨て"
    );

    let (_, mut l) = three_stops();
    let mut trains = Vec::new();
    fleet(&mut l, &mut trains, &mut onboard, 2 * U);
    fleet(&mut l, &mut trains, &mut onboard, 9 * U);
    assert_eq!(
        add(&mut l, &mut trains, &mut onboard),
        2 * U + 7 * U / 2,
        "同じ大きさなら始まりが小さい方"
    );
}

#[test]
fn push_train_assigns_next_id_and_empty_row() {
    let (_, mut l) = three_stops();
    l.id = LineId(2);
    let mut trains = vec![train(0, &l, 0, Forward, 0, RUN)];
    let mut onboard = PaxMatrix::new(19);
    onboard.push_row();
    onboard.add(0, StationId(1), People(4));
    let id = push_train(
        &mut trains,
        &mut onboard,
        &mut l,
        (1, Backward, PosUnits(7)),
    );
    assert_eq!(id, TrainId(1));
    assert_eq!(l.trains, [TrainId(1)]);
    assert_eq!(
        trains[1],
        Train {
            id: TrainId(1),
            line: LineId(2),
            section: 1,
            dir: Backward,
            pos: PosUnits(7),
            phase: RUN,
            riders: People::ZERO,
        }
    );
    assert_eq!(onboard.row_total(1), People::ZERO);
}

/// 置いた直後に停車駅に重なる列車は、同じ tick の手順3で到着する(SIM-D-05)。
#[test]
fn placed_on_stop_arrives_in_same_tick() {
    let (_, l) = three_stops();
    let cases = [(0, 0), (3 * U, 1), (7 * U, 2), (11 * U, 1)];
    for (c, station) in cases {
        let mut tr = placed(&l, c);
        check_inv10(&l, &tr, dwell_max(), true);
        assert!(advance_one(&l, &mut tr), "c = {c}");
        assert_eq!(next_stop(&l, &tr), StationId(station), "c = {c}");
    }
    let mut tr = placed(&l, 0);
    advance_one(&l, &mut tr);
    assert_eq!(
        turn_at_arrival(&l, &mut tr),
        StationId(1),
        "起点では往路へ出発する"
    );
}

fn config() -> ProptestConfig {
    let mut c = ProptestConfig {
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

    #[test]
    fn place_at_is_inverse_of_cycle_coord(gaps in proptest::collection::vec(1u32..8, 1..6), r in any::<u64>()) {
        let mut x = 1;
        let cells: Vec<(i32, i32, u8)> = core::iter::once(0).chain(gaps.iter().copied()).map(|g| {
            x += g as i32;
            (x, 5, 0)
        }).collect();
        let st = stations(&cells);
        let order: Vec<u32> = (0..cells.len() as u32).collect();
        let l = line(0, &order, &st, 0);
        let p = cycle_len(&l);
        prop_assert_eq!(p, 2 * u64::from(gaps.iter().sum::<u32>()) * U);
        let c = r % p;
        let tr = placed(&l, c);
        prop_assert_eq!(cycle_coord(&l, &tr), c);
        let s: &Section = &l.sections[usize::from(tr.section)];
        prop_assert!(tr.pos <= s.length);
        prop_assert_eq!(tr.pos == s.length, find_stop_at_coord(&l, c));
    }
}

fn find_stop_at_coord(l: &Line, c: u64) -> bool {
    let half = cycle_len(l) / 2;
    let x = if c < half { c } else { 2 * half - c };
    l.offsets.iter().any(|&o| u64::from(o) * U == x)
}
