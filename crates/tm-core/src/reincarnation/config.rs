//! 転生システムの設定ファイル(TOML)と、検証済みの表。

use core::fmt;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tm_numeric::BigNum;

use crate::hash::StateHasher;
use crate::ids::{CompanyLevel, EvolutionId};
use crate::units::Money;

/// 転生の設定の形式の版。フィールドを変えたら上げる。
pub const REINCARNATION_FORMAT_VERSION: u32 = 1;

/// 埋め込みの `config/reincarnation.toml`。
pub const BUILTIN_REINCARNATION_TOML: &str = include_str!("../../../../config/reincarnation.toml");

/// 設定の読み込み・検証のエラー。
#[derive(Clone, Debug, PartialEq)]
pub enum ReincarnationError {
    /// TOML の構文・型の誤り。
    Parse {
        /// パーサのエラー。
        message: String,
    },
    /// 検証に通らなかった。
    Invalid {
        /// 検査の ID(`"RV-06"` など)。
        rule: &'static str,
        /// 何が違反したか。
        detail: String,
    },
}

impl fmt::Display for ReincarnationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReincarnationError::Parse { message } => {
                write!(f, "転生設定の読み込みに失敗: {message}")
            }
            ReincarnationError::Invalid { rule, detail } => write!(f, "{rule}: {detail}"),
        }
    }
}

impl std::error::Error for ReincarnationError {}

fn invalid(rule: &'static str, detail: String) -> ReincarnationError {
    ReincarnationError::Invalid { rule, detail }
}

/// `enabled` の既定値。省略したら有効。
fn default_true() -> bool {
    true
}

fn default_cost_ratio_permille() -> u32 {
    1000
}

fn default_max_level() -> u8 {
    1
}

/// TOML にそのまま対応する生の設定。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReincarnationSpec {
    /// `REINCARNATION_FORMAT_VERSION` と同じ値。
    pub format_version: u32,
    /// 転生を有効にするか。省略は有効。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 転生できる累計収入(10 の累乗)。
    #[serde(default)]
    pub min_rebirth_total_earned_pow10: u32,
    /// 転生できる会社レベル。
    #[serde(default)]
    pub min_rebirth_company_level: u8,
    /// 転生 1 回で得る魂。
    #[serde(default)]
    pub gain: GainRules,
    /// 段階の解放条件。
    #[serde(default)]
    pub tiers: Vec<TierRules>,
    /// 進化(ノード)。
    #[serde(default)]
    pub nodes: Vec<NodeRules>,
}

impl Default for ReincarnationSpec {
    fn default() -> Self {
        ReincarnationSpec {
            format_version: REINCARNATION_FORMAT_VERSION,
            enabled: true,
            min_rebirth_total_earned_pow10: 0,
            min_rebirth_company_level: 0,
            gain: GainRules {
                base: 1,
                ..GainRules::default()
            },
            tiers: Vec::new(),
            nodes: Vec::new(),
        }
    }
}

/// 転生で得る魂の式。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GainRules {
    /// 基礎の魂。
    #[serde(default)]
    pub base: u64,
    /// 会社レベル 1 つあたりの魂。
    #[serde(default)]
    pub per_company_level: u64,
    /// 駅 1 つあたりの魂。
    #[serde(default)]
    pub per_station: u64,
    /// 累計収入の段階ごとの魂(到達したものの合計)。
    #[serde(default)]
    pub tiers: Vec<GainTier>,
}

/// 累計収入の段階 1 つ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GainTier {
    /// 累計収入 10^この値 円に届いたら。
    pub min_total_earned_pow10: u32,
    /// 足す魂。
    pub souls: u64,
}

/// 進化の段階 1 つ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierRules {
    /// 段階の番号(ノードの `tier` が指す)。
    pub level: u8,
    /// 必要な転生回数。
    #[serde(default)]
    pub required_rebirths: u32,
    /// 必要な累計の魂(使った分も含む)。
    #[serde(default)]
    pub required_lifetime_souls: u64,
}

