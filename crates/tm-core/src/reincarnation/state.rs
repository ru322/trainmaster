//! 転生しても残る状態と、進化の効果をまとめた値。

use serde::{Deserialize, Serialize};

use super::{EvolutionEffects, ReincarnationRules};
use crate::hash::StateHasher;
use crate::ids::EvolutionId;
use crate::units::{MilliPeople, Money};

/// 転生しても残る状態。会社の状態(`GameState`)とは別に持つ。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReincarnationState {
    /// 使っていない魂。
    #[serde(default)]
    pub souls: u64,
    /// これまでもらった魂の合計(使った分を含む)。
    #[serde(default)]
    pub lifetime_souls: u64,
    /// これまで進化に使った魂の合計。
    #[serde(default)]
    pub spent_souls: u64,
    /// 転生した回数。
    #[serde(default)]
    pub rebirths: u32,
    /// ノードごとのレベル(添字 = `EvolutionId`)。
    #[serde(default)]
    pub nodes: Vec<u8>,
}

impl ReincarnationState {
    /// ノードの数に合わせた空の状態。
    pub fn new(node_count: usize) -> Self {
        ReincarnationState {
            nodes: vec![0; node_count],
            ..ReincarnationState::default()
        }
    }

    /// 進化のレベル。定義の外は 0。
    pub fn level(&self, id: EvolutionId) -> u8 {
        self.nodes.get(id.index()).copied().unwrap_or(0)
    }

    /// 進化を 1 レベル上げる(検査は呼び出し側。範囲外は何もしない)。
    pub fn raise(&mut self, id: EvolutionId) {
        if let Some(level) = self.nodes.get_mut(id.index()) {
            *level = level.saturating_add(1);
        }
    }

    /// 何も残っていないか(セーブの写しを省略する判定)。
    pub fn is_empty(&self) -> bool {
        self.souls == 0
            && self.lifetime_souls == 0
            && self.spent_souls == 0
            && self.rebirths == 0
            && self.nodes.iter().all(|&l| l == 0)
    }

    /// セーブの写しのハッシュ。
    pub fn hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_bytes(b"TMRH");
        h.write_u64(self.souls);
        h.write_u64(self.lifetime_souls);
        h.write_u64(self.spent_souls);
        h.write_u32(self.rebirths);
        h.write_len(self.nodes.len());
        for &level in &self.nodes {
            h.write_u8(level);
        }
        h.finish()
    }

    /// いまのレベルぶんの進化の効果をまとめる。
    pub fn bonuses(&self, rules: &ReincarnationRules) -> ReincarnationBonuses {
        let mut out = ReincarnationBonuses::default();
        for (i, &level) in self.nodes.iter().enumerate() {
            if level == 0 {
                continue;
            }
            let id = EvolutionId::from_index(i);
            let e = rules.node_bonuses(id, level);
            out.add(&e);
        }
        out
    }
}

/// 進化の効果をまとめた値。実行中の会社に効く。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReincarnationBonuses {
    /// 運賃収入の加算(‰)。
    pub fare_income_permille: i64,
    /// お客さんの発生の加算(‰)。
    pub spawn_rate_permille: i64,
    /// 強化の値段の加算(‰)。
    pub upgrade_cost_permille: i64,
    /// 線路の値段の加算(‰)。
    pub track_cost_permille: i64,
    /// 転生で得る魂の加算(‰)。
    pub soul_gain_permille: i64,
    /// 転生後の所持金の加算(円)。
    pub start_money_yen: u64,
}

impl ReincarnationBonuses {
    fn add(&mut self, e: &EvolutionEffects) {
        self.fare_income_permille = self
            .fare_income_permille
            .saturating_add(i64::from(e.fare_income_permille));
        self.spawn_rate_permille = self
            .spawn_rate_permille
            .saturating_add(i64::from(e.spawn_rate_permille));
        self.upgrade_cost_permille = self
            .upgrade_cost_permille
            .saturating_add(i64::from(e.upgrade_cost_permille));
        self.track_cost_permille = self
            .track_cost_permille
            .saturating_add(i64::from(e.track_cost_permille));
        self.soul_gain_permille = self
            .soul_gain_permille
            .saturating_add(i64::from(e.soul_gain_permille));
        self.start_money_yen = self.start_money_yen.saturating_add(e.start_money_yen);
    }

    /// 運賃収入に効果を掛ける。
    pub fn fare_income(&self, v: Money) -> Money {
        scale_money(v, self.fare_income_permille)
    }

    /// お客さんの発生に効果を掛ける。
    pub fn spawn_rate(&self, v: MilliPeople) -> MilliPeople {
        MilliPeople(scale_u64(v.0, self.spawn_rate_permille))
    }

    /// 強化の値段に効果を掛ける。
    pub fn upgrade_cost(&self, v: Money) -> Money {
        scale_money(v, self.upgrade_cost_permille)
    }

    /// 線路の値段に効果を掛ける。
    pub fn track_cost(&self, v: Money) -> Money {
        scale_money(v, self.track_cost_permille)
    }

    /// 転生で得る魂に効果を掛ける。
    pub fn soul_gain(&self, v: u64) -> u64 {
        scale_u64(v, self.soul_gain_permille)
    }

    /// 転生後の所持金の加算。
    pub fn start_money(&self) -> Money {
        Money::from_u64(self.start_money_yen)
    }
}

/// `v × (1000 + permille) / 1000`。負の効果でも 0 未満にはしない。
fn scale_money(v: Money, permille: i64) -> Money {
    let factor = 1000i64.saturating_add(permille);
    if factor <= 0 || v.is_zero() {
        Money::ZERO
    } else {
        v.mul_small(factor as u64).div_small(1000)
    }
}

fn scale_u64(v: u64, permille: i64) -> u64 {
    let factor = 1000i64.saturating_add(permille);
    if factor <= 0 {
        return 0;
    }
    let n = u128::from(v)
        .saturating_mul(u128::try_from(factor).unwrap_or(u128::MAX))
        .saturating_div(1000);
    u64::try_from(n).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_bonuses_accumulate() {
        let rules = ReincarnationRules::builtin();
        let mut st = ReincarnationState::new(rules.node_count());
        let capital = rules.node_id("seed_capital").unwrap();
        let echo = rules.node_id("soul_echo").unwrap();
        st.raise(capital);
        st.raise(capital);
        st.raise(echo);
        let b = st.bonuses(&rules);
        assert_eq!(b.start_money_yen, 2000);
        assert_eq!(b.soul_gain_permille, 100);
        assert_eq!(b.fare_income_permille, 0);
    }

    #[test]
    fn scaling_is_floor_and_never_negative() {
        assert_eq!(scale_u64(100, 100), 110);
        assert_eq!(scale_u64(101, -100), 90);
        assert_eq!(scale_u64(100, -1000), 0);
        assert_eq!(scale_u64(100, -2000), 0);
        assert_eq!(scale_money(Money::from_u64(100), 150), Money::from_u64(115));
    }

    #[test]
    fn empty_state_hash_is_stable() {
        let a = ReincarnationState::default();
        let b = ReincarnationState::new(3);
        assert!(a.is_empty() && b.is_empty());
        assert_ne!(a.hash(), b.hash(), "ノードの数もハッシュに入る");
    }
}
