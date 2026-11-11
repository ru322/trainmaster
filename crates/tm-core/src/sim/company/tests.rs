use super::*;
use crate::ids::Level;
use crate::sim::economy::fixtures::*;

use ParamKey::*;

fn three() -> GameState {
    world(
        bal(),
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0)],
        &[(&[0, 1, 2], 0)],
        0,
    )
}

/// S1・S2 のレベルを駅の順に、残りを T1・T4 に配って合計 `total` にする。
fn spread(st: &mut GameState, total: u8) {
    let mut left = total;
    let mut take = |want: u8| {
        let v = want.min(left);
        left -= v;
        v
    };
    for s in 0..st.stations.len() as u32 {
        if st.stations[s as usize].is_connected() {
            let (a, b) = (take(4), take(4));
            set_station(st, s, S1Spawn, a);
            set_station(st, s, S2Fare, b);
        }
    }
    let (a, b) = (take(4), take(4));
    st.lines[0].levels.speed = Level(a);
    st.lines[0].levels.car_capacity = Level(b);
    assert_eq!(left, 0);
}

#[test]
fn gauge_example_of_01() {
    let b = bal();
    let mut st = three();
    assert_eq!(gauge(&st, b), (0, 32));
    spread(&mut st, 22);
    let mut ev = Vec::new();
    settle_company_level(&mut st, b, &mut ev);
    assert!(ev.is_empty());
    assert_eq!(st.company.level, CompanyLevel(0));
    assert_eq!(gauge_view(&st, b).display_percent, 98);

    spread(&mut st, 23);
    settle_company_level(&mut st, b, &mut ev);
    assert_eq!(st.company.level, CompanyLevel(1));
    let pos = st.station_plan[3];
    assert_eq!(
        ev,
        [
            GameEvent::CompanyLevelUp {
                new_level: CompanyLevel(1),
                all_features_unlocked: false
            },
            GameEvent::FeatureUnlocked {
                feature: Feature::Param(S3StationCap),
                company_level: CompanyLevel(1)
            },
            GameEvent::FeatureUnlocked {
                feature: Feature::Param(R2LineSlots),
                company_level: CompanyLevel(1)
            },
            GameEvent::FeatureUnlocked {
                feature: Feature::Transfer,
                company_level: CompanyLevel(1)
            },
            GameEvent::StationSpawned {
                id: StationId(3),
                pos
            },
        ]
    );
    assert_eq!(st.stations.len(), 4);
    assert_eq!(st.waiting.rows(), 4);
    assert!(st.routing.is_dirty());
    assert_eq!(st.map_revision, 1);
}

#[test]
fn gauge_boundary_is_inclusive() {
    let b = bal();
    let mut st = world(
        b,
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0), (14, 9, 0)],
        &[(&[0, 1, 2, 3], 0)],
        0,
    );
    assert_eq!(gauge(&st, b), (0, 40));
    spread(&mut st, 27);
    settle_company_level(&mut st, b, &mut Vec::new());
    assert_eq!(st.company.level, CompanyLevel(0));
    spread(&mut st, 28);
    assert_eq!(gauge(&st, b), (28, 40));
    assert_eq!(gauge_view(&st, b).display_percent, 100);
    settle_company_level(&mut st, b, &mut Vec::new());
    assert_eq!(st.company.level, CompanyLevel(1));
}

#[test]
fn gauge_counts_every_spawned_station_and_only_unlocked() {
    let b = bal();
    let mut st = three();
    spawn_station(&mut st, &mut Vec::new());
    assert!(!st.stations[3].is_connected());
    set_station(&mut st, 0, S3StationCap, 3);
    assert_eq!(gauge(&st, b), (0, 40));
    st.progress.unlocked = unlocked_at(b, CompanyLevel(1));
    st.company.level = CompanyLevel(1);
    st.lines[0].levels.speed = Level(9);
    st.company.line_slots = Level(2);
    let (cap_c1, r2_intrinsic) = (6, 5);
    let (station_slots, line_slots) = (4 * 3, 2);
    assert_eq!(
        gauge(&st, b),
        (
            3 + cap_c1 + 2,
            cap_c1 * (station_slots + line_slots) + r2_intrinsic
        )
    );
}