/// 進化のノード 1 つ。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeRules {
    /// 設定の中の識別子。重複なし。
    pub id: String,
    /// 表示用の名前。省略は `id`。
    #[serde(default)]
    pub name: String,
    /// 所属する段階。
    pub tier: u8,
    /// 1 レベル目の値段(魂)。
    pub cost: u64,
    /// 1 レベルごとの値段の倍率(1000 = 変わらない)。
    #[serde(default = "default_cost_ratio_permille")]
    pub cost_ratio_permille: u32,
    /// 最大レベル。
    #[serde(default = "default_max_level")]
    pub max_level: u8,
    /// 前提のノード。すべて 1 レベル以上必要。
    #[serde(default)]
    pub requires: Vec<String>,
    /// 1 レベルあたりの効果。
    #[serde(default)]
    pub effects: EvolutionEffects,
}

impl NodeRules {
    /// 表示用の名前(省略なら `id`)。
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.id
        } else {
            &self.name
        }
    }
}

/// 進化 1 レベルあたりの効果。すべて加算する。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionEffects {
    /// 運賃収入 +N‰。
    #[serde(default)]
    pub fare_income_permille: i32,
    /// お客さんの発生 +N‰。
    #[serde(default)]
    pub spawn_rate_permille: i32,
    /// 強化の値段 +N‰(負なら安くなる)。
    #[serde(default)]
    pub upgrade_cost_permille: i32,
    /// 線路の値段 +N‰(負なら安くなる)。
    #[serde(default)]
    pub track_cost_permille: i32,
    /// 転生で得る魂 +N‰。
    #[serde(default)]
    pub soul_gain_permille: i32,
    /// 転生後の所持金 +N 円。
    #[serde(default)]
    pub start_money_yen: u64,
}

impl EvolutionEffects {
    fn add_assign(&mut self, other: &EvolutionEffects) {
        self.fare_income_permille = self
            .fare_income_permille
            .saturating_add(other.fare_income_permille);
        self.spawn_rate_permille = self
            .spawn_rate_permille
            .saturating_add(other.spawn_rate_permille);
        self.upgrade_cost_permille = self
            .upgrade_cost_permille
            .saturating_add(other.upgrade_cost_permille);
        self.track_cost_permille = self
            .track_cost_permille
            .saturating_add(other.track_cost_permille);
        self.soul_gain_permille = self
            .soul_gain_permille
            .saturating_add(other.soul_gain_permille);
        self.start_money_yen = self.start_money_yen.saturating_add(other.start_money_yen);
    }
}

/// 検証済みの転生の設定。`from_spec` / `from_toml_str` でしか作れない。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReincarnationRules {
    spec: ReincarnationSpec,
    index: BTreeMap<String, EvolutionId>,
    tiers: BTreeMap<u8, TierRules>,
    hash: u64,
}

