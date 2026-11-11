use proptest::prelude::*;

use super::fixtures::*;
use crate::balance::BalanceTable;
use crate::event::GameEvent;
use crate::hash::StateHasher;
use crate::ids::{Level, LineId, StationId, TrainId};
use crate::routing::Routing;
use crate::sim::movement::place_initial_train;
use crate::state::GameState;
use crate::time::TICKS_PER_SEC;
use crate::units::People;

fn bal() -> BalanceTable {
    BalanceTable::builtin()
}

/// 11 §6.5 の保存の等式と、INV-08・INV-09・INV-11・INV-18・INV-19・待ち人数の上限。
fn check_all(st: &GameState, bal: &BalanceTable) {
    check_pax_invariants(st, bal);
    for stn in &st.stations {
        assert!(stn.spawn_acc.0 < 1000, "INV-11: {}", stn.id);
        if !stn.is_connected() {
            assert_eq!(
                (stn.spawn_acc.0, stn.waiting_total),
                (0, People::ZERO),
                "INV-19: {}",
                stn.id
            );
        }
        for (t, &n) in st
            .waiting
            .row(stn.id.0)
            .iter()
            .enumerate()
            .take(st.stations.len())
        {
            let t = StationId::from_index(t);
            assert!(
                n == 0 || st.routing.station_cost(stn.id, t).is_some(),
                "INV-18: {} → {t}",
                stn.id
            );
        }
    }
    #[cfg(feature = "test-support")]
    {
        let lg = st.ledger;
        assert_eq!(
            lg.spawned,
            in_system(st) + lg.delivered + lg.waiting_cap_loss,
            "保存の等式 {lg:?}"
        );
        assert_eq!(lg.delivered, st.stats.delivered);
    }
}

fn run(st: &mut GameState, bal: &BalanceTable, secs: u32, events: &mut Vec<GameEvent>) {
    let mut arrived: Vec<TrainId> = Vec::new();
    for _ in 0..secs {
        for _ in 0..TICKS_PER_SEC {
            tick(st, bal, &mut arrived, events);
        }
        check_all(st, bal);
    }
}

fn place_trains(st: &mut GameState, l: u32, n: u32) {
    for k in 0..n {
        place_initial_train(st, LineId(l), k, n);
    }
}

/// 乗り換え駅 S1(定員 10、上限 100)に、大きい L0 から乗り換え客が流れ込み、小さい L1 が運びきれない場面。
fn congested(bal: &BalanceTable) -> GameState {
    let mut st = transfer_net(bal);
    let l0 = &mut st.lines[0].levels;
    l0.car_capacity = Level(6);
    l0.crowding = Level(6);
    l0.boarding = Level(6);
    l0.speed = Level(10);
    st.stations[0].levels.spawn = Level(12);
    st.stations[0].levels.capacity = Level(10);
    st.stations[2].levels.spawn = Level(8);
    st.stations[1].levels.fare = Level(3);
    st.routing = Routing::new(&st, bal);
    place_trains(&mut st, 0, 3);
    place_trains(&mut st, 1, 1);
    st
}

/// 上限で消える乗り換え客(waiting_cap_loss)と、定員で生まれない人数(capacity_loss)が両方出ても、等式と上限が保たれる。
#[test]
fn congested_transfer_station_keeps_conservation() {
    let b = bal();
    let mut st = congested(&b);
    let mut ev = Vec::new();
    let mut peak = 0;
    for _ in 0..10 {
        run(&mut st, &b, 30, &mut ev);
        peak = peak.max(st.stations[1].waiting_total.0);
    }
    assert_eq!(peak, 100, "乗り換え駅の待ち人数は上限 100 まで届いて止まる");
    assert!(st.stats.delivered > 0);
    #[cfg(feature = "test-support")]
    {
        assert!(st.ledger.waiting_cap_loss > 0, "{:?}", st.ledger);
        assert!(st.ledger.capacity_loss > 0, "{:?}", st.ledger);
    }
}

