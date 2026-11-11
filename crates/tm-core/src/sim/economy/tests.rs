use super::fixtures::*;
use super::*;
use crate::geom::GridPos;
use crate::routing::Routing;
use crate::sim::topology::fixtures::check_inv10;
use crate::state::{Direction, TrainPhase};
use crate::time::Ticks;
use crate::units::{Millis, PosUnits, ms_to_ticks};

use ParamKey::*;
use UpgradeScope::{AllLines, AllStations};

/// 横一列の 3 駅(6 マス間隔)と路線 [0, 1, 2]。
fn three(c: u8) -> GameState {
    world(
        bal(),
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0)],
        &[(&[0, 1, 2], 0)],
        c,
    )
}

fn ids_of(plan: &PurchasePlan) -> Vec<Target> {
    plan.targets.as_slice().iter().map(|t| t.target).collect()
}

fn buy(st: &mut GameState, b: &BalanceTable, plan: &PurchasePlan) -> Vec<GameEvent> {
    let (mut a, mut c, mut s, mut ev) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    apply_purchase(st, b, plan, &mut a, &mut c, &mut s, &mut ev);
    ev
}

#[test]
fn cost_table_multiplies_step_ratios_in_order() {
    let b = bal();
    let top = CompanyLevel(b.spec().company.max_level);
    for p in ParamKey::ALL {
        let d = b.param(p);
        let mut want = Money::from_u64(d.base_yen);
        for l in 0..b.limit(p, top).0 {
            assert_eq!(b.cost(p, Level(l)).to_bits(), want.to_bits(), "{p:?} Lv{l}");
            let r = d
                .cost_steps
                .iter()
                .rev()
                .find(|s| s.from <= l)
                .map_or(d.cost_ratio, |s| s.ratio);
            want = want.mul_small(u64::from(r));
        }
    }
}

#[test]
fn cost_matches_01_quick_table() {
    let b = bal();
    let c = |p, l| b.cost(p, Level(l));
    assert_eq!(c(S2Fare, 0), yen(5));
    assert_eq!(c(S2Fare, 1), yen(50));
    assert_eq!(c(S2Fare, 3), yen(5_000));
    assert_eq!(c(S2Fare, 5), yen(1_300_000));
    assert_eq!(c(S2Fare, 10), yen(15_445_788_800_000));
    assert_eq!(c(T1Speed, 0), yen(10));
    assert_eq!(c(T1Speed, 1), yen(20));
    assert_eq!(c(T1Speed, 5), yen(320));
    assert_eq!(c(R1MaxTrains, 0), yen(10_000));
    assert_eq!(c(R1MaxTrains, 1), yen(30_000));
    assert_eq!(c(R1MaxTrains, 5), yen(2_430_000));
}

#[test]
fn scope_mismatch_comes_before_unknown_ids() {
    let st = three(0);
    let b = bal();
    let err = |p, scope| plan_purchase(&st, b, p, scope, BuyMode::One).unwrap_err();
    assert_eq!(
        err(S1Spawn, UpgradeScope::Line(LineId(99))),
        CommandError::ScopeMismatch {
            param: S1Spawn,
            scope: UpgradeScope::Line(LineId(99))
        }
    );
    assert!(matches!(
        err(R2LineSlots, AllLines),
        CommandError::ScopeMismatch { .. }
    ));
    assert!(matches!(
        err(T1Speed, UpgradeScope::Company),
        CommandError::ScopeMismatch { .. }
    ));
    assert_eq!(
        err(S2Fare, UpgradeScope::Station(sid(9))),
        CommandError::UnknownStation(sid(9))
    );
    assert_eq!(
        err(T1Speed, UpgradeScope::Line(LineId(1))),
        CommandError::UnknownLine(LineId(1))
    );
}

