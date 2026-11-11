use super::fixtures::*;
use super::*;
use crate::balance::BalanceTable;
use crate::ids::{Level, LineId};
use crate::state::{Direction, TrainPhase};
use crate::units::PosUnits;

use Direction::{Backward, Forward};

fn bal() -> BalanceTable {
    BalanceTable::builtin()
}

fn serve(st: &mut GameState, bal: &BalanceTable, tid: TrainId) -> GameEvent {
    let mut ev = Vec::new();
    serve_arrival(st, bal, tid, &mut ev);
    assert_eq!(ev.len(), 1);
    ev.pop().unwrap()
}

fn arrived(ev: &GameEvent) -> (u64, u64, u64, Money, Ticks) {
    match *ev {
        GameEvent::TrainArrived {
            riders,
            alighted,
            boarded,
            income,
            dwell,
            ..
        } => (riders.0, alighted.0, boarded.0, income, dwell),
        _ => panic!("TrainArrived でない: {ev:?}"),
    }
}

fn dwell_remaining(st: &GameState, tid: TrainId) -> u32 {
    match st.trains[tid.index()].phase {
        TrainPhase::Dwelling { remaining, .. } => remaining.0,
        TrainPhase::Running => panic!("停車していない"),
    }
}

/// 運賃は着いたときに乗っている全員(ここで降りる人を含む)× 駅の運賃。ここで乗る人は払わない。
#[test]
fn fare_is_riders_on_arrival_times_station_fare() {
    let b = bal();
    let mut st = one_line(&b);
    st.stations[1].levels.fare = Level(2);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_onboard(&mut st, t, 1, 3);
    put_onboard(&mut st, t, 2, 4);
    put_waiting(&mut st, 1, 2, 5);
    let ev = serve(&mut st, &b, t);
    let fare = Money::from_u64(100);
    assert_eq!(arrived(&ev).3, fare.mul_small(7));
    assert_eq!(
        ev,
        GameEvent::TrainArrived {
            train: t,
            line: LineId(0),
            station: StationId(1),
            riders: People(7),
            alighted: People(3),
            boarded: People(5),
            income: fare.mul_small(7),
            dwell: ms_to_ticks(Millis(1000 + 1000 * 8 / 5)),
        }
    );
    assert_eq!(st.money, fare.mul_small(7));
    assert_eq!(st.progress.total_earned, st.money);
    assert_eq!(st.stats.income_this_sec, st.money);
    assert_eq!((st.stats.arrivals, st.stats.delivered), (1, 3));
    assert_eq!(st.trains[t.index()].riders, People(9));
}

/// 0 人でも TrainArrived を出す(収入 0、停車 1 秒 = 60 tick)。
#[test]
fn empty_arrival_still_reports() {
    let b = bal();
    let mut st = one_line(&b);
    let t = arriving_train(&mut st, 0, 0, Forward);
    let ev = serve(&mut st, &b, t);
    assert_eq!(arrived(&ev), (0, 0, 0, Money::ZERO, Ticks(60)));
    assert_eq!(dwell_remaining(&st, t), 59);
    assert_eq!(st.stats.arrivals, 1);
}

/// 乗り換え駅 S1 に L0 の往路の列車が着く: S1 行きは着く、S2 行きは乗ったまま、S3 行き(乗り換え)と S0 行き(向きが逆)は降りて待つ。
#[test]
fn rider_actions_arrive_stay_and_alight_to_wait() {
    let b = bal();
    let mut st = transfer_net(&b);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_onboard(&mut st, t, 0, 1);
    put_onboard(&mut st, t, 1, 2);
    put_onboard(&mut st, t, 2, 3);
    put_onboard(&mut st, t, 3, 4);
    assert_eq!(
        st.routing
            .rider_action(LineId(0), StationId(1), StationId(3), StationId(2)),
        RiderAction::AlightToWait
    );
    let (riders, alighted, boarded, _, _) = arrived(&serve(&mut st, &b, t));
    assert_eq!((riders, alighted, boarded), (10, 7, 0));
    assert_eq!(
        [0, 1, 2, 3].map(|d| onboard(&st, t, d)),
        [0, 0, 3, 0],
        "S2 行きだけが残る"
    );
    assert_eq!([waiting(&st, 1, 0), waiting(&st, 1, 3)], [1, 4]);
    assert_eq!(st.stations[1].waiting_total, People(5));
    assert_eq!(st.stats.delivered, 2);
    assert_eq!(st.trains[t.index()].riders, People(3));
    check_pax_invariants(&st, &b);
}