#[test]
fn unconnected_stations_hold_the_gauge_down() {
    let b = bal();
    let mut st = world(b, &[(2, 5, 0), (8, 5, 0), (14, 5, 0)], &[], 0);
    assert_eq!(gauge(&st, b), (0, 24));
    assert_eq!(gauge_view(&st, b).display_percent, 0);
    let mut ev = Vec::new();
    settle_company_level(&mut st, b, &mut ev);
    assert!(ev.is_empty());

    let mut st = world(b, &[(2, 5, 0), (8, 5, 0), (14, 5, 0)], &[(&[0, 1], 0)], 0);
    spread(&mut st, 20);
    assert_eq!(gauge(&st, b), (20, 32));
    assert_eq!(gauge_view(&st, b).display_percent, 89);
    settle_company_level(&mut st, b, &mut ev);
    assert!(ev.is_empty());
}

/// 1 回の判定で Lv1 に上がったとき、その tick に出た駅を次のレベルの判定に数える(数えなければ Lv2 まで上がる)。
#[test]
fn station_spawned_in_the_same_settle_counts_for_the_next_level() {
    let b = bal();
    let mut st = three();
    st.tick = crate::time::Tick(100);
    for s in 0..3 {
        set_station(&mut st, s, S1Spawn, 6);
        set_station(&mut st, s, S2Fare, 6);
    }
    st.lines[0].levels.speed = Level(6);
    st.lines[0].levels.car_capacity = Level(6);
    st.company.line_slots = Level(5);
    let mut ev = Vec::new();
    settle_company_level(&mut st, b, &mut ev);
    assert_eq!(st.company.level, CompanyLevel(1));
    assert_eq!(st.stations.len(), 4);
    assert_eq!(st.stations[3].spawned_at, st.tick);
    let (cap_c1, r2) = (6, 5);
    assert_eq!(
        gauge(&st, b),
        (6 * 2 * 3 + 6 * 2 + r2, cap_c1 * (3 * 4 + 2) + r2)
    );
}

/// 新しい駅をつながずに、つながっている駅と路線を上限まで上げ続けても会社 Lv2 で止まる(IMP-706)。
#[test]
fn maxing_without_connecting_new_stations_stops_at_level_2() {
    let b = bal();
    let mut st = three();
    let mut ev = Vec::new();
    for _ in 0..4 {
        let (c, unlocked) = (st.company.level, st.progress.unlocked);
        for p in ParamKey::ALL
            .into_iter()
            .filter(|&p| unlocked.contains(Feature::Param(p)))
        {
            let m = b.limit(p, c);
            match p.owner() {
                ParamOwner::Station => (0..3).for_each(|s| set_station(&mut st, s, p, m.0)),
                ParamOwner::Line => *st.lines[0].levels.get_mut(p).expect("路線") = m,
                ParamOwner::Company => st.company.line_slots = m,
            }
        }
        settle_company_level(&mut st, b, &mut ev);
    }
    assert_eq!(st.company.level, CompanyLevel(2));
    assert_eq!(st.stations.len(), 5);
    let (sum, lim) = gauge(&st, b);
    assert!(10 * sum < 7 * lim && 10 * sum >= 6 * lim, "{sum}/{lim}");
}

#[test]
fn nothing_unlocked_means_no_gauge() {
    let b = bal();
    let mut st = three();
    st.progress.unlocked = FeatureSet::default();
    assert_eq!(gauge(&st, b), (0, 0));
    assert_eq!(gauge_view(&st, b).display_percent, 0);
    let mut ev = Vec::new();
    settle_company_level(&mut st, b, &mut ev);
    assert!(ev.is_empty());
}