impl ReincarnationRules {
    /// 検証して前計算する。
    pub fn from_spec(spec: ReincarnationSpec) -> Result<Self, ReincarnationError> {
        validate(&spec)?;
        let index = spec
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), EvolutionId::from_index(i)))
            .collect();
        let tiers = spec.tiers.iter().map(|t| (t.level, *t)).collect();
        let mut rules = ReincarnationRules {
            spec,
            index,
            tiers,
            hash: 0,
        };
        rules.hash = rules.compute_hash();
        Ok(rules)
    }

    /// TOML の文字列から読んで検証する。
    pub fn from_toml_str(src: &str) -> Result<Self, ReincarnationError> {
        let spec: ReincarnationSpec =
            toml::from_str(src).map_err(|e| ReincarnationError::Parse {
                message: e.to_string(),
            })?;
        ReincarnationRules::from_spec(spec)
    }

    /// 埋め込みの設定。検証に通らなければ panic する(CI のテストで保証)。
    pub fn builtin() -> Self {
        ReincarnationRules::from_toml_str(BUILTIN_REINCARNATION_TOML)
            .unwrap_or_else(|e| panic!("埋め込みの reincarnation.toml が不正: {e}"))
    }

    /// 生の設定。
    pub fn spec(&self) -> &ReincarnationSpec {
        &self.spec
    }

    /// 転生が有効か。
    pub fn enabled(&self) -> bool {
        self.spec.enabled
    }

    /// ノードの数。
    pub fn node_count(&self) -> usize {
        self.spec.nodes.len()
    }

    /// ノードの定義。
    pub fn node(&self, id: EvolutionId) -> Option<&NodeRules> {
        self.spec.nodes.get(id.index())
    }

    /// 識別子からノードを引く。
    pub fn node_id(&self, id: &str) -> Option<EvolutionId> {
        self.index.get(id).copied()
    }

    /// 段階の定義。
    pub fn tier(&self, level: u8) -> Option<&TierRules> {
        self.tiers.get(&level)
    }

    /// 転生に必要な累計収入。
    pub fn required_total_earned(&self) -> Money {
        BigNum::pow10(self.spec.min_rebirth_total_earned_pow10)
    }

    /// 転生に必要な会社レベル。
    pub fn required_company_level(&self) -> CompanyLevel {
        CompanyLevel(self.spec.min_rebirth_company_level)
    }

    /// 累計収入・会社レベル・駅数から、進化の効果を入れる前の魂を求める。
    pub fn base_gain(&self, total_earned: Money, company: CompanyLevel, stations: usize) -> u64 {
        let g = &self.spec.gain;
        let mut souls = g.base;
        souls = souls.saturating_add(g.per_company_level.saturating_mul(u64::from(company.0)));
        souls = souls.saturating_add(
            g.per_station
                .saturating_mul(u64::try_from(stations).unwrap_or(u64::MAX)),
        );
        for tier in &g.tiers {
            if total_earned.total_cmp(&BigNum::pow10(tier.min_total_earned_pow10))
                != core::cmp::Ordering::Less
            {
                souls = souls.saturating_add(tier.souls);
            }
        }
        souls
    }

    /// `level` から `level + 1` にする値段(魂)。
    pub fn unlock_cost(&self, id: EvolutionId, level: u8) -> Option<u64> {
        let node = self.node(id)?;
        let mut cost = u128::from(node.cost);
        for _ in 0..level {
            cost = cost
                .saturating_mul(u128::from(node.cost_ratio_permille))
                .saturating_div(1000);
        }
        Some(u64::try_from(cost).unwrap_or(u64::MAX))
    }

    /// ノードの段階が解放済みか。
    pub fn tier_unlocked(&self, tier: u8, rebirths: u32, lifetime_souls: u64) -> bool {
        self.tier(tier).is_none_or(|t| {
            rebirths >= t.required_rebirths && lifetime_souls >= t.required_lifetime_souls
        })
    }

    /// ノードの効果をレベル分だけまとめる。
    pub fn node_bonuses(&self, id: EvolutionId, level: u8) -> EvolutionEffects {
        let Some(node) = self.node(id) else {
            return EvolutionEffects::default();
        };
        let mut out = EvolutionEffects::default();
        for _ in 0..level {
            out.add_assign(&node.effects);
        }
        out
    }

    /// 設定のハッシュ(形式・値の識別。セーブの照合に使う)。
    pub fn config_hash(&self) -> u64 {
        self.hash
    }

    fn compute_hash(&self) -> u64 {
        let text = ron::ser::to_string(&self.spec).expect("転生設定は RON に書ける");
        let mut h = StateHasher::new();
        h.write_bytes(b"TMRB");
        h.write_u32(REINCARNATION_FORMAT_VERSION);
        h.write_bytes(text.as_bytes());
        h.finish()
    }
}

