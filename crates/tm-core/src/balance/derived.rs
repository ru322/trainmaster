use tm_numeric::BigNum;

use super::{BalanceSpec, CompanyRules, ParamDef, TrackCostRules, ValueFormula};
use crate::ids::{CompanyLevel, Level};
use crate::param::{Feature, ParamKey};
use crate::units::Money;

/// 読み込み時に 1 回だけ計算する値とコストの表。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BalanceDerived {
    pub max_stations: u32,
    pub max_lines: u32,
    pub max_trains_per_line: u32,
    /// どのパラメータも超えない最大レベル(01 では 52)。
    pub max_level_any: Level,
    /// パラメータごとの到達できる最大レベル `limit(p, max_level)`。
    pub max_reach: [Level; 12],
    /// `L = 0..=max_reach[p]` の値。S2 は空(`fare` を使う)。
    pub values: [Vec<u64>; 12],
    pub fare: Vec<Money>,
    /// `L = 0..=max_reach[p]` の `L → L+1` のコスト。
    pub cost: [Vec<Money>; 12],
    pub goal: Money,
    /// `C = 0..=max_level` の `c1 × level_ratio^(C−1)`。`C < free_below_level` は 0。
    pub track_pow_c: Vec<Money>,
    /// `n = 0..=tile_cap` の `tile_ratio^n`。
    pub track_pow_n: Vec<Money>,
    /// `LineExtend`、`BulkBuy`、`Transfer` の順。
    pub feature_unlock: [CompanyLevel; 3],
}

impl BalanceDerived {
    pub fn feature_unlock(&self, f: Feature) -> CompanyLevel {
        match f {
            Feature::LineExtend => self.feature_unlock[0],
            Feature::BulkBuy => self.feature_unlock[1],
            Feature::Transfer => self.feature_unlock[2],
            Feature::Param(_) => unreachable!("Param の解放レベルは ParamDef.unlock"),
        }
    }

    /// 11 §4.8 の順: `pow_n` を i の昇順に足してから `pow_c` を掛ける。
    pub fn track_cost(&self, rules: &TrackCostRules, c: CompanyLevel, n: u32, k: u32) -> Money {
        if c.0 < rules.free_below_level || k == 0 {
            return Money::ZERO;
        }
        let mut sum = Money::ZERO;
        for i in 0..k {
            let idx = n.saturating_add(i).min(rules.tile_cap);
            sum = sum.saturating_add(self.track_pow_n[idx as usize]);
        }
        self.track_pow_c[usize::from(c.0)].saturating_mul(sum)
    }
}

pub(crate) fn cap_u32(company: &CompanyRules, c: u32) -> u32 {
    u32::from(company.cap_base) + u32::from(company.cap_step) * c
}

pub(crate) fn cap(company: &CompanyRules, c: CompanyLevel) -> u8 {
    u8::try_from(cap_u32(company, u32::from(c.0))).unwrap_or(u8::MAX)
}

pub(crate) fn limit(spec: &BalanceSpec, p: ParamKey, c: CompanyLevel) -> Level {
    let cap = cap(&spec.company, c);
    Level(
        spec.params[p.index()]
            .intrinsic_max
            .map_or(cap, |m| m.min(cap)),
    )
}

/// 値の式を `u64` で評価する(`Geometric` は `value_steps` の比を L の昇順に掛ける)。あふれたら `None`。
pub(crate) fn eval_u64(def: &ParamDef, l: u8) -> Option<u64> {
    match def.value {
        ValueFormula::Linear { v0, step } => step.checked_mul(u64::from(l))?.checked_add(v0),
        ValueFormula::Geometric { v0, .. } => {
            (0..l).try_fold(v0, |v, i| v.checked_mul(def.value_ratio_at(i)?))
        }
    }
}

/// 値の式を `Money` で評価する(S2。`value_steps` は使わない。BV-21)。
pub(crate) fn eval_money(def: &ParamDef, l: u8) -> Option<Money> {
    match def.value {
        ValueFormula::Linear { .. } => eval_u64(def, l).map(BigNum::from_u64),
        ValueFormula::Geometric { v0, ratio } => Some(
            BigNum::from_u64(v0)
                .saturating_mul(BigNum::pow_u32(BigNum::from_u64(ratio), u32::from(l))),
        ),
    }
}

/// `L → L+1` の段の比(10 TD-43)。
pub(crate) fn ratio_at(def: &ParamDef, l: u8) -> u32 {
    def.cost_steps
        .iter()
        .rev()
        .find(|s| s.from <= l)
        .map_or(def.cost_ratio, |s| s.ratio)
}

/// `L = 0..=top` のコスト。`base` に `ratio(L)` を L の昇順に 1 回ずつ掛ける(10 TD-43)。
pub(crate) fn cost_table(def: &ParamDef, top: u8) -> Vec<Money> {
    let mut c = BigNum::from_u64(def.base_yen);
    let mut table = Vec::with_capacity(usize::from(top) + 1);
    for l in 0..=top {
        if l > 0 {
            c = c.mul_small(u64::from(ratio_at(def, l - 1)));
        }
        table.push(c);
    }
    table
}

/// `track_pow_c`(`C = 0..=max_level`)。
pub(crate) fn track_pow_c(rules: &TrackCostRules, max_level: u8) -> Vec<Money> {
    let mut p = BigNum::from_u64(rules.c1_yen);
    let mut table = Vec::with_capacity(usize::from(max_level) + 1);
    for c in 0..=max_level {
        if c > 1 {
            p = p.mul_small(u64::from(rules.level_ratio));
        }
        table.push(if c < rules.free_below_level {
            Money::ZERO
        } else {
            p
        });
    }
    table
}

/// `track_pow_n`(`n = 0..=tile_cap`)。1 回ごとに掛けてから割る。
pub(crate) fn track_pow_n(rules: &TrackCostRules) -> Vec<Money> {
    let (num, den) = (u64::from(rules.tile_ratio.0), u64::from(rules.tile_ratio.1));
    let mut p = Money::ONE;
    let mut table = Vec::with_capacity(rules.tile_cap as usize + 1);
    table.push(p);
    for _ in 0..rules.tile_cap {
        p = p.mul_small(num).div_small(den);
        table.push(p);
    }
    table
}