fn digest(st: &GameState) -> u64 {
    let mut h = StateHasher::new();
    for &v in st.waiting.as_slice().iter().chain(st.onboard.as_slice()) {
        h.write_u64(v);
    }
    for s in &st.stations {
        h.write_u64(s.spawn_acc.0);
    }
    h.write_u64(st.stats.arrivals);
    h.write_u128(st.stats.delivered);
    h.write_money(st.money);
    h.write_u64(st.rng.passenger_dest.words());
    h.finish()
}

/// 決まった場面を 10 分回した結果のゴールデン値。同じ入力の 2 回の実行は状態とイベント列が一致する。
#[test]
fn ten_minutes_golden_and_deterministic() {
    let b = bal();
    let mut a = congested(&b);
    let mut c = a.clone();
    let (mut ea, mut ec) = (Vec::new(), Vec::new());
    run(&mut a, &b, 600, &mut ea);
    run(&mut c, &b, 600, &mut ec);
    assert_eq!(a, c);
    assert_eq!(ea, ec);
    assert_eq!(
        (
            a.stats.arrivals,
            a.stats.delivered,
            a.money.to_bits(),
            a.rng.passenger_dest.words()
        ),
        (517, 109_826, 4_730_229_344_079_708_160, 543)
    );
    assert_eq!(digest(&a), 2_597_847_088_031_730_776);
}

#[derive(Clone, Debug)]
struct Scenario {
    net: usize,
    station_levels: Vec<(u8, u8, u8)>,
    line_levels: Vec<(u8, u8, u8, u8, u8)>,
    trains: Vec<u32>,
    secs: u32,
}

fn build(sc: &Scenario, bal: &BalanceTable) -> GameState {
    let mut st = match sc.net {
        0 => one_line(bal),
        1 => transfer_net(bal),
        _ => state(
            &[
                (2, 5, 1),
                (5, 5, 0),
                (9, 5, 1),
                (5, 2, 1),
                (5, 9, 1),
                (12, 9, 1),
            ],
            &[(&[0, 1, 2], 0), (&[3, 1, 4], 2), (&[4, 5, 2], 0)],
            bal,
        ),
    };
    for (s, &(spawn, cap, fare)) in st.stations.iter_mut().zip(&sc.station_levels) {
        s.levels.spawn = Level(spawn);
        s.levels.capacity = Level(cap);
        s.levels.fare = Level(fare);
    }
    for (l, &(speed, crowd, board, car, attr)) in st.lines.iter_mut().zip(&sc.line_levels) {
        l.levels.speed = Level(speed);
        l.levels.crowding = Level(crowd);
        l.levels.boarding = Level(board);
        l.levels.car_capacity = Level(car);
        l.levels.attractiveness = Level(attr);
    }
    st.routing = Routing::new(&st, bal);
    for l in 0..st.lines.len() {
        place_trains(&mut st, u32::try_from(l).unwrap(), sc.trains[l]);
    }
    st
}

fn scenario() -> impl Strategy<Value = Scenario> {
    (
        0usize..3,
        proptest::collection::vec((0u8..=12, 0u8..=6, 0u8..=10), 6),
        proptest::collection::vec((0u8..=10, 0u8..=6, 0u8..=5, 0u8..=5, 0u8..=10), 3),
        proptest::collection::vec(1u32..=4, 3),
        60u32..=240,
    )
        .prop_map(
            |(net, station_levels, line_levels, trains, secs)| Scenario {
                net,
                station_levels,
                line_levels,
                trains,
                secs,
            },
        )
}

fn config() -> ProptestConfig {
    let mut c = ProptestConfig {
        cases: 48,
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

    /// 11 §11.2 の人数の保存: 毎秒、台帳の等式・キャッシュの一致・定員・上限・到達できる目的駅が成り立ち、乗客はいつか着く。
    #[test]
    fn passengers_are_conserved(sc in scenario()) {
        let b = bal();
        let mut st = build(&sc, &b);
        let mut ev = Vec::new();
        run(&mut st, &b, sc.secs, &mut ev);
        prop_assert!(st.stats.delivered > 0, "乗客が 1 人も着かない");
        let arrivals = ev.iter().filter(|e| matches!(e, GameEvent::TrainArrived { .. })).count();
        prop_assert_eq!(arrivals as u64, st.stats.arrivals);
    }
}
