use super::*;
use crate::scenario::Pause;
use crate::strategy::Player;

/// 流量モデルも、S3 の解放後は駅の S1 を S3 のレベルまでしか上げない(M9)。
#[test]
fn spawn_waits_for_capacity_m9() {
    let bal = BalanceTable::builtin();
    let mut f = FlowSim::new(&bal, Scenario::new(Player::B, Pause::NONE));
    f.lines.push(FlowLine {
        stations: vec![0, 1, 2],
        lv: [0; 12],
        born: CompanyLevel::ZERO,
    });
    let (s1, s3) = (ParamKey::S1Spawn.index(), ParamKey::S3StationCap.index());
    for s in &mut f.stations {
        s[s1] = 3;
        s[s3] = 3;
    }
    assert_eq!(
        f.raisable(ParamKey::S1Spawn).0,
        [0, 1, 2],
        "会社 Lv0 は S3 が未解放"
    );
    f.company = 1;
    assert!(f.raisable(ParamKey::S1Spawn).0.is_empty());
    f.stations[1][s3] = 4;
    assert_eq!(f.raisable(ParamKey::S1Spawn).0, [1]);
}

/// 流量モデルの「ぜんぶ」1 段の値段も、あとから出た駅の値段の下限(M8、M10)を使う。
#[test]
fn step_uses_the_upgrade_floor() {
    let bal = BalanceTable::builtin();
    let mut f = FlowSim::new(&bal, Scenario::new(Player::B, Pause::NONE));
    while f.stations.len() < 10 {
        f.stations.push([0; 12]);
    }
    f.lines.push(FlowLine {
        stations: (0..10).collect(),
        lv: [0; 12],
        born: CompanyLevel::ZERO,
    });
    f.company = 10;
    for s in &mut f.stations {
        s[ParamKey::S3StationCap.index()] = 20;
    }
    let p = ParamKey::S1Spawn;
    let r = f.raisable(p);
    assert_eq!(r.0.len(), 10);
    let want = (0..10).fold(0.0, |acc, s| {
        let born = bal.station_born_level(StationId::from_index(s));
        acc + money_f64(bal.upgrade_cost(p, tm_core::Level(0), born))
    });
    let plain = 10.0 * money_f64(bal.cost(p, tm_core::Level(0)));
    assert_eq!(f.step_of(p, &r), Some(want));
    assert!(want > plain * 1e6, "下限が効いている: {want} / {plain}");
    let s4 = ParamKey::S4Platform;
    let r4 = f.raisable(s4);
    let want4 = r4.0.iter().fold(0.0, |acc, &s| {
        let born = bal.station_born_level(StationId::from_index(s));
        acc + money_f64(bal.upgrade_cost(s4, tm_core::Level(0), born))
    });
    assert_eq!(f.step_of(s4, &r4), Some(want4));
    assert!(want4 > money_f64(bal.cost(s4, tm_core::Level(0))) * 10.0 * 1e6);
}

/// 流量モデルで会社 Lv4 以降に作った路線は、作ったときの会社レベルで値段の下限が付く(M10)。
#[test]
fn line_created_later_has_the_floor() {
    let bal = BalanceTable::builtin();
    let mut f = FlowSim::new(&bal, Scenario::new(Player::B, Pause::NONE));
    f.apply_build(Build::OpeningCreate);
    while f.stations.len() < 10 {
        f.stations.push([0; 12]);
    }
    f.company = 7;
    f.apply_build(Build::Create(9, 0));
    assert_eq!(f.lines[1].born, CompanyLevel(7));
    let p = ParamKey::T1Speed;
    let r = f.raisable(p);
    assert_eq!(r.1, [0, 1]);
    let want = money_f64(bal.cost(p, tm_core::Level(0)))
        + money_f64(bal.upgrade_cost(p, tm_core::Level(0), CompanyLevel(7)));
    assert_eq!(f.step_of(p, &r), Some(want));
    assert!(want > 1e6 * money_f64(bal.cost(p, tm_core::Level(0))));
}
