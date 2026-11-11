use super::*;
use crate::ids::Level;
use crate::rng::StreamRng;
use crate::routing::Routing;
use crate::sim::boarding::fixtures::*;
use crate::time::Tick;

fn bal() -> BalanceTable {
    BalanceTable::builtin()
}

fn words(st: &GameState) -> u64 {
    st.rng.passenger_dest.words()
}

fn row(st: &GameState, s: u32) -> Vec<u64> {
    st.waiting.row(s)[..st.stations.len()].to_vec()
}

/// 11 §6.2 の例: Lv0・魅力度 Lv0 は毎秒 500。1 秒目は 0 人(乱数を引かない)、2 秒目に 1 人、余り 0。
#[test]
fn level0_spawns_one_every_two_seconds() {
    let b = bal();
    let mut st = state(&[(2, 5, 0), (5, 5, 0)], &[(&[0, 1], 0)], &b);
    st.tick = Tick(60);
    spawn_all(&mut st, &b);
    assert_eq!(st.stations[0].spawn_acc, MilliPeople(500));
    assert_eq!(st.stations[0].waiting_total, People(0));
    assert_eq!(words(&st), 0, "0 人の秒は乱数を引かない");
    st.tick = Tick(120);
    spawn_all(&mut st, &b);
    assert_eq!(st.stations[0].spawn_acc, MilliPeople(0));
    assert_eq!((row(&st, 0), row(&st, 1)), (vec![0, 1], vec![1, 0]));
    assert_eq!(
        [st.stations[0].waiting_total, st.stations[1].waiting_total],
        [People(1); 2]
    );
    assert_eq!(words(&st), 2, "グループごとに below(1) を 1 回");
}

/// 目的駅は reachable_dests(s).nth(below(count))。乱数は StationId 昇順に 1 グループ 1 回。
#[test]
fn destination_is_nth_reachable_by_below() {
    let b = bal();
    let mut st = state(
        &[(2, 5, 0), (5, 5, 0), (9, 5, 0), (12, 5, 0), (20, 15, 0)],
        &[(&[0, 1, 2, 3], 0)],
        &b,
    );
    for s in &mut st.stations {
        s.levels.spawn = Level(1);
        s.levels.capacity = Level(10);
    }
    let mut rng: StreamRng = st.rng.passenger_dest.clone();
    for sec in 1..=20u64 {
        st.tick = Tick(60 * sec);
        let before = st.waiting.clone();
        spawn_all(&mut st, &b);
        for s in 0..4u32 {
            let reach: Vec<_> = st.routing.reachable_dests(StationId(s)).collect();
            assert_eq!(reach.len(), 3);
            let k = below(&mut rng, 3) as usize;
            let t = reach[k];
            assert_eq!(
                st.waiting.get(s, t).0,
                before.get(s, t).0 + 1,
                "{sec} 秒目 S{s}"
            );
        }
        assert_eq!(row(&st, 4), vec![0; 5], "路線に入っていない駅は発生しない");
    }
    assert_eq!(st.rng.passenger_dest, rng);
    assert_eq!(st.stations[4].spawn_acc, MilliPeople(0));
}

/// 定員で頭打ち: 取り出した 9 人のうち残り 5 人分だけ生まれ、4 人は消える。累積器の余り 600 は残る。
#[test]
fn capacity_caps_spawn_and_keeps_remainder() {
    let b = bal();
    let mut st = state(&[(2, 5, 0), (5, 5, 0)], &[(&[0, 1], 0)], &b);
    st.lines[0].levels.attractiveness = Level(1);
    st.routing = Routing::new(&st, &b);
    st.stations[0].levels.spawn = Level(4);
    put_waiting(&mut st, 0, 1, 5);
    spawn_all(&mut st, &b);
    assert_eq!(st.stations[0].spawn_acc, MilliPeople(600));
    assert_eq!(st.stations[0].waiting_total, People(10));
    #[cfg(feature = "test-support")]
    assert_eq!((st.ledger.spawned, st.ledger.capacity_loss), (5, 4));

    let w = words(&st);
    spawn_all(&mut st, &b);
    assert_eq!(st.stations[0].spawn_acc, MilliPeople(200));
    assert_eq!(st.stations[0].waiting_total, People(10), "満員なら 0 人");
    assert_eq!(
        words(&st),
        w + 1,
        "満員の駅は乱数を引かない(S1 の 1 回だけ)"
    );
}

/// 降りて戻った乗客で定員を超えている駅も、新しい発生は 0 人。
#[test]
fn over_capacity_station_spawns_nobody() {
    let b = bal();
    let mut st = state(&[(2, 5, 0), (5, 5, 0)], &[(&[0, 1], 0)], &b);
    st.stations[0].levels.spawn = Level(3);
    put_waiting(&mut st, 0, 1, 40);
    spawn_all(&mut st, &b);
    assert_eq!(st.stations[0].waiting_total, People(40));
    assert_eq!(st.stations[0].spawn_acc, MilliPeople(0));
}

/// 行ける駅がない駅(路線に入っていない駅、全部の路線が通過する駅)は累積器も増えない。
#[test]
fn no_reachable_destination_no_accumulation() {
    let b = bal();
    let mut st = state(
        &[(2, 5, 1), (5, 5, 0), (9, 5, 1), (20, 15, 1)],
        &[(&[0, 1, 2], 2)],
        &b,
    );
    assert_eq!(st.lines[0].stops, [true, false, true]);
    for _ in 0..3 {
        spawn_all(&mut st, &b);
    }
    assert!(st.stations[1].is_connected());
    assert_eq!(st.stations[1].spawn_acc, MilliPeople(0));
    assert_eq!(st.stations[3].spawn_acc, MilliPeople(0));
    assert_eq!(st.stations[0].spawn_acc, MilliPeople(500));
    assert_eq!([row(&st, 1), row(&st, 3)], [vec![0; 4], vec![0; 4]]);
}

/// 魅力度の倍率は停車する路線の最大。通過する路線の魅力度は使わない。
#[test]
fn attractiveness_is_max_over_stopping_lines() {
    let b = bal();
    let mut st = state(
        &[(2, 5, 1), (5, 5, 0), (9, 5, 1), (5, 2, 1), (5, 9, 1)],
        &[(&[0, 1, 2], 0), (&[3, 1, 4], 2), (&[3, 0], 0)],
        &b,
    );
    assert_eq!(st.lines[1].stops, [true, false, true]);
    st.lines[0].levels.attractiveness = Level(2);
    st.lines[1].levels.attractiveness = Level(9);
    st.lines[2].levels.attractiveness = Level(4);
    st.routing = Routing::new(&st, &b);
    assert_eq!(
        max_stopping_attractiveness(&st, &b, StationId(1)),
        Permille(1400)
    );
    assert_eq!(
        max_stopping_attractiveness(&st, &b, StationId(0)),
        Permille(1800)
    );
    assert_eq!(
        max_stopping_attractiveness(&st, &b, StationId(3)),
        Permille(2800)
    );
    spawn_all(&mut st, &b);
    assert_eq!(st.stations[1].spawn_acc, MilliPeople(700));
    assert_eq!(st.stations[0].spawn_acc, MilliPeople(900));
    spawn_all(&mut st, &b);
    assert_eq!(st.stations[0].spawn_acc, MilliPeople(800));
    assert_eq!(st.stations[0].waiting_total, People(1));
}