/// 設定の検証。検査の順は形式 → 数値 → 段階 → ノード。
fn validate(spec: &ReincarnationSpec) -> Result<(), ReincarnationError> {
    if spec.format_version != REINCARNATION_FORMAT_VERSION {
        return Err(invalid(
            "RV-01",
            format!("format_version = {}", spec.format_version),
        ));
    }
    if spec.min_rebirth_total_earned_pow10 > 300 {
        return Err(invalid(
            "RV-02",
            format!(
                "min_rebirth_total_earned_pow10 = {}",
                spec.min_rebirth_total_earned_pow10
            ),
        ));
    }

    if spec.enabled
        && spec.gain.base == 0
        && spec.gain.per_company_level == 0
        && spec.gain.per_station == 0
        && spec.gain.tiers.iter().all(|t| t.souls == 0)
    {
        return Err(invalid(
            "RV-02",
            "gain がどこでも 0(転生しても魂が増えない)".to_string(),
        ));
    }
    let mut prev = None;
    for tier in &spec.gain.tiers {
        if tier.min_total_earned_pow10 > 300
            || prev.is_some_and(|p| tier.min_total_earned_pow10 <= p)
        {
            return Err(invalid(
                "RV-03",
                format!("gain.tiers が昇順でないか 300 を超える: {tier:?}"),
            ));
        }
        prev = Some(tier.min_total_earned_pow10);
    }

    if spec.tiers.len() > 255 {
        return Err(invalid("RV-04", format!("tiers = {}", spec.tiers.len())));
    }
    let mut levels = BTreeMap::new();
    for tier in &spec.tiers {
        if levels.insert(tier.level, ()).is_some() {
            return Err(invalid(
                "RV-04",
                format!("tiers の level {} が重複", tier.level),
            ));
        }
    }

    if spec.nodes.len() > 255 {
        return Err(invalid("RV-05", format!("nodes = {}", spec.nodes.len())));
    }
    let mut ids = BTreeMap::new();
    for (i, node) in spec.nodes.iter().enumerate() {
        if node.id.is_empty() {
            return Err(invalid("RV-06", format!("nodes[{i}] の id が空")));
        }
        if !node
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(invalid(
                "RV-06",
                format!("nodes[{i}] の id {:?} に使えない文字", node.id),
            ));
        }
        if ids.insert(node.id.clone(), i).is_some() {
            return Err(invalid("RV-06", format!("id {:?} が重複", node.id)));
        }
        if node.cost < 1 {
            return Err(invalid("RV-07", format!("{}: cost = 0", node.id)));
        }
        if node.cost_ratio_permille < 1 || node.max_level < 1 {
            return Err(invalid(
                "RV-07",
                format!(
                    "{}: cost_ratio_permille = {}、max_level = {}",
                    node.id, node.cost_ratio_permille, node.max_level
                ),
            ));
        }
        if !levels.contains_key(&node.tier) {
            return Err(invalid(
                "RV-08",
                format!("{}: tier {} の定義がない", node.id, node.tier),
            ));
        }
        check_effect(
            "RV-09",
            &node.id,
            "fare_income_permille",
            node.effects.fare_income_permille,
        )?;
        check_effect(
            "RV-09",
            &node.id,
            "spawn_rate_permille",
            node.effects.spawn_rate_permille,
        )?;
        check_effect(
            "RV-09",
            &node.id,
            "upgrade_cost_permille",
            node.effects.upgrade_cost_permille,
        )?;
        check_effect(
            "RV-09",
            &node.id,
            "track_cost_permille",
            node.effects.track_cost_permille,
        )?;
        check_effect(
            "RV-09",
            &node.id,
            "soul_gain_permille",
            node.effects.soul_gain_permille,
        )?;
    }

    for (i, node) in spec.nodes.iter().enumerate() {
        for req in &node.requires {
            let Some(&j) = ids.get(req) else {
                return Err(invalid(
                    "RV-10",
                    format!("{}: 前提 {:?} の定義がない", node.id, req),
                ));
            };
            if j >= i {
                return Err(invalid(
                    "RV-10",
                    format!(
                        "{}: 前提 {:?} が後ろにある(循環を避けるため前のノードだけ)",
                        node.id, req
                    ),
                ));
            }
            if spec.nodes[j].tier > node.tier {
                return Err(invalid(
                    "RV-10",
                    format!("{}: 前提 {:?} の段階が上", node.id, req),
                ));
            }
        }
    }
    Ok(())
}