/// 端の S2 で折り返す列車: 来た方向(S1・S0)へ戻る乗客は降りずに乗ったまま、S0 方向の待ち客は乗る。
#[test]
fn riders_stay_through_turnaround_at_end() {
    let b = bal();
    let mut st = one_line(&b);
    let t = arriving_train(&mut st, 0, 1, Forward);
    put_onboard(&mut st, t, 0, 2);
    put_onboard(&mut st, t, 2, 1);
    put_waiting(&mut st, 2, 1, 3);
    let (riders, alighted, boarded, _, _) = arrived(&serve(&mut st, &b, t));
    assert_eq!((riders, alighted, boarded), (3, 1, 3));
    let tr = &st.trains[t.index()];
    assert_eq!((tr.section, tr.dir, tr.pos), (1, Backward, PosUnits(0)));
    assert_eq!(
        tr.phase,
        TrainPhase::Dwelling {
            at: 2,
            remaining: Ticks(ms_to_ticks(Millis(1800)).0 - 1)
        }
    );
    assert_eq!([onboard(&st, t, 0), onboard(&st, t, 1)], [2, 3]);
}

/// 向きの違う待ち客と、別の路線の待ち客は乗らない。
#[test]
fn only_matching_direction_and_line_board() {
    let b = bal();
    let mut st = transfer_net(&b);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_waiting(&mut st, 1, 0, 2);
    put_waiting(&mut st, 1, 3, 4);
    put_waiting(&mut st, 1, 2, 5);
    let (_, _, boarded, _, _) = arrived(&serve(&mut st, &b, t));
    assert_eq!(boarded, 5);
    assert_eq!(
        [waiting(&st, 1, 0), waiting(&st, 1, 2), waiting(&st, 1, 3)],
        [2, 0, 4]
    );
    assert_eq!(st.stations[1].waiting_total, People(6));
}

/// 11 §6.3 の例: 定員 10 で 3 人降り、待ち 100 人なら乗車 = min(10, 45 − 3) = 10、停車 3600ms = 216 tick。
#[test]
fn boarding_limited_by_capacity_example() {
    let b = bal();
    let mut st = one_line(&b);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_onboard(&mut st, t, 1, 3);
    put_waiting(&mut st, 1, 2, 100);
    let (_, alighted, boarded, _, dwell) = arrived(&serve(&mut st, &b, t));
    assert_eq!((alighted, boarded, dwell), (3, 10, Ticks(216)));
    assert_eq!(dwell_remaining(&st, t), 215);
    assert_eq!(waiting(&st, 1, 2), 90);
    assert_eq!(st.stations[1].waiting_total, People(90));
    assert_eq!(st.trains[t.index()].riders, People(10));
}

/// 乗ったままの乗客がいれば、乗車は定員の空き(10 − 6 = 4)まで。
#[test]
fn boarding_fills_only_free_seats() {
    let b = bal();
    let mut st = one_line(&b);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_onboard(&mut st, t, 2, 6);
    put_onboard(&mut st, t, 1, 2);
    put_waiting(&mut st, 1, 2, 100);
    let (riders, alighted, boarded, _, _) = arrived(&serve(&mut st, &b, t));
    assert_eq!((riders, alighted, boarded), (8, 2, 4));
    assert_eq!(st.trains[t.index()].riders, People(10));
    check_pax_invariants(&st, &b);
}

/// 定員に余裕があれば、乗車は停車の枠(速度 5 人/秒 × 9 = 45 人)− 降車まで。停車はちょうど上限の 10 秒。
#[test]
fn boarding_limited_by_dwell_budget() {
    let b = bal();
    let mut st = one_line(&b);
    st.lines[0].levels.car_capacity = Level(5);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_onboard(&mut st, t, 1, 3);
    put_waiting(&mut st, 1, 2, 100);
    let (_, alighted, boarded, _, dwell) = arrived(&serve(&mut st, &b, t));
    assert_eq!((alighted, boarded, dwell), (3, 42, Ticks(600)));
    assert_eq!(dwell_remaining(&st, t), 599);
}

/// 降りる人数が枠を超えても全員降り、乗車は 0、停車は 10 秒。
#[test]
fn alighting_over_budget_boards_nobody() {
    let b = bal();
    let mut st = one_line(&b);
    st.lines[0].levels.car_capacity = Level(3);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_onboard(&mut st, t, 1, 50);
    put_waiting(&mut st, 1, 2, 20);
    let (_, alighted, boarded, _, dwell) = arrived(&serve(&mut st, &b, t));
    assert_eq!((alighted, boarded, dwell), (50, 0, Ticks(600)));
    assert_eq!(st.trains[t.index()].riders, People(0));
    assert_eq!(waiting(&st, 1, 2), 20);
}

