//! 転生(プレステージ)システム。設定(TOML)・残る状態・コマンドの適用。
//!
//! 会社の状態(`GameState`)は転生で作り直し、[`ReincarnationState`] だけが残る。

mod config;
mod state;

pub use config::{
    BUILTIN_REINCARNATION_TOML, EvolutionEffects, GainRules, GainTier, NodeRules,
    REINCARNATION_FORMAT_VERSION, ReincarnationError, ReincarnationRules, ReincarnationSpec,
    TierRules,
};
pub use state::{ReincarnationBonuses, ReincarnationState};

use crate::balance::BalanceTable;
use crate::command::{Command, CommandError, CommandPreview};
use crate::event::GameEvent;
use crate::ids::EvolutionId;
use crate::mapgen::plan_stations;
use crate::rng::{STREAM_REBIRTH, StreamRng};
use crate::state::GameState;
use crate::units::Money;

/// 転生の計画。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RebirthPlan {
    /// 得る魂。
    pub souls: u64,
}

/// メタのコマンドか。
pub(crate) fn is_meta_command(cmd: &Command) -> bool {
    matches!(cmd, Command::Reincarnate | Command::UnlockEvolution { .. })
}

/// 状態を変えずに検査する。メタのコマンドだけを渡す。
pub(crate) fn validate_command(
    st: &GameState,
    meta: &ReincarnationState,
    rules: &ReincarnationRules,
    cmd: &Command,
) -> Result<CommandPreview, CommandError> {
    match cmd {
        Command::Reincarnate => {
            let plan = plan_reincarnate(st, meta, rules)?;
            Ok(preview(plan.souls))
        }
        Command::UnlockEvolution { node } => {
            let cost = plan_unlock(meta, rules, *node)?;
            Ok(preview(cost))
        }
        _ => unreachable!("メタのコマンドだけを検査する"),
    }
}

fn preview(souls: u64) -> CommandPreview {
    CommandPreview {
        cost: Money::ZERO,
        steps: 0,
        raised: 0,
        track_tiles: 0,
        new_track_tiles: 0,
        new_pass_through: 0,
        souls,
    }
}

/// 転生できるか。得る魂を返す。
pub(crate) fn plan_reincarnate(
    st: &GameState,
    meta: &ReincarnationState,
    rules: &ReincarnationRules,
) -> Result<RebirthPlan, CommandError> {
    if !rules.enabled() {
        return Err(CommandError::ReincarnationDisabled);
    }
    let required = rules.required_total_earned();
    if st.progress.total_earned.total_cmp(&required) == core::cmp::Ordering::Less {
        return Err(CommandError::RebirthNotReady {
            required,
            available: st.progress.total_earned,
        });
    }
    let required_company = rules.required_company_level();
    if st.company.level < required_company {
        return Err(CommandError::RebirthCompanyLevelTooLow {
            required: required_company,
            current: st.company.level,
        });
    }
    let base = rules.base_gain(
        st.progress.total_earned,
        st.company.level,
        st.stations.len(),
    );
    let souls = meta.bonuses(rules).soul_gain(base);
    Ok(RebirthPlan { souls })
}

/// 進化を 1 レベル上げられるか。値段(魂)を返す。
pub(crate) fn plan_unlock(
    meta: &ReincarnationState,
    rules: &ReincarnationRules,
    id: EvolutionId,
) -> Result<u64, CommandError> {
    if !rules.enabled() {
        return Err(CommandError::ReincarnationDisabled);
    }
    let node = rules.node(id).ok_or(CommandError::UnknownEvolution(id))?;
    let level = meta.level(id);
    if level >= node.max_level {
        return Err(CommandError::EvolutionMaxLevel { node: id, level });
    }
    if let Some(tier) = rules.tier(node.tier)
        && !rules.tier_unlocked(node.tier, meta.rebirths, meta.lifetime_souls)
    {
        return Err(CommandError::EvolutionTierLocked {
            tier: node.tier,
            required_rebirths: tier.required_rebirths,
            required_lifetime_souls: tier.required_lifetime_souls,
            rebirths: meta.rebirths,
            lifetime_souls: meta.lifetime_souls,
        });
    }
    for req in &node.requires {
        let Some(prerequisite) = rules.node_id(req) else {
            return Err(CommandError::UnknownEvolution(id));
        };
        if meta.level(prerequisite) == 0 {
            return Err(CommandError::EvolutionPrerequisite {
                node: id,
                prerequisite,
            });
        }
    }
    let cost = rules
        .unlock_cost(id, level)
        .ok_or(CommandError::UnknownEvolution(id))?;
    if meta.souls < cost {
        return Err(CommandError::InsufficientSouls {
            required: cost,
            available: meta.souls,
        });
    }
    Ok(cost)
}

