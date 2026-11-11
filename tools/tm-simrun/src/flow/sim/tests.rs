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
