use super::*;
use crate::scenario::{Pause, Scenario, TPS};
use crate::strategy::Player;
use tm_core::BalanceTable;

fn hand_line(stops: &[StopFlow]) -> LineFlow<'_> {
    LineFlow {
        run_tiles: 12.0,
        speed: 1.0,
        trains: 1.0,
        cap: 10.0,
        boarding: 5.0,
        stops,
    }
}

#[test]
fn line_income_matches_hand_value_11_7_9() {
    let stop = StopFlow {
        spawn: 0.5,
        capacity: 10.0,
        fare: 1.0,
    };
    let stops = [stop; 3];
    let got = line_income(
        &hand_line(&stops),
        ride_stations(3),
        Dwell::from_ms(1000, 10_000),
    );
    assert!((got - 5.0 / 7.0).abs() <= 1e-12 * (5.0 / 7.0), "{got}");
    assert_eq!(
        line_income(&hand_line(&stops[..1]), 1.0, Dwell::from_ms(1000, 10_000)),
        0.0
    );
}

#[test]
fn ride_stations_is_clamped() {
    assert_eq!(ride_stations(0), 1.0);
    assert_eq!(ride_stations(6), 2.0);
    assert_eq!(ride_stations(19), 4.0);
}

const TABLE_4_6_STANDARD: [(Player, [f64; 10]); 3] = [
    (
        Player::A,
        [4.2, 5.2, 6.0, 6.8, 7.3, 8.6, 10.0, 5.9, 6.7, 10.5],
    ),
    (
        Player::B,
        [3.8, 4.8, 5.7, 6.6, 7.2, 8.7, 10.1, 5.7, 6.5, 10.7],
    ),
    (
        Player::C,
        [4.0, 5.3, 6.5, 7.7, 8.4, 10.9, 13.3, 5.6, 6.8, 13.6],
    ),
];

/// 表の値を測った版 1 の数値(会社 Lv 0〜16、`cap_step` 3、区間の比なし、線路はただ)。今の数値の値は IMP-706 で測る(01 §4.6)。
/// (c) と停止なし・長めのゴールは、ゲージに未接続の駅を数える規則(IMP-706b)で表から 0.2〜0.3 分早い。
fn v1_numbers() -> BalanceTable {
    let mut spec = BalanceTable::builtin().spec().clone();
    spec.company.max_level = 16;
    spec.company.cap_step = 3;
    spec.economy.track.free_below_level = 16;
    spec.economy.upgrade_floor.behind = u8::MAX;
    spec.transit.spawn_level_margin = u8::MAX;
    for p in &mut spec.params {
        p.cost_steps.clear();
        p.value_steps.clear();
    }
    BalanceTable::from_spec(spec).expect("検証を通る表")
}

fn flow_minutes(player: Player, pause: Pause) -> [f64; 10] {
    let bal = v1_numbers();
    let r = FlowSim::new(&bal, Scenario::new(player, pause)).run_for(40);
    let m = &r.milestones;
    [
        m.level_at(1),
        m.level_at(2),
        m.level_at(3),
        m.level_at(4),
        m.all_params,
        m.level_at(8),
        m.level_at(16),
        m.income_1e8,
        m.income_1e12,
        m.goal,
    ]
    .map(|t| t.expect("40 分で届く") as f64 / 3600.0)
}

#[test]
fn flow_sim_reproduces_01_4_6_standard_table() {
    for (player, want) in TABLE_4_6_STANDARD {
        let got = flow_minutes(player, Pause::STANDARD);
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() <= 0.1, "{player:?}: {got:?} / {want:?}");
        }
    }
}

#[test]
fn flow_sim_matches_01_4_6_pause_ranges() {
    let ranges = [
        (Pause::NONE, (4.5, 5.9), (8.1, 11.6)),
        (Pause::LONG, (7.5, 8.7), (11.4, 14.7)),
    ];
    for (pause, lv4, goal) in ranges {
        let got: Vec<[f64; 10]> = Player::ALL.map(|p| flow_minutes(p, pause)).to_vec();
        let span = |i: usize| {
            let v = got.iter().map(|r| r[i]);
            (v.clone().fold(f64::MAX, f64::min), v.fold(0.0, f64::max))
        };
        for ((lo, hi), (wlo, whi)) in [(span(3), lv4), (span(9), goal)] {
            assert!(
                (lo - wlo).abs() <= 0.1 && (hi - whi).abs() <= 0.1,
                "{pause:?}: {got:?}"
            );
        }
    }
}