/// メタのコマンドを適用する。失敗したら `CommandRejected` を出す。
pub(crate) fn apply_command(
    st: &mut GameState,
    bal: &BalanceTable,
    seed: &mut u64,
    meta: &mut ReincarnationState,
    rules: &ReincarnationRules,
    cmd: &Command,
    events: &mut Vec<GameEvent>,
) {
    match cmd {
        Command::Reincarnate => match plan_reincarnate(st, meta, rules) {
            Ok(plan) => apply_reincarnate(st, bal, seed, meta, rules, plan, events),
            Err(reason) => events.push(GameEvent::CommandRejected {
                kind: cmd.kind(),
                reason,
            }),
        },
        Command::UnlockEvolution { node } => match plan_unlock(meta, rules, *node) {
            Ok(cost) => apply_unlock(st, meta, rules, *node, cost, events),
            Err(reason) => events.push(GameEvent::CommandRejected {
                kind: cmd.kind(),
                reason,
            }),
        },
        _ => unreachable!("メタのコマンドだけを適用する"),
    }
}

fn apply_reincarnate(
    st: &mut GameState,
    bal: &BalanceTable,
    seed: &mut u64,
    meta: &mut ReincarnationState,
    rules: &ReincarnationRules,
    plan: RebirthPlan,
    events: &mut Vec<GameEvent>,
) {
    meta.souls = meta.souls.saturating_add(plan.souls);
    meta.lifetime_souls = meta.lifetime_souls.saturating_add(plan.souls);
    meta.rebirths = meta.rebirths.saturating_add(1);

    let old_revision = st.map_revision;
    let old_tick = st.tick;
    let new_seed = next_seed(*seed);
    *seed = new_seed;
    let bonuses = meta.bonuses(rules);
    let spec = bal.spec();
    let plan_stations = plan_stations(new_seed, &spec.map, &spec.company, bal.max_stations());
    *st = crate::game::initial_state(new_seed, bal, plan_stations, bonuses);
    // 転生してもゲームの時計は戻さない(リプレイと保存の tick を単調に保つ)。
    st.tick = old_tick;
    for station in &mut st.stations {
        station.spawned_at = old_tick;
    }
    // 表示のキャッシュ(マップの版)を、作り直したことを伝えるために進める。
    st.map_revision = old_revision.wrapping_add(1);

    events.push(GameEvent::Reincarnated {
        souls_gained: plan.souls,
        souls: meta.souls,
        rebirths: meta.rebirths,
        seed: new_seed,
    });
}

fn apply_unlock(
    st: &mut GameState,
    meta: &mut ReincarnationState,
    rules: &ReincarnationRules,
    id: EvolutionId,
    cost: u64,
    events: &mut Vec<GameEvent>,
) {
    meta.souls -= cost;
    meta.spent_souls = meta.spent_souls.saturating_add(cost);
    meta.raise(id);
    st.bonuses = meta.bonuses(rules);
    events.push(GameEvent::EvolutionUnlocked {
        node: id,
        level: meta.level(id),
        cost,
        souls: meta.souls,
    });
}

/// 転生の次のシード。同じシードなら同じ値(リプレイのため)。
fn next_seed(seed: u64) -> u64 {
    let mut rng = StreamRng::new(seed, STREAM_REBIRTH);
    let hi = u64::from(rng.next_u32());
    let lo = u64::from(rng.next_u32());
    let seed = (hi << 32) | lo;
    if seed == 0 { 1 } else { seed }
}

#[cfg(test)]
mod tests;