#[test]
fn locked_comes_before_targets() {
    let mut st = three(0);
    st.station_plan[3] = GridPos { x: 20, y: 9 };
    crate::sim::company::spawn_station(&mut st, &mut Vec::new());
    let b = bal();
    assert_eq!(
        plan_purchase(
            &st,
            b,
            S3StationCap,
            UpgradeScope::Station(sid(3)),
            BuyMode::One
        ),
        Err(CommandError::Locked {
            feature: Feature::Param(S3StationCap),
            required: CompanyLevel(1)
        })
    );
    assert_eq!(
        plan_purchase(&st, b, S2Fare, UpgradeScope::Station(sid(3)), BuyMode::One),
        Err(CommandError::StationNotConnected(sid(3)))
    );
    assert_eq!(
        plan_purchase(
            &st,
            b,
            S3StationCap,
            UpgradeScope::Station(sid(9)),
            BuyMode::One
        ),
        Err(CommandError::UnknownStation(sid(9)))
    );
}

#[test]
fn bulk_buy_needs_its_unlock() {
    let b = custom(|s| {
        for f in &mut s.features {
            if f.feature == Feature::BulkBuy {
                f.unlock = 1;
            }
        }
    });
    let mut st = world(&b, &[(2, 5, 0), (8, 5, 0)], &[(&[0, 1], 0)], 0);
    st.money = yen(1000);
    let locked = Err(CommandError::Locked {
        feature: Feature::BulkBuy,
        required: CompanyLevel(1),
    });
    assert_eq!(
        plan_purchase(&st, &b, S2Fare, AllStations, BuyMode::One),
        locked
    );
    assert_eq!(
        plan_purchase(&st, &b, T1Speed, AllLines, BuyMode::One),
        locked
    );
    let one = UpgradeScope::Station(sid(0));
    assert_eq!(plan_purchase(&st, &b, S2Fare, one, BuyMode::Max), locked);
    assert!(plan_purchase(&st, &b, S2Fare, one, BuyMode::One).is_ok());
}

#[test]
fn no_target_without_lines() {
    let st = world(bal(), &[(2, 5, 0), (8, 5, 0), (14, 5, 0)], &[], 0);
    let b = bal();
    assert_eq!(
        plan_purchase(&st, b, S2Fare, AllStations, BuyMode::One),
        Err(CommandError::NoTarget { param: S2Fare })
    );
    assert_eq!(
        plan_purchase(&st, b, T4CarCapacity, AllLines, BuyMode::Max),
        Err(CommandError::NoTarget {
            param: T4CarCapacity
        })
    );
}

#[test]
fn at_level_limit_kinds() {
    let b = bal();
    let mut st = three(0);
    st.money = yen(1_000_000);
    for s in 0..3 {
        set_station(&mut st, s, S2Fare, 4);
    }
    assert_eq!(
        plan_purchase(&st, b, S2Fare, AllStations, BuyMode::Max),
        Err(CommandError::AtLevelLimit {
            param: S2Fare,
            limit: Level(4),
            kind: LimitKind::CompanyCap
        })
    );

    let mut st = three(3);
    set_line(&mut st, b, 0, T1Speed, 10);
    assert_eq!(
        plan_purchase(&st, b, T1Speed, UpgradeScope::Line(LineId(0)), BuyMode::One),
        Err(CommandError::AtLevelLimit {
            param: T1Speed,
            limit: Level(10),
            kind: LimitKind::Intrinsic
        })
    );

    let st = world(
        b,
        &[(2, 5, 0), (8, 5, 0), (2, 9, 8), (8, 9, 8)],
        &[(&[0, 1], 1), (&[2, 3], 9)],
        3,
    );
    assert_eq!(
        plan_purchase(&st, b, T5Cars, UpgradeScope::Line(LineId(1)), BuyMode::One),
        Err(CommandError::AtLevelLimit {
            param: T5Cars,
            limit: Level(9),
            kind: LimitKind::Intrinsic
        })
    );
    assert_eq!(
        plan_purchase(&st, b, T5Cars, AllLines, BuyMode::One),
        Err(CommandError::AtLevelLimit {
            param: T5Cars,
            limit: Level(1),
            kind: LimitKind::Platform
        })
    );
}

