use super::fixtures::*;
use super::*;
use crate::recommend::{
    EconSnapshot, IdSet, Quote, RecommendReason, SegTiles, estimate_income, recommend,
};

use ParamKey::*;

fn f(m: Money) -> f64 {
    f64::from_bits(m.to_bits())
}

fn rel_err(got: f64, want: f64) -> f64 {
    (got - want).abs() / want.abs().max(1e-300)
}

fn three() -> GameState {
    world(
        bal(),
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0)],
        &[(&[0, 1, 2], 0)],
        0,
    )
}

/// 2 路線・4 駅。路線 0 の 3 駅目はホームが短く通過駅。
fn two_lines() -> GameState {
    let b = bal();
    let mut st = world(
        b,
        &[(2, 4, 1), (7, 4, 1), (11, 4, 0), (7, 9, 1), (14, 4, 1)],
        &[(&[0, 1, 2, 4], 2), (&[3, 1, 0], 0)],
        4,
    );
    for (s, sp, fare, cap) in [(0, 3, 2, 1), (1, 1, 4, 3), (3, 2, 0, 2), (4, 0, 1, 0)] {
        set_station(&mut st, s, S1Spawn, sp);
        set_station(&mut st, s, S2Fare, fare);
        set_station(&mut st, s, S3StationCap, cap);
    }
    set_line(&mut st, b, 0, T1Speed, 3);
    set_line(&mut st, b, 0, T2Crowding, 2);
    set_line(&mut st, b, 0, T3Boarding, 1);
    set_line(&mut st, b, 0, R1MaxTrains, 2);
    set_line(&mut st, b, 1, T4CarCapacity, 2);
    set_line(&mut st, b, 1, R3Attractiveness, 4);
    set_line(&mut st, b, 0, R3Attractiveness, 1);
    st
}

/// 01 付録 A.1 をそのまま実数で書いたもの(同じ演算順)。整数の値は表と一致する状態だけで使う。
fn naive_a1(st: &GameState, m: f64) -> f64 {
    let pow2 = |l: u8| (0..l).fold(1.0f64, |a, _| a * 2.0);
    let connected = st.stations.iter().filter(|s| s.is_connected()).count() as f64;
    let h = (connected / 3.0).clamp(1.0, 4.0);
    let mut income = 0.0;
    for line in &st.lines {
        let lv = line.levels;
        let cars = 1.0 + f64::from(lv.cars.0);
        let stop = |s: StationId, l: &Line| {
            2.0 + f64::from(st.stations[s.index()].levels.platform.0)
                >= 1.0 + f64::from(l.levels.cars.0)
        };
        let stops: Vec<usize> = (0..line.stations.len())
            .filter(|&i| stop(line.stations[i], line))
            .collect();
        if stops.len() < 2 {
            continue;
        }
        let a = |s: StationId| {
            st.stations[s.index()]
                .serving_lines
                .iter()
                .map(|&o| &st.lines[o.index()])
                .filter(|l| stop(s, l))
                .map(|l| 1.0 + 0.2 * f64::from(l.levels.attractiveness.0))
                .fold(0.0, f64::max)
        };
        let s_n = stops.len() as f64;
        let v = 1.0 + 0.5 * f64::from(lv.speed.0);
        let trains = 1.0 + f64::from(lv.max_trains.0);
        let cap = 10.0 * pow2(lv.car_capacity.0) * cars * (1.0 + 0.25 * f64::from(lv.crowding.0));
        let b = 5.0 * pow2(lv.boarding.0);
        let t_run = (stops[stops.len() - 1] - stops[0]) as f64 * m / v;
        let mut x = 0.0;
        let mut supply = 0.0;
        for _ in 0..3 {
            let t_one = t_run + s_n * (1.0 + x / b).min(10.0);
            let headway = 2.0 * t_one / trains;
            supply = 0.0;
            for &i in &stops {
                let sl = st.stations[line.stations[i].index()].levels;
                let spawn = 0.5 * pow2(sl.spawn.0) * a(line.stations[i]);
                supply += spawn.min(10.0 * pow2(sl.capacity.0) / headway);
            }
            x = cap.min(supply * h * headway / s_n);
        }
        let t_one = t_run + s_n * (1.0 + x / b).min(10.0);
        let arrivals = (supply * h).min(trains * cap * (s_n - 1.0) / t_one);
        let fares: f64 = stops
            .iter()
            .map(|&i| {
                (0..st.stations[line.stations[i].index()].levels.fare.0).fold(1.0, |a, _| a * 10.0)
            })
            .sum();
        income += arrivals * fares / s_n;
    }
    income
}