/// ゲージのしきい値を 1/10 にした表で、1 回の判定で何レベルも上がる(未解放の路線のレベルは解放で数え始める)。
#[test]
fn unlock_order_and_all_features() {
    let b = custom(|s| s.company.gauge_num = 1);
    let mut st = world(
        &b,
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0)],
        &[(&[0, 1, 2], 0)],
        0,
    );
    spread(&mut st, 32);
    st.company.line_slots = Level(5);
    let lv = &mut st.lines[0].levels;
    (lv.crowding, lv.boarding, lv.cars) = (Level(6), Level(12), Level(9));
    (lv.max_trains, lv.attractiveness) = (Level(7), Level(10));
    let mut ev = Vec::new();
    settle_company_level(&mut st, &b, &mut ev);
    let ups: Vec<_> = ev
        .iter()
        .filter_map(|e| match *e {
            GameEvent::CompanyLevelUp {
                new_level,
                all_features_unlocked,
            } => Some((new_level.0, all_features_unlocked)),
            _ => None,
        })
        .collect();
    assert!(ups.len() >= 5, "{ups:?}");
    for (i, &(c, all)) in ups.iter().enumerate() {
        assert_eq!(usize::from(c), i + 1);
        assert_eq!(all, c == 4);
    }
    let unlocked: Vec<_> = ev
        .iter()
        .filter_map(|e| match *e {
            GameEvent::FeatureUnlocked {
                feature,
                company_level,
            } => Some((company_level.0, feature)),
            _ => None,
        })
        .collect();
    use Feature::Param as P;
    assert_eq!(
        unlocked,
        [
            (1, P(S3StationCap)),
            (1, P(R2LineSlots)),
            (1, Feature::Transfer),
            (2, P(S4Platform)),
            (2, P(T5Cars)),
            (3, P(R1MaxTrains)),
            (3, P(R3Attractiveness)),
            (4, P(T2Crowding)),
            (4, P(T3Boarding)),
        ]
    );
    let spawned: Vec<StationId> = ev
        .iter()
        .filter_map(|e| match *e {
            GameEvent::StationSpawned { id, pos } => {
                assert_eq!(pos, st.station_plan[id.index()]);
                Some(id)
            }
            _ => None,
        })
        .collect();
    assert_eq!(spawned.len(), ups.len());
    assert!(spawned.iter().enumerate().all(|(i, s)| s.index() == 3 + i));
    let order: Vec<u8> = ev
        .iter()
        .map(|e| match e {
            GameEvent::CompanyLevelUp { .. } => 0,
            GameEvent::FeatureUnlocked { .. } => 1,
            GameEvent::StationSpawned { .. } => 2,
            _ => 9,
        })
        .collect();
    assert!(order.windows(2).all(|w| w[0] <= w[1] || w[1] == 0));
    assert_eq!(st.progress.unlocked, unlocked_at(&b, st.company.level));
}

#[test]
fn stops_at_max_level() {
    let b = bal();
    let cells: Vec<(i32, i32, u8)> = (0..26)
        .map(|i| (2 + (i % 13) * 3, 3 + (i / 13) * 6, 0))
        .collect();
    let order: Vec<u32> = (0..26).collect();
    let mut st = world(b, &cells, &[(&order, 0)], 23);
    for s in 0..26 {
        for p in [S1Spawn, S2Fare, S3StationCap] {
            set_station(&mut st, s, p, 50);
        }
        set_station(&mut st, s, S4Platform, 8);
    }
    st.company.line_slots = Level(5);
    let lv = &mut st.lines[0].levels;
    lv.speed = Level(10);
    lv.crowding = Level(6);
    lv.boarding = Level(50);
    lv.car_capacity = Level(50);
    lv.cars = Level(9);
    lv.max_trains = Level(7);
    lv.attractiveness = Level(10);
    let mut ev = Vec::new();
    settle_company_level(&mut st, b, &mut ev);
    assert_eq!(st.company.level, CompanyLevel(24));
    assert_eq!(st.stations.len(), 27);
    assert_eq!(gauge_view(&st, b).display_percent, 100);
    assert_eq!(
        ev[0],
        GameEvent::CompanyLevelUp {
            new_level: CompanyLevel(24),
            all_features_unlocked: false
        }
    );
    settle_company_level(&mut st, b, &mut ev);
    assert_eq!(ev.len(), 2);
}

#[test]
fn stations_per_level_spawns_that_many() {
    let b = custom(|s| {
        s.company.stations_per_level = 2;
        s.company.max_level = 14;
    });
    let mut st = world(
        &b,
        &[(2, 5, 0), (8, 5, 0), (14, 5, 0)],
        &[(&[0, 1, 2], 0)],
        0,
    );
    spread(&mut st, 23);
    let mut ev = Vec::new();
    settle_company_level(&mut st, &b, &mut ev);
    assert_eq!(st.company.level, CompanyLevel(1));
    assert_eq!(st.stations.len(), 5);
}

#[test]
fn unlocked_at_levels() {
    let b = bal();
    let c0 = unlocked_at(b, CompanyLevel(0));
    for f in Feature::all_in_order() {
        assert_eq!(c0.contains(f), b.unlock_level(f).0 == 0, "{f:?}");
    }
    assert!(c0.contains(Feature::BulkBuy) && c0.contains(Feature::LineExtend));
    assert!(!c0.contains(Feature::Transfer));
    assert_eq!(
        unlocked_at(b, CompanyLevel(4)),
        unlocked_at(b, CompanyLevel(24))
    );
    assert_ne!(
        unlocked_at(b, CompanyLevel(3)),
        unlocked_at(b, CompanyLevel(4))
    );
}