/// M9: S3 の解放後、駅の S1 は S3 のレベルまで。解放の前(会社 Lv0)は上限 cap(0) まで。
#[test]
fn spawn_waits_for_station_capacity_m9() {
    let b = bal();
    let mut st = three(0);
    st.money = Money::pow10(20);
    let max = |st: &GameState, scope| plan_purchase(st, b, S1Spawn, scope, BuyMode::Max);
    let p = max(&st, AllStations).unwrap();
    assert!(p.targets.as_slice().iter().all(|t| t.cur == Level(4)));

    let mut st = three(1);
    st.money = Money::pow10(20);
    for s in 0..3 {
        set_station(&mut st, s, S1Spawn, 4);
    }
    assert_eq!(
        max(&st, AllStations),
        Err(CommandError::AtLevelLimit {
            param: S1Spawn,
            limit: Level(0),
            kind: LimitKind::Capacity
        })
    );
    set_station(&mut st, 1, S3StationCap, 5);
    set_station(&mut st, 2, S3StationCap, 6);
    let p = max(&st, AllStations).unwrap();
    assert_eq!(
        ids_of(&p),
        [
            Target::Station(sid(0)),
            Target::Station(sid(1)),
            Target::Station(sid(2))
        ]
    );
    let lv: Vec<Level> = p.targets.as_slice().iter().map(|t| t.cur).collect();
    assert_eq!(lv, [Level(4), Level(5), Level(6)]);
    assert_eq!((p.steps, p.raised), (2, 3));
    set_station(&mut st, 0, S3StationCap, 6);
    assert_eq!(
        plan_purchase(&st, b, S1Spawn, UpgradeScope::Station(sid(0)), BuyMode::Max)
            .unwrap()
            .targets
            .as_slice()[0]
            .cur,
        Level(6),
        "S3 が上限 cap(1) = 6 なら、S1 も cap まで"
    );
    set_station(&mut st, 0, S1Spawn, 6);
    assert_eq!(
        plan_purchase(&st, b, S1Spawn, UpgradeScope::Station(sid(0)), BuyMode::One),
        Err(CommandError::AtLevelLimit {
            param: S1Spawn,
            limit: Level(6),
            kind: LimitKind::CompanyCap
        }),
        "S3 と会社の上限が同じなら会社の上限"
    );
    set_station(&mut st, 1, S1Spawn, 5);
    set_station(&mut st, 2, S1Spawn, 6);
    assert_eq!(
        max(&st, AllStations),
        Err(CommandError::AtLevelLimit {
            param: S1Spawn,
            limit: Level(5),
            kind: LimitKind::Capacity
        }),
        "会社の上限と S3 の上限が混ざったら S3 の上限(先に上げられる方)"
    );
}

#[test]
fn platform_beats_company_cap() {
    let b = custom(|s| s.params[T5Cars.index()].unlock = 0);
    let st = world(
        &b,
        &[(2, 5, 4), (8, 5, 4), (2, 9, 0), (8, 9, 0)],
        &[(&[0, 1], 4), (&[2, 3], 1)],
        0,
    );
    let one = |l| plan_purchase(&st, &b, T5Cars, UpgradeScope::Line(LineId(l)), BuyMode::One);
    assert_eq!(
        one(0),
        Err(CommandError::AtLevelLimit {
            param: T5Cars,
            limit: Level(4),
            kind: LimitKind::CompanyCap
        })
    );
    assert_eq!(
        plan_purchase(&st, &b, T5Cars, AllLines, BuyMode::One),
        Err(CommandError::AtLevelLimit {
            param: T5Cars,
            limit: Level(1),
            kind: LimitKind::Platform
        })
    );
}

#[test]
fn insufficient_funds_reports_step_cost() {
    let mut st = three(0);
    st.money = yen(14);
    assert_eq!(
        plan_purchase(&st, bal(), S2Fare, AllStations, BuyMode::Max),
        Err(CommandError::InsufficientFunds {
            required: yen(15),
            available: yen(14)
        })
    );
}