#[test]
fn estimator_hand_value_of_a1() {
    let st = three();
    let sn = EconSnapshot::of(&st);
    let fixed = f(estimate_income(&sn, bal(), SegTiles::Fixed(6)));
    assert!(rel_err(fixed, 5.0 / 7.0) < 1e-12, "{fixed}");
    let actual = f(estimate_income(&sn, bal(), SegTiles::Actual));
    assert!(rel_err(actual, 5.0 / 7.0) < 1e-12, "{actual}");
}

#[test]
fn estimator_matches_plain_a1() {
    let mut big = three();
    let b = bal();
    big.company.level = CompanyLevel(4);
    for s in 0..3 {
        set_station(&mut big, s, S1Spawn, 6);
        set_station(&mut big, s, S3StationCap, 3);
    }
    set_line(&mut big, b, 0, T3Boarding, 2);
    set_line(&mut big, b, 0, T1Speed, 4);
    let cells: Vec<(i32, i32, u8)> = (0..13).map(|i| (2 + 2 * i, 7, 0)).collect();
    let order: Vec<u32> = (0..13).collect();
    let mut long = world(b, &cells, &[(&order, 0)], 4);
    set_line(&mut long, b, 0, T4CarCapacity, 8);
    set_line(&mut long, b, 0, T3Boarding, 6);
    for st in [three(), two_lines(), big, long] {
        let got = f(estimate_income(
            &EconSnapshot::of(&st),
            b,
            SegTiles::Fixed(6),
        ));
        let want = naive_a1(&st, 6.0);
        assert!(rel_err(got, want) < 1e-12, "{got} {want}");
        assert!(got > 0.0);
    }
}

#[test]
fn estimator_golden_bits() {
    let st = two_lines();
    let v = estimate_income(&EconSnapshot::of(&st), bal(), SegTiles::Actual);
    assert_eq!(v.to_bits(), GOLDEN_TWO_LINES, "{}", f(v));
}

const GOLDEN_TWO_LINES: u64 = 4_674_595_758_340_626_143;

#[test]
fn estimator_uses_actual_tiles() {
    let st = world(
        bal(),
        &[(2, 5, 0), (5, 5, 0), (8, 5, 0)],
        &[(&[0, 1, 2], 0)],
        0,
    );
    let sn = EconSnapshot::of(&st);
    let fixed3 = estimate_income(&sn, bal(), SegTiles::Fixed(3));
    assert_eq!(estimate_income(&sn, bal(), SegTiles::Actual), fixed3);
    assert_ne!(estimate_income(&sn, bal(), SegTiles::Fixed(6)), fixed3);

    let st = world(
        bal(),
        &[(2, 5, 0), (5, 5, 1), (8, 5, 1)],
        &[(&[0, 1, 2], 2)],
        2,
    );
    assert_eq!(st.lines[0].stops, [false, true, true]);
    let sn = EconSnapshot::of(&st);
    assert_eq!(
        estimate_income(&sn, bal(), SegTiles::Actual),
        estimate_income(&sn, bal(), SegTiles::Fixed(3))
    );
}

#[test]
fn bumped_snapshot_changes_only_targets() {
    let b = bal();
    let mut st = two_lines();
    st.money = Money::pow10(40);
    let base = estimate_income(&EconSnapshot::of(&st), b, SegTiles::Actual);
    let est = |p| {
        let q = quote(&st, b, p).unwrap();
        estimate_income(&EconSnapshot::of(&st).bumped(&q), b, SegTiles::Actual)
    };
    for p in [S2Fare, T1Speed, R1MaxTrains, R3Attractiveness, S1Spawn] {
        assert_eq!(est(p).total_cmp(&base), Ordering::Greater, "{p:?}");
    }
    let q = quote(&st, b, S4Platform).unwrap();
    let sn = EconSnapshot::of(&st).bumped(&q);
    assert_eq!(sn.station_levels(sid(2)).platform, Level(1));
    assert_eq!(sn.station_levels(sid(2)).fare, Level(0));
    assert_eq!(sn.line_levels(LineId(0)), st.lines[0].levels);
    assert_ne!(est(S4Platform), base);
    let q = quote(&st, b, T5Cars).unwrap();
    assert!(q.raised.contains(1) && !q.raised.contains(0));
    assert_eq!(
        EconSnapshot::of(&st).bumped(&q).line_levels(LineId(1)).cars,
        Level(1)
    );
}

fn rec_of(st: &GameState) -> Option<Recommendation> {
    recommendation(st, bal())
}

fn pick(st: &GameState) -> (ParamKey, RecommendReason, bool) {
    let r = rec_of(st).expect("おすすめがある");
    (r.param, r.reason, r.affordable)
}