#[test]
fn flow_sim_pays_for_track_when_built() {
    let bal = BalanceTable::builtin();
    let sc = Scenario::new(Player::B, Pause::NONE);
    let mut sim = FlowSim::new(&bal, sc.clone());
    while sim.company_level() == 0 {
        sim.step_second();
    }
    assert_eq!(SEGMENT_NEW_TILES, 6);
    assert_eq!(sim.track(), (13, 0.0));
    let r = FlowSim::new(&bal, sc).run_for(40);
    assert!(r.end_level >= 4, "{}", r.csv_row());
    let paid: Vec<&crate::report::BuildRecord> = r.builds.iter().filter(|b| b.level >= 1).collect();
    assert!(!paid.is_empty());
    assert!(
        paid.iter()
            .all(|b| b.cost > 0.0 && b.new_tiles == SEGMENT_NEW_TILES)
    );
    let sum = r.builds.iter().fold(0.0, |a, b| a + b.cost);
    assert_eq!(r.track_spent.to_bits(), sum.to_bits());
    let tiles: u32 = r.builds.iter().map(|b| b.new_tiles).sum();
    assert_eq!(r.track_tiles, tiles);
    let first = paid[0];
    let n_before = tiles_before(&r.builds, first);
    let want = money_f64(bal.track_cost(
        tm_core::CompanyLevel(first.level),
        n_before,
        SEGMENT_NEW_TILES,
    ));
    assert_eq!(first.cost.to_bits(), want.to_bits());
}

fn tiles_before(builds: &[crate::report::BuildRecord], b: &crate::report::BuildRecord) -> u32 {
    builds
        .iter()
        .take_while(|x| !core::ptr::eq(*x, b))
        .map(|x| x.new_tiles)
        .sum()
}

#[test]
fn flow_sim_saves_for_track_and_stays_idle() {
    let bal = BalanceTable::builtin();
    let mut sc = Scenario::new(Player::B, Pause::NONE);
    sc.save_s = 0;
    let none = FlowSim::new(&bal, sc.clone()).run_for(60);
    assert_eq!(none.saved_s, 0);
    sc.save_s = 1_000_000;
    let all = FlowSim::new(&bal, sc.clone()).run_for(60);
    assert!(all.saved_s > 0 && all.saved_s % 4 == 0, "{}", all.saved_s);
    sc.idle = vec![crate::scenario::Idle::after(300)];
    let idle = FlowSim::new(&bal, sc).run_for(60);
    assert!(idle.end_level < all.end_level, "{}", idle.csv_row());
    assert!(
        idle.milestones
            .level
            .iter()
            .flatten()
            .all(|&t| t <= 300 * TPS + 60 * TPS)
    );
}

#[test]
fn flow_sim_takes_track_price_from_money() {
    let bal = BalanceTable::builtin();
    let mut sim = FlowSim::new(&bal, Scenario::new(Player::B, Pause::NONE));
    let mut paid = 0;
    let iv = u64::from(Player::B.interval_s());
    for t in 1u64.. {
        if sim.company_level() >= 4 {
            break;
        }
        let ((m0, t0), (_, s0)) = (sim.money_total(), sim.track());
        sim.step_second();
        let ((m1, t1), (_, s1)) = (sim.money_total(), sim.track());
        let cost = s1 - s0;
        if cost > 0.0 && !t.is_multiple_of(iv) {
            let want = m0 - cost + (t1 - t0);
            assert!(
                (m1 - want).abs() <= 1e-9 * want.max(1.0),
                "{m0} {m1} {cost}"
            );
            paid += 1;
        }
    }
    assert!(paid > 0);
}