#[test]
fn one_and_max_subtract_each_step() {
    let b = bal();
    let mut st = three(0);
    let plan = |st: &GameState, mode| plan_purchase(st, b, S2Fare, AllStations, mode).unwrap();

    st.money = yen(160);
    let p = plan(&st, BuyMode::Max);
    assert_eq!(
        (p.steps, p.raised, p.cost, p.money_after),
        (1, 3, yen(15), yen(145))
    );

    st.money = yen(1000);
    let p = plan(&st, BuyMode::Max);
    assert_eq!(
        (p.steps, p.raised, p.cost, p.money_after),
        (2, 6, yen(165), yen(835))
    );
    let p = plan(&st, BuyMode::One);
    assert_eq!(
        (p.steps, p.raised, p.cost, p.money_after),
        (1, 3, yen(15), yen(985))
    );
    assert!(p.targets.as_slice().iter().all(|t| t.cur == Level(1)));

    st.money = Money::pow10(30);
    let p = plan(&st, BuyMode::Max);
    assert_eq!((p.steps, p.raised, p.cost), (4, 12, yen(3 * 5555)));
    let after = [5, 50, 500, 5000]
        .iter()
        .fold(st.money, |m, &c| m.checked_sub(yen(3 * c)).unwrap());
    assert_eq!(p.money_after.to_bits(), after.to_bits());
}

#[test]
fn max_raises_only_targets_below_limit() {
    let mut st = three(0);
    for (s, l) in [(0, 0), (1, 2), (2, 4)] {
        set_station(&mut st, s, S2Fare, l);
    }
    st.money = Money::pow10(9);
    let p = plan_purchase(&st, bal(), S2Fare, AllStations, BuyMode::Max).unwrap();
    assert_eq!((p.steps, p.raised), (4, 6));
    assert_eq!(p.cost, yen(5 + 500 + 50 + 5000 + 500 + 5000));
    assert!(p.targets.as_slice().iter().all(|t| t.cur == Level(4)));
}

#[test]
fn single_station_and_line() {
    let b = bal();
    let mut st = three(0);
    st.money = yen(100);
    let p = plan_purchase(&st, b, S1Spawn, UpgradeScope::Station(sid(1)), BuyMode::One).unwrap();
    assert_eq!(ids_of(&p), [Target::Station(sid(1))]);
    assert_eq!(
        (p.cost, p.preview().steps, p.preview().raised),
        (yen(5), 1, 1)
    );
    st.money = yen(110);
    let p = plan_purchase(
        &st,
        b,
        T4CarCapacity,
        UpgradeScope::Line(LineId(0)),
        BuyMode::Max,
    )
    .unwrap();
    assert_eq!(ids_of(&p), [Target::Line(LineId(0))]);
    assert_eq!((p.steps, p.cost), (2, yen(110)));
}

/// 横一列の 7 駅と路線 [0..=6]、会社 Lv6。駅 5・6 は会社 Lv3・4 で出た駅(M8 の値段の下限)。
fn seven() -> GameState {
    let cells: Vec<(i32, i32, u8)> = (0..7).map(|i| (2 + 6 * i, 5, 0)).collect();
    world(bal(), &cells, &[(&[0, 1, 2, 3, 4, 5, 6], 0)], 6)
}

#[test]
fn late_station_costs_start_at_the_floor() {
    let b = bal();
    let mut st = seven();
    st.money = Money::pow10(40);
    for s in 0..7 {
        set_station(&mut st, s, S3StationCap, 16);
    }
    let one = |st: &GameState, s| {
        plan_purchase(st, b, S1Spawn, UpgradeScope::Station(sid(s)), BuyMode::One)
            .unwrap()
            .cost
    };
    for s in 0..=4 {
        assert_eq!(one(&st, s), b.cost(S1Spawn, Level(0)), "S{s}");
    }
    assert_eq!(one(&st, 5), b.cost(S1Spawn, Level(4)));
    assert_eq!(one(&st, 6), b.cost(S1Spawn, Level(6)));
    set_station(&mut st, 6, S1Spawn, 9);
    assert_eq!(one(&st, 6), b.cost(S1Spawn, Level(9)));

    let all = plan_purchase(&st, b, S2Fare, AllStations, BuyMode::One).unwrap();
    let want = (0..5).fold(
        b.cost(S2Fare, Level(4))
            .saturating_add(b.cost(S2Fare, Level(6))),
        |m, _| m.saturating_add(b.cost(S2Fare, Level(0))),
    );
    assert_eq!(all.cost, want);

    set_station(&mut st, 6, S3StationCap, 0);
    let max = plan_purchase(
        &st,
        b,
        S3StationCap,
        UpgradeScope::Station(sid(6)),
        BuyMode::Max,
    )
    .unwrap();
    let want = (0..16).fold(Money::ZERO, |m, l| {
        m.saturating_add(b.cost(S3StationCap, Level(l.max(6))))
    });
    assert_eq!((max.steps, max.cost), (16, want));
}

