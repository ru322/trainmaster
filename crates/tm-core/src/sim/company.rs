//! 会社ゲージ・会社レベルアップ・解放・駅の出現(11 §7.5)。

use crate::balance::BalanceTable;
use crate::event::GameEvent;
use crate::ids::{CompanyLevel, StationId};
use crate::param::{Feature, FeatureSet, ParamKey, ParamOwner, StationLevels};
use crate::state::{GameState, Station};
use crate::units::{MilliPeople, People};
use crate::view::GaugeView;

/// 会社ゲージの合計 `(Σmin(L, 上限), Σ上限)`(10 §3.3、TD-34)。駅は出現済みのすべて(未接続はレベル 0。INV-06)。状態を変えない。
pub(crate) fn gauge(st: &GameState, bal: &BalanceTable) -> (u64, u64) {
    let c = st.company.level;
    let (mut sum, mut lim) = (0u64, 0u64);
    for p in ParamKey::ALL {
        if !st.progress.unlocked.contains(Feature::Param(p)) {
            continue;
        }
        let m = u64::from(bal.limit(p, c).0);
        let mut add = |level: u8| {
            sum += u64::from(level).min(m);
            lim += m;
        };
        match p.owner() {
            ParamOwner::Station => st
                .stations
                .iter()
                .for_each(|s| add(s.levels.get(p).expect("駅のパラメータ").0)),
            ParamOwner::Line => st
                .lines
                .iter()
                .for_each(|l| add(l.levels.get(p).expect("路線のパラメータ").0)),
            ParamOwner::Company => add(st.company.line_slots.0),
        }
    }
    (sum, lim)
}

/// ゲージの表示(10 §3.3。70% を満タンとした 0..=100)。
pub(crate) fn gauge_view(st: &GameState, bal: &BalanceTable) -> GaugeView {
    let rules = &bal.spec().company;
    let (sum_level, sum_limit) = gauge(st, bal);
    let display_percent = if st.company.level.0 >= rules.max_level {
        100
    } else if sum_limit == 0 {
        0
    } else {
        let num = 100 * u128::from(rules.gauge_den) * u128::from(sum_level);
        let den = u128::from(rules.gauge_num) * u128::from(sum_limit);
        u8::try_from((num / den).min(100)).expect("100 以下")
    };
    GaugeView {
        sum_level,
        sum_limit,
        display_percent,
    }
}

/// 解放レベルが `c` 以下の Feature の集合(10 INV-14)。
pub(crate) fn unlocked_at(bal: &BalanceTable, c: CompanyLevel) -> FeatureSet {
    let mut set = FeatureSet::default();
    for f in Feature::all_in_order().filter(|&f| bal.unlock_level(f) <= c) {
        set.insert(f);
    }
    set
}

/// 成功したコマンドの後: 条件を満たす間 1 レベルずつ上げ、解放 → 駅の出現を行う(10 TD-10)。
pub(crate) fn settle_company_level(
    st: &mut GameState,
    bal: &BalanceTable,
    events: &mut Vec<GameEvent>,
) {
    let rules = &bal.spec().company;
    let all = unlocked_at(bal, CompanyLevel(rules.max_level));
    loop {
        let c = st.company.level;
        if c.0 >= rules.max_level {
            break;
        }
        let (sum, lim) = gauge(st, bal);
        if lim == 0 || u64::from(rules.gauge_den) * sum < u64::from(rules.gauge_num) * lim {
            break;
        }
        let nc = c.next();
        st.company.level = nc;
        let was_all = st.progress.unlocked == all;
        let mut newly = FeatureSet::default();
        for f in Feature::all_in_order().filter(|&f| bal.unlock_level(f) == nc) {
            newly.insert(f);
        }
        st.progress.unlocked = st.progress.unlocked.union(newly);
        events.push(GameEvent::CompanyLevelUp {
            new_level: nc,
            all_features_unlocked: st.progress.unlocked == all && !was_all,
        });
        for f in Feature::all_in_order().filter(|&f| newly.contains(f)) {
            events.push(GameEvent::FeatureUnlocked {
                feature: f,
                company_level: nc,
            });
        }
        // 10 INV-01: 駅の数 = stations_base + C × stations_per_level(01 では 1 レベルに 1 駅)。
        for _ in 0..rules.stations_per_level {
            spawn_station(st, events);
        }
    }
}

/// `station_plan` の次の位置に駅を 1 つ出す(11 §3.4)。
pub(crate) fn spawn_station(st: &mut GameState, events: &mut Vec<GameEvent>) {
    let id = StationId::from_index(st.stations.len());
    let pos = st.station_plan[id.index()];
    st.stations.push(Station {
        id,
        pos,
        spawned_at: st.tick,
        levels: StationLevels::default(),
        spawn_acc: MilliPeople::ZERO,
        waiting_total: People::ZERO,
        serving_lines: Vec::new(),
    });
    st.waiting.push_row();
    st.routing.mark_dirty();
    st.map_revision += 1;
    events.push(GameEvent::StationSpawned { id, pos });
}

#[cfg(test)]
mod tests;