/// 停車時間の切り上げ: 1 人・速度 80 人/秒で ⌈1012.5⌉ = 1013ms → 61 tick。
#[test]
fn dwell_boundaries() {
    let b = bal();
    let mut st = one_line(&b);
    st.lines[0].levels.boarding = Level(4);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_waiting(&mut st, 1, 2, 1);
    assert_eq!(arrived(&serve(&mut st, &b, t)).4, Ticks(61));
    assert_eq!(dwell_remaining(&st, t), 60);
}

/// 枠が足りないときは目的駅の昇順に乗る(SIM-D-09)。
#[test]
fn boarding_in_destination_order() {
    let b = bal();
    let mut st = state(
        &[(2, 5, 0), (5, 5, 0), (9, 5, 0), (12, 5, 0)],
        &[(&[0, 1, 2, 3], 0)],
        &b,
    );
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_waiting(&mut st, 1, 3, 7);
    put_waiting(&mut st, 1, 2, 6);
    let (_, _, boarded, _, _) = arrived(&serve(&mut st, &b, t));
    assert_eq!(boarded, 10);
    assert_eq!([onboard(&st, t, 2), onboard(&st, t, 3)], [6, 4]);
    assert_eq!([waiting(&st, 1, 2), waiting(&st, 1, 3)], [0, 3]);
}

/// 同じ tick に同じ駅に着いた列車は TrainId の小さい方が先に乗せる(02 §3.4 (a))。
#[test]
fn smaller_train_id_boards_first() {
    let b = bal();
    let mut st = one_line(&b);
    let t0 = arriving_train(&mut st, 0, 0, Forward);
    let t1 = arriving_train(&mut st, 0, 0, Forward);
    put_waiting(&mut st, 1, 2, 15);
    let mut arrived_ids = Vec::new();
    let mut ev = Vec::new();
    tick(&mut st, &b, &mut arrived_ids, &mut ev);
    assert_eq!(arrived_ids, [t0, t1]);
    assert_eq!([onboard(&st, t0, 2), onboard(&st, t1, 2)], [10, 5]);
}

/// 乗り換えで降りた乗客は、同じ tick に後で処理する列車に乗れる(02 §3.4 (b))。同じ列車には乗り直さない(12 Q-2)。
#[test]
fn transferring_riders_catch_later_train_in_same_tick() {
    let b = bal();
    let mut st = transfer_net(&b);
    let t0 = arriving_train(&mut st, 0, 0, Forward);
    let t1 = arriving_train(&mut st, 1, 0, Backward);
    put_onboard(&mut st, t0, 3, 4);
    put_onboard(&mut st, t1, 1, 2);
    let mut ids = Vec::new();
    let mut ev = Vec::new();
    tick(&mut st, &b, &mut ids, &mut ev);
    assert_eq!(ids, [t0, t1]);
    assert_eq!(arrived(&ev[0]).1, 4);
    assert_eq!(arrived(&ev[0]).2, 0, "降りた列車には乗り直さない");
    assert_eq!((arrived(&ev[1]).1, arrived(&ev[1]).2), (2, 4));
    assert_eq!((onboard(&st, t1, 3), waiting(&st, 1, 3)), (4, 0));
    check_pax_invariants(&st, &b);
}

/// 降りて戻る乗客は駅の定員(S3 = 10)を超えてよいが、合計は定員 × 10 で頭打ち。
#[test]
fn returning_riders_may_exceed_capacity_up_to_limit() {
    let b = bal();
    let mut st = transfer_net(&b);
    st.lines[0].levels.car_capacity = Level(5);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_onboard(&mut st, t, 3, 120);
    put_waiting(&mut st, 1, 0, 5);
    let (_, alighted, _, _, _) = arrived(&serve(&mut st, &b, t));
    assert_eq!(alighted, 120, "降車は全員");
    assert_eq!(waiting(&st, 1, 3), 95);
    assert_eq!(st.stations[1].waiting_total, People(100));
    #[cfg(feature = "test-support")]
    assert_eq!(st.ledger.waiting_cap_loss, 25);
    check_pax_invariants(&st, &b);
}