fn check_effect(
    rule: &'static str,
    id: &str,
    name: &'static str,
    value: i32,
) -> Result<(), ReincarnationError> {
    if !(-1000..=1_000_000).contains(&value) {
        return Err(invalid(
            rule,
            format!("{id}: {name} = {value}(-1000..=1000000)"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_is_valid_and_stable() {
        let a = ReincarnationRules::builtin();
        let b = ReincarnationRules::builtin();
        assert_eq!(a, b);
        assert!(a.enabled());
        assert!(a.node_count() >= 1);
        assert_eq!(a.node_id("seed_capital"), Some(EvolutionId(0)));
        assert_eq!(
            a.node(EvolutionId(0)).unwrap().display_name(),
            "はじまりの資金"
        );
    }

    #[test]
    fn cost_grows_per_level() {
        let r = ReincarnationRules::builtin();
        let id = r.node_id("soul_echo").unwrap();
        assert_eq!(r.unlock_cost(id, 0), Some(2));
        assert_eq!(r.unlock_cost(id, 1), Some(3));
        assert_eq!(r.unlock_cost(id, 2), Some(4));
    }

    #[test]
    fn config_hash_ignores_comments_but_not_values() {
        let base = ReincarnationRules::builtin();
        let spaced = BUILTIN_REINCARNATION_TOML.replace("# ", "# x ");
        assert_eq!(
            ReincarnationRules::from_toml_str(&spaced)
                .unwrap()
                .config_hash(),
            base.config_hash()
        );
        let changed = BUILTIN_REINCARNATION_TOML.replace("base = 1", "base = 2");
        assert_ne!(
            ReincarnationRules::from_toml_str(&changed)
                .unwrap()
                .config_hash(),
            base.config_hash()
        );
    }

    #[test]
    fn default_spec_is_minimally_valid() {
        let r = ReincarnationRules::from_spec(ReincarnationSpec::default()).unwrap();
        assert!(r.enabled() && r.node_count() == 0);
    }

    #[test]
    fn disabled_config_needs_no_gain() {
        let r = ReincarnationRules::from_toml_str("format_version = 1\nenabled = false\n")
            .expect("無効なら魂の式は要らない");
        assert!(!r.enabled());
    }

    #[test]
    fn invalid_configs_are_refused() {
        let err = |s: &str| ReincarnationRules::from_toml_str(s).unwrap_err();
        assert!(matches!(
            err("format_version = 9"),
            ReincarnationError::Invalid { rule: "RV-01", .. }
        ));
        assert!(matches!(
            err("format_version = 1\nnodes = ["),
            ReincarnationError::Parse { .. }
        ));
        assert!(matches!(
            err("format_version = 1\nmin_rebirth_total_earned_pow10 = 301"),
            ReincarnationError::Invalid { rule: "RV-02", .. }
        ));
        assert!(matches!(
            err(r#"
                format_version = 1
                [gain]
                base = 1
                [[tiers]]
                level = 1
                [[nodes]]
                id = "a"
                tier = 2
                cost = 1
                "#),
            ReincarnationError::Invalid { rule: "RV-08", .. }
        ));
        assert!(matches!(
            err(r#"
                format_version = 1
                [gain]
                base = 1
                [[tiers]]
                level = 1
                [[nodes]]
                id = "a"
                tier = 1
                cost = 1
                requires = ["a"]
                "#),
            ReincarnationError::Invalid { rule: "RV-10", .. }
        ));
    }
}