#[test]
fn nothing_to_recommend() {
    let st = world(bal(), &[(2, 5, 0), (8, 5, 0), (14, 5, 0)], &[], 0);
    assert_eq!(rec_of(&st), None);
}

#[test]
fn only_line_slots_is_cheapest() {
    let st = world(
        bal(),
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0), (8, 9, 0)],
        &[],
        1,
    );
    let r = rec_of(&st).unwrap();
    assert_eq!(
        r,
        Recommendation {
            param: R2LineSlots,
            scope: UpgradeScope::Company,
            affordable: false,
            reason: RecommendReason::Cheapest
        }
    );
}

#[test]
fn need_line_slot_only_when_affordable() {
    let mut st = world(
        bal(),
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0), (8, 9, 0)],
        &[(&[0, 1, 2], 0)],
        1,
    );
    st.money = yen(1000);
    assert_eq!(
        pick(&st),
        (R2LineSlots, RecommendReason::NeedLineSlot, true)
    );
    st.money = yen(999);
    assert_eq!(pick(&st), (T1Speed, RecommendReason::New, true));
    st.money = yen(1000);
    st.company.line_slots = Level(1);
    assert_eq!(pick(&st), (T1Speed, RecommendReason::New, true));
}

#[test]
fn new_is_cheapest_affordable_new() {
    let mut st = world(
        bal(),
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0)],
        &[(&[0, 1, 2], 0)],
        1,
    );
    for p in [S1Spawn, S2Fare, T1Speed, T4CarCapacity] {
        st.progress.upgraded_once.insert(p);
    }
    st.money = yen(150);
    assert_eq!(pick(&st), (S3StationCap, RecommendReason::New, true));
    st.money = yen(149);
    assert_eq!(pick(&st).1, RecommendReason::BestValue);
}

#[test]
fn best_value_by_income_per_cost() {
    let mut st = three();
    st.progress.upgraded_once = all_upgraded();
    st.money = yen(1000);
    assert_eq!(pick(&st), (S2Fare, RecommendReason::BestValue, true));
    st.money = yen(14);
    assert_eq!(pick(&st), (T1Speed, RecommendReason::BestValue, true));
    st.money = Money::ZERO;
    assert_eq!(pick(&st), (S2Fare, RecommendReason::BestValue, false));
}

#[test]
fn best_value_is_the_argmax() {
    let b = bal();
    let mut st = two_lines();
    st.progress.upgraded_once = all_upgraded();
    for money in [yen(0), yen(3000), yen(100_000), Money::pow10(12)] {
        st.money = money;
        let quotes = ParamKey::ALL.map(|p| quote(&st, b, p));
        let any = quotes.iter().flatten().any(|q| q.affordable);
        let base = estimate_income(&EconSnapshot::of(&st), b, SegTiles::Actual);
        let mut best: Option<(f64, f64, ParamKey)> = None;
        for q in quotes.iter().flatten().filter(|q| q.affordable || !any) {
            let e = if q.param == R2LineSlots {
                0.0
            } else {
                let up = estimate_income(&EconSnapshot::of(&st).bumped(q), b, SegTiles::Actual);
                (f(up) - f(base)).max(0.0) / f(q.step)
            };
            let s = f(q.step);
            if best.is_none_or(|(be, bs, _)| e > be || (e == be && s < bs)) {
                best = Some((e, s, q.param));
            }
        }
        let (e, _, p) = best.unwrap();
        let r = rec_of(&st).unwrap();
        assert!(e > 0.0);
        assert_eq!(
            (r.param, r.reason),
            (p, RecommendReason::BestValue),
            "{money:?}"
        );
    }
}

/// S4(通過駅がない)と R2 だけが候補で、どちらも収入を増やさない。
fn only_zero_value(r2: u8) -> GameState {
    let b = bal();
    let mut st = world(b, &[(2, 5, 2), (8, 5, 0)], &[(&[0, 1], 1)], 2);
    for s in 0..2 {
        for p in [S1Spawn, S2Fare, S3StationCap] {
            set_station(&mut st, s, p, 10);
        }
    }
    set_line(&mut st, b, 0, T1Speed, 10);
    set_line(&mut st, b, 0, T4CarCapacity, 10);
    st.company.line_slots = Level(r2);
    st.progress.upgraded_once = all_upgraded();
    st.money = Money::pow10(20);
    st
}