/// 11 §11.1: 定員 10・倍率 10 の駅で合計 95 に 20 人戻すと 5 人だけ足され、15 人が消える。
#[test]
fn add_waiting_return_clamps_at_limit() {
    let b = bal();
    let mut st = one_line(&b);
    assert_eq!(waiting_limit(&b, &st.stations[1]), 100);
    put_waiting(&mut st, 1, 0, 90);
    put_waiting(&mut st, 1, 2, 5);
    add_waiting_return(&mut st, &b, StationId(1), StationId(2), 20);
    assert_eq!(waiting(&st, 1, 2), 10);
    assert_eq!(st.stations[1].waiting_total, People(100));
    #[cfg(feature = "test-support")]
    assert_eq!(st.ledger.waiting_cap_loss, 15);

    add_waiting_return(&mut st, &b, StationId(1), StationId(0), 7);
    assert_eq!(waiting(&st, 1, 0), 90, "上限ちょうどなら 0 人");
    #[cfg(feature = "test-support")]
    assert_eq!(st.ledger.waiting_cap_loss, 22);
    check_pax_invariants(&st, &b);

    let t = arriving_train(&mut st, 0, 0, Forward);
    let (_, _, boarded, _, _) = arrived(&serve(&mut st, &b, t));
    assert_eq!(boarded, 10);
    assert_eq!(st.stations[1].waiting_total, People(90));
    check_pax_invariants(&st, &b);
}

/// 上限の掛け算は u128 で行い、u64::MAX に飽和する(10 TD-37)。
#[test]
fn waiting_limit_saturates_without_overflow() {
    let mut spec = BalanceTable::builtin().spec().clone();
    spec.transit.waiting_cap_factor = u32::MAX;
    let b = BalanceTable::from_spec(spec).unwrap();
    let mut st = one_line(&b);
    st.stations[1].levels.capacity = Level(52);
    assert_eq!(waiting_limit(&b, &st.stations[1]), u64::MAX);
    put_waiting(&mut st, 1, 0, u64::MAX - 3);
    add_waiting_return(&mut st, &b, StationId(1), StationId(2), 10);
    assert_eq!(waiting(&st, 1, 2), 3);
    assert_eq!(st.stations[1].waiting_total, People(u64::MAX));
    #[cfg(feature = "test-support")]
    assert_eq!(st.ledger.waiting_cap_loss, 7);
}

/// 定員と停車の枠の計算は 01 の最大(Lv52 の定員と乗り降り速度)でもあふれない。
#[test]
fn large_levels_do_not_overflow() {
    let b = bal();
    let mut st = one_line(&b);
    let lv = &mut st.lines[0].levels;
    lv.car_capacity = Level(52);
    lv.cars = Level(0);
    lv.crowding = Level(6);
    lv.boarding = Level(52);
    st.stations[1].levels.capacity = Level(52);
    st.stations[1].levels.fare = Level(52);
    let t = arriving_train(&mut st, 0, 0, Forward);
    let big = 10u64 << 52;
    put_onboard(&mut st, t, 1, big);
    put_waiting(&mut st, 1, 2, big);
    let (riders, alighted, boarded, income, dwell) = arrived(&serve(&mut st, &b, t));
    assert_eq!((riders, alighted, boarded), (big, big, big));
    assert_eq!(income, b.fare(Level(52)).mul_small(big));
    assert_eq!(dwell, ms_to_ticks(Millis(5000)));
    assert_eq!(st.stats.delivered, u128::from(big));
}

/// 統計の delivered は u128 で数え、u64 を超えても正確(SIM-CR-03)。
#[test]
fn delivered_counts_beyond_u64() {
    let b = bal();
    let mut st = one_line(&b);
    st.stats.delivered = u128::from(u64::MAX);
    let t = arriving_train(&mut st, 0, 0, Forward);
    put_onboard(&mut st, t, 1, 2);
    serve(&mut st, &b, t);
    assert_eq!(st.stats.delivered, u128::from(u64::MAX) + 2);
}

/// 経路表の向き: 復路の列車が S1 に着くと S0 行きは乗ったまま、S2 行きは降りて待つ。
#[test]
fn backward_train_keeps_backward_riders() {
    let b = bal();
    let mut st = one_line(&b);
    let t = arriving_train(&mut st, 0, 1, Backward);
    put_onboard(&mut st, t, 0, 2);
    put_onboard(&mut st, t, 2, 1);
    let (_, alighted, boarded, _, _) = arrived(&serve(&mut st, &b, t));
    assert_eq!((alighted, boarded), (1, 0));
    assert_eq!(waiting(&st, 1, 2), 1);
    let tr = &st.trains[t.index()];
    assert_eq!((tr.section, tr.dir), (0, Backward));
    assert_eq!(
        tr.phase,
        TrainPhase::Dwelling {
            at: 1,
            remaining: Ticks(ms_to_ticks(Millis(1200)).0 - 1)
        }
    );
}