#[test]
fn late_line_costs_start_at_the_floor_but_finite_params_do_not() {
    let b = bal();
    let mut st = seven();
    st.company.level = CompanyLevel(8);
    st.progress.unlocked = crate::sim::company::unlocked_at(b, CompanyLevel(8));
    st.lines[0].born_level = CompanyLevel(7);
    st.money = Money::pow10(40);
    let one = |p| {
        plan_purchase(&st, b, p, UpgradeScope::Line(LineId(0)), BuyMode::One)
            .unwrap()
            .cost
    };
    assert_eq!(one(T4CarCapacity), b.cost(T4CarCapacity, Level(12)));
    assert_eq!(one(T3Boarding), b.cost(T3Boarding, Level(12)));
    assert_eq!(one(T1Speed), b.cost(T1Speed, Level(0)));
    assert_eq!(one(R1MaxTrains), b.cost(R1MaxTrains, Level(0)));
    st.lines[0].born_level = CompanyLevel(6);
    assert_eq!(
        plan_purchase(
            &st,
            b,
            T3Boarding,
            UpgradeScope::Line(LineId(0)),
            BuyMode::One
        )
        .unwrap()
        .cost,
        b.cost(T3Boarding, Level(0)),
        "T3 は会社 Lv4 で解放。Lv6 で作った路線は 6 − 3 < 4 なので下限なし"
    );
}

#[test]
fn t5_stops_at_shortest_platform() {
    let b = bal();
    let mut st = world(
        b,
        &[(2, 5, 1), (8, 5, 3), (14, 5, 2)],
        &[(&[0, 1, 2], 0)],
        2,
    );
    st.money = Money::pow10(20);
    let p = plan_purchase(&st, b, T5Cars, AllLines, BuyMode::Max).unwrap();
    assert_eq!((p.steps, p.targets.as_slice()[0].cur), (2, Level(2)));
    assert_eq!(p.targets.as_slice()[0].kind, LimitKind::Platform);
}

#[test]
fn apply_r1_adds_trains_step_then_line() {
    let b = bal();
    let mut st = world(
        b,
        &[(2, 5, 0), (8, 5, 0), (2, 9, 0), (8, 9, 0)],
        &[(&[0, 1], 0), (&[2, 3], 0)],
        3,
    );
    set_line(&mut st, b, 1, R1MaxTrains, 1);
    assert_eq!(st.trains.len(), 3);
    st.money = yen(200_000);
    let plan = plan_purchase(&st, b, R1MaxTrains, AllLines, BuyMode::Max).unwrap();
    assert_eq!((plan.steps, plan.cost), (2, yen(160_000)));
    let ev = buy(&mut st, b, &plan);
    let added = |t, l| GameEvent::TrainAdded {
        train: TrainId(t),
        line: LineId(l),
    };
    assert_eq!(
        ev,
        [
            GameEvent::Purchased {
                param: R1MaxTrains,
                scope: AllLines,
                mode: BuyMode::Max,
                steps: 2,
                raised: 4,
                cost: yen(160_000)
            },
            added(3, 0),
            added(4, 1),
            added(5, 0),
            added(6, 1),
        ]
    );
    assert_eq!(st.money, yen(40_000));
    assert_eq!(st.lines[0].trains.len(), 3);
    assert_eq!(st.lines[1].trains.len(), 4);
    assert_eq!(st.onboard.rows(), 7);
    assert!(st.progress.upgraded_once.contains(R1MaxTrains));
    assert_eq!(st.stats.purchases, 1);
    assert!(!st.routing.is_dirty());
}