#[test]
fn cheapest_when_nothing_raises_income() {
    let st = only_zero_value(1);
    let q = |p| quote(&st, bal(), p).map(|q| q.step);
    assert_eq!(q(S4Platform), Some(yen(10_000)));
    assert_eq!(q(R2LineSlots), Some(yen(10_000)));
    assert_eq!(ParamKey::ALL.iter().filter(|&&p| q(p).is_some()).count(), 2);
    assert_eq!(pick(&st), (S4Platform, RecommendReason::Cheapest, true));
    assert_eq!(
        pick(&only_zero_value(0)),
        (R2LineSlots, RecommendReason::Cheapest, true)
    );
}

#[test]
fn recommend_does_not_change_state() {
    let mut st = two_lines();
    st.money = yen(5000);
    let before = st.clone();
    let _ = rec_of(&st);
    assert_eq!(st, before);
    let quotes = ParamKey::ALL.map(|p| quote(&st, bal(), p));
    assert_eq!(recommend(&st, bal(), &quotes), rec_of(&st));
}

#[test]
fn new_never_picks_line_slots() {
    let mut st = world(
        bal(),
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0)],
        &[(&[0, 1, 2], 0)],
        1,
    );
    st.progress.upgraded_once = all_upgraded();
    st.progress.upgraded_once.0 &= !(1 << R2LineSlots.index());
    st.money = yen(1000);
    assert_eq!(pick(&st), (S2Fare, RecommendReason::BestValue, true));
}

/// 評価が同じなら 1 段のコストが小さい方、それも同じなら先の候補(01 D-25)。
#[test]
fn best_value_tie_prefers_smaller_step() {
    let b = bal();
    let mut st = three();
    for s in 0..3 {
        set_station(&mut st, s, S3StationCap, 4);
    }
    set_line(&mut st, b, 0, T4CarCapacity, 2);
    st.progress.upgraded_once = all_upgraded();
    let mut both = IdSet::default();
    both.insert(0);
    both.insert(1);
    let mut first = IdSet::default();
    first.insert(0);
    let q = |scope, step, raised| {
        Some(Quote {
            param: S2Fare,
            scope,
            step: yen(step),
            affordable: true,
            raised,
        })
    };
    let all = UpgradeScope::AllStations;
    let one = UpgradeScope::Station(sid(0));
    let base = estimate_income(&EconSnapshot::of(&st), b, SegTiles::Actual);
    assert_eq!(base, Money::from_u64(3).div_small(2));

    let mut quotes: [Option<Quote>; 12] = [None; 12];
    quotes[0] = q(all, 10, both);
    quotes[1] = q(one, 5, first);
    assert_eq!(recommend(&st, b, &quotes).unwrap().scope, one);
    quotes[0] = q(one, 5, first);
    quotes[1] = q(all, 10, both);
    assert_eq!(recommend(&st, b, &quotes).unwrap().scope, one);
    quotes[1] = q(all, 5, first);
    assert_eq!(recommend(&st, b, &quotes).unwrap().scope, one);
}

#[test]
fn estimator_h_is_at_least_one() {
    let st = world(bal(), &[(2, 5, 0), (8, 5, 0)], &[(&[0, 1], 0)], 0);
    let got = f(estimate_income(
        &EconSnapshot::of(&st),
        bal(),
        SegTiles::Fixed(6),
    ));
    assert!(rel_err(got, 5.0 / 6.0) < 1e-12, "{got}");
    assert!(rel_err(got, naive_a1(&st, 6.0)) < 1e-12);
}

/// 駅 2 は路線 0 の停車駅で、魅力度の高い路線 1 は通過する。A(2) は路線 0 だけで決まる。
#[test]
fn attractiveness_counts_only_stopping_lines() {
    let b = bal();
    let mut st = world(
        b,
        &[(2, 5, 1), (8, 5, 1), (14, 5, 0), (20, 9, 1), (8, 9, 1)],
        &[(&[0, 1, 2], 0), (&[3, 2, 4], 2)],
        4,
    );
    assert_eq!(st.lines[1].stops, [true, false, true]);
    set_station(&mut st, 2, S3StationCap, 4);
    set_line(&mut st, b, 0, T4CarCapacity, 8);
    set_line(&mut st, b, 0, T3Boarding, 6);
    let low = estimate_income(&EconSnapshot::of(&st), b, SegTiles::Actual);
    set_line(&mut st, b, 1, R3Attractiveness, 10);
    let high = estimate_income(&EconSnapshot::of(&st), b, SegTiles::Actual);
    assert_eq!(high, low);
    let got = f(estimate_income(
        &EconSnapshot::of(&st),
        b,
        SegTiles::Fixed(6),
    ));
    assert!(rel_err(got, naive_a1(&st, 6.0)) < 1e-12);
}
