//! 毎秒の乗客の発生(11 §6.2)。

use super::topology::station_index;
use crate::balance::BalanceTable;
use crate::ids::StationId;
use crate::rng::below;
use crate::state::GameState;
use crate::units::{MilliPeople, People, Permille, spawn_per_sec};

/// 手順5: 駅を `StationId` 昇順に、累積器から取り出した人数のグループを 1 つ作る [01 §7.1]。
pub(crate) fn spawn_all(st: &mut GameState, bal: &BalanceTable) {
    for i in 0..st.stations.len() {
        let s = StationId::from_index(i);
        if !st.stations[i].is_connected() {
            continue;
        }
        let count = st.routing.reachable_dests(s).count();
        if count == 0 {
            continue;
        }
        let add = st.bonuses.spawn_rate(spawn_per_sec(
            bal.spawn_rate(st.stations[i].levels.spawn),
            max_stopping_attractiveness(st, bal, s),
        ));
        let stn = &mut st.stations[i];
        let acc = stn.spawn_acc.add_sat(add).0;
        stn.spawn_acc = MilliPeople(acc % 1000);
        let k = acc / 1000;
        let room = bal
            .station_capacity(stn.levels.capacity)
            .0
            .saturating_sub(stn.waiting_total.0);
        let n = k.min(room);
        #[cfg(feature = "test-support")]
        {
            st.ledger.capacity_loss += u128::from(k - n);
            st.ledger.spawned += u128::from(n);
        }
        if n == 0 {
            continue;
        }
        let pick = below(
            &mut st.rng.passenger_dest,
            u32::try_from(count).expect("駅の数は u32"),
        );
        let t = st
            .routing
            .reachable_dests(s)
            .nth(pick as usize)
            .expect("count の中の番号");
        st.waiting.add(s.0, t, People(n));
        let stn = &mut st.stations[i];
        stn.waiting_total = stn.waiting_total.add_sat(People(n));
    }
}

/// 駅 `s` に停車する路線の魅力度の最大。なければ 1000 [01 §7.1]。
pub(crate) fn max_stopping_attractiveness(
    st: &GameState,
    bal: &BalanceTable,
    s: StationId,
) -> Permille {
    st.stations[s.index()]
        .serving_lines
        .iter()
        .map(|l| &st.lines[l.index()])
        .filter(|line| line.stops[station_index(line, s)])
        .map(|line| bal.attractiveness(line.levels.attractiveness))
        .max()
        .unwrap_or(Permille(1000))
}

#[cfg(test)]
mod tests;