#[test]
fn apply_s4_resolves_pass_through_in_line_order() {
    let b = bal();
    let mut st = world(
        b,
        &[
            (2, 5, 1),
            (6, 5, 0),
            (10, 5, 0),
            (14, 5, 1),
            (10, 1, 1),
            (10, 9, 1),
        ],
        &[(&[0, 1, 2, 3], 2), (&[4, 2, 5], 2)],
        2,
    );
    assert_eq!(st.lines[0].stops, [true, false, false, true]);
    assert_eq!(st.lines[1].stops, [true, false, true]);
    st.money = Money::pow10(30);
    let rev = st.map_revision;
    let plan = plan_purchase(&st, b, S4Platform, AllStations, BuyMode::Max).unwrap();
    assert_eq!(plan.steps, 8);
    let ev = buy(&mut st, b, &plan);
    let resolved = |l, s| GameEvent::PassThroughResolved {
        line: LineId(l),
        station: sid(s),
    };
    assert!(matches!(ev[0], GameEvent::Purchased { steps: 8, .. }));
    assert_eq!(ev[1..], [resolved(0, 1), resolved(0, 2), resolved(1, 2)]);
    assert!(st.lines.iter().all(|l| l.stops.iter().all(|&x| x)));
    assert_eq!(st.lines[0].sections.len(), 3);
    assert_eq!(st.map_revision, rev + 1);
    assert!(st.routing.is_dirty());
    let dwell_max = ms_to_ticks(Millis(b.spec().transit.dwell_max_ms));
    for tr in &st.trains {
        check_inv10(&st.lines[tr.line.index()], tr, dwell_max, true);
    }
}

#[test]
fn apply_s4_without_change_keeps_shape() {
    let b = bal();
    let mut st = three(2);
    st.money = yen(10_000);
    let before = st.lines.clone();
    let plan = plan_purchase(&st, b, S4Platform, AllStations, BuyMode::One).unwrap();
    let ev = buy(&mut st, b, &plan);
    assert_eq!(ev.len(), 1);
    assert_eq!(st.lines, before);
    assert_eq!(st.map_revision, 0);
    assert!(!st.routing.is_dirty());
    assert!(st.stations.iter().all(|s| s.levels.platform == Level(1)));
}

#[test]
fn apply_s4_moves_dwelling_train_to_front_half() {
    let b = bal();
    let mut st = world(
        b,
        &[(2, 5, 1), (6, 5, 0), (10, 5, 1)],
        &[(&[0, 1, 2], 2)],
        2,
    );
    let tr = &mut st.trains[0];
    tr.section = 0;
    tr.dir = Direction::Forward;
    tr.pos = PosUnits(0);
    tr.phase = TrainPhase::Dwelling {
        at: 0,
        remaining: Ticks(7),
    };
    st.money = yen(10_000);
    let plan = plan_purchase(
        &st,
        b,
        S4Platform,
        UpgradeScope::Station(sid(1)),
        BuyMode::One,
    )
    .unwrap();
    buy(&mut st, b, &plan);
    let tr = st.trains[0];
    assert_eq!((tr.section, tr.pos), (0, PosUnits(0)));
    assert_eq!(st.lines[0].sections[0].to, 1);
    assert_eq!(
        tr.phase,
        TrainPhase::Dwelling {
            at: 0,
            remaining: Ticks(7)
        }
    );
}

#[test]
fn apply_marks_routes_and_slots() {
    let b = bal();
    let mut st = three(1);
    st.money = Money::pow10(10);
    for (p, scope, dirty) in [
        (S2Fare, AllStations, false),
        (T4CarCapacity, AllLines, false),
        (T1Speed, AllLines, true),
    ] {
        st.routing = Routing::new(&st, b);
        let plan = plan_purchase(&st, b, p, scope, BuyMode::One).unwrap();
        buy(&mut st, b, &plan);
        assert_eq!(st.routing.is_dirty(), dirty, "{p:?}");
    }
    let plan = plan_purchase(&st, b, R2LineSlots, UpgradeScope::Company, BuyMode::Max).unwrap();
    buy(&mut st, b, &plan);
    assert_eq!(st.company.line_slots, Level(5));
    assert_eq!(st.stats.purchases, 4);
    assert_eq!(st.lines[0].levels.speed, Level(1));
    assert_eq!(st.stations[2].levels.fare, Level(1));
}

#[test]
fn book_and_window() {
    let mut st = three(0);
    book_income(&mut st, yen(7));
    book_income(&mut st, yen(5));
    assert_eq!(
        (st.money, st.progress.total_earned, st.stats.income_this_sec),
        (yen(12), yen(12), yen(12))
    );
    roll_income_window(&mut st);
    assert_eq!(
        (st.stats.income_window[0], st.stats.window_head),
        (yen(12), 1)
    );
    assert!(st.stats.income_this_sec.is_zero());
    assert_eq!(income_per_sec(&st), yen(12).div_small(10));
    for i in 1..=10 {
        book_income(&mut st, yen(i));
        roll_income_window(&mut st);
    }
    assert_eq!(st.stats.window_head, 1);
    assert_eq!(st.stats.income_window[0], yen(10));
    assert_eq!(income_per_sec(&st), yen(55).div_small(10));
}

#[test]
fn milestones_in_unit_order_and_goal_once() {
    let b = bal();
    let mut st = three(0);
    let mut ev = Vec::new();
    check_money_milestones(&mut st, b, &mut ev);
    assert!(ev.is_empty());
    book_income(&mut st, Money::pow10(9));
    check_money_milestones(&mut st, b, &mut ev);
    assert_eq!(
        ev,
        [
            GameEvent::MoneyUnitReached { unit: 1 },
            GameEvent::MoneyUnitReached { unit: 2 }
        ]
    );
    ev.clear();
    st.money = Money::ZERO;
    book_income(&mut st, Money::pow10(68));
    check_money_milestones(&mut st, b, &mut ev);
    assert_eq!(ev.len(), 16);
    assert_eq!(ev[14], GameEvent::MoneyUnitReached { unit: 17 });
    assert!(matches!(ev[15], GameEvent::GoalReached { .. }));
    ev.clear();
    check_money_milestones(&mut st, b, &mut ev);
    assert!(ev.is_empty());
    assert!(st.progress.goal_reached);
}

#[test]
fn quote_matches_plan() {
    let b = bal();
    let mut st = three(0);
    st.money = yen(12);
    let q = quote(&st, b, S2Fare).unwrap();
    assert_eq!(
        (q.step, q.affordable, q.scope),
        (yen(15), false, AllStations)
    );
    assert!((0..3).all(|i| q.raised.contains(i)));
    let q = quote(&st, b, T1Speed).unwrap();
    assert_eq!((q.step, q.affordable), (yen(10), true));
    assert!(q.raised.contains(0) && !q.raised.contains(1));
    assert_eq!(quote(&st, b, S3StationCap), None);
    for s in 0..2 {
        set_station(&mut st, s, S1Spawn, 4);
    }
    let q = quote(&st, b, S1Spawn).unwrap();
    assert_eq!(q.step, yen(5));
    assert!(!q.raised.contains(0) && q.raised.contains(2));
}

#[test]
fn checked_sub_never_negative() {
    let mut st = three(0);
    st.money = yen(15);
    let p = plan_purchase(&st, bal(), S2Fare, AllStations, BuyMode::Max).unwrap();
    assert!(p.money_after.is_zero());
}

#[test]
fn level_targets_ignore_unconnected() {
    let b = bal();
    let mut st = three(0);
    crate::sim::company::spawn_station(&mut st, &mut Vec::new());
    st.money = yen(1000);
    let p = plan_purchase(&st, b, S1Spawn, AllStations, BuyMode::One).unwrap();
    assert_eq!(p.raised, 3);
    let plan = p.clone();
    buy(&mut st, b, &plan);
    assert_eq!(st.stations[3].levels.spawn, Level::ZERO);
}
