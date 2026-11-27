use super::*;
use crate::balance::BalanceTable;
use crate::game::{Game, GameConfig};
use crate::ids::CompanyLevel;
use std::sync::Arc;

fn game() -> Game {
    let bal = Arc::new(BalanceTable::builtin());
    let rules = Arc::new(ReincarnationRules::builtin());
    Game::new_with_reincarnation(
        GameConfig {
            seed: 7,
            balance: bal,
        },
        rules,
    )
}

fn ready(g: &mut Game) {
    g.state.progress.total_earned = Money::from_u64(100);
}

fn step(g: &mut Game, cmd: Command) -> Vec<GameEvent> {
    g.enqueue(cmd);
    let mut events = Vec::new();
    g.step(&mut events);
    events
}

#[test]
fn rebirth_needs_the_threshold() {
    let mut g = game();
    let cmd = Command::Reincarnate;
    assert!(matches!(
        g.validate(&cmd),
        Err(CommandError::RebirthNotReady { .. })
    ));
    ready(&mut g);
    let p = g.validate(&cmd).expect("転生できる");
    assert_eq!(p.souls, 2, "base 1 + 10^2 の 1");
    let tick_after = g.next_tick();
    let seed = g.config().seed;
    let events = step(&mut g, cmd);
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::Reincarnated {
            souls_gained: 2,
            ..
        }
    )));
    assert_eq!(g.reincarnation().rebirths, 1);
    assert_eq!(g.reincarnation().souls, 2);
    assert_eq!(g.state.progress.total_earned, Money::ZERO);
    assert_eq!(g.now(), tick_after, "転生でゲームの時計は戻さない");
    assert_ne!(g.config().seed, seed, "新しいシードで駅を作り直す");
    assert_eq!(g.state.stations.len(), 3);
    assert!(g.state.lines.is_empty() && g.state.trains.is_empty());
}

#[test]
fn company_level_can_gate_rebirth() {
    let bal = Arc::new(BalanceTable::builtin());
    let mut spec = ReincarnationRules::builtin().spec().clone();
    spec.min_rebirth_company_level = 2;
    let rules = Arc::new(ReincarnationRules::from_spec(spec).unwrap());
    let mut g = Game::new_with_reincarnation(
        GameConfig {
            seed: 1,
            balance: bal,
        },
        rules,
    );
    ready(&mut g);
    assert!(matches!(
        g.validate(&Command::Reincarnate),
        Err(CommandError::RebirthCompanyLevelTooLow {
            required: CompanyLevel(2),
            current: CompanyLevel(0)
        })
    ));
    g.state.company.level = CompanyLevel(2);
    assert!(g.validate(&Command::Reincarnate).is_ok());
}

#[test]
fn disabled_config_refuses_both_commands() {
    let bal = Arc::new(BalanceTable::builtin());
    let mut spec = ReincarnationRules::builtin().spec().clone();
    spec.enabled = false;
    let rules = Arc::new(ReincarnationRules::from_spec(spec).unwrap());
    let mut g = Game::new_with_reincarnation(
        GameConfig {
            seed: 1,
            balance: bal,
        },
        rules,
    );
    ready(&mut g);
    assert_eq!(
        g.validate(&Command::Reincarnate),
        Err(CommandError::ReincarnationDisabled)
    );
    assert_eq!(
        g.validate(&Command::UnlockEvolution {
            node: EvolutionId(0)
        }),
        Err(CommandError::ReincarnationDisabled)
    );
}

#[test]
fn unlock_spends_souls_and_needs_prerequisites() {
    let mut g = game();
    let rules = g.reincarnation_rules().clone();
    let capital = rules.node_id("seed_capital").unwrap();
    g.reincarnation.souls = 100;
    g.reincarnation.lifetime_souls = 100;
    let events = step(&mut g, Command::UnlockEvolution { node: capital });
    let unlocked = events.iter().find_map(|e| match e {
        GameEvent::EvolutionUnlocked {
            node,
            level,
            cost,
            souls,
        } => Some((*node, *level, *cost, *souls)),
        _ => None,
    });
    assert_eq!(unlocked, Some((capital, 1, 1, 99)));
    assert_eq!(g.reincarnation().level(capital), 1);
    assert_eq!(g.reincarnation().souls, 99);
    assert_eq!(g.reincarnation().spent_souls, 1);
    assert_eq!(g.state.bonuses.start_money_yen, 1000);

    // 転生すると効果が効いた所持金で始まる。
    ready(&mut g);
    step(&mut g, Command::Reincarnate);
    assert_eq!(g.state.money, Money::from_u64(1000));

    let busy = rules.node_id("busy_platforms").unwrap();
    assert!(matches!(
        g.validate(&Command::UnlockEvolution { node: busy }),
        Err(CommandError::EvolutionPrerequisite { .. })
    ));
}

#[test]
fn max_level_and_tier_are_checked() {
    let mut g = game();
    let rules = g.reincarnation_rules().clone();
    let echo = rules.node_id("soul_echo").unwrap();
    guard_souls(&mut g, 1000);
    for _ in 0..3 {
        assert!(g.validate(&Command::UnlockEvolution { node: echo }).is_ok());
        step(&mut g, Command::UnlockEvolution { node: echo });
    }
    assert_eq!(
        g.validate(&Command::UnlockEvolution { node: echo }),
        Err(CommandError::EvolutionMaxLevel {
            node: echo,
            level: 3
        })
    );

    let deep = rules.node_id("deep_memory").unwrap();
    guard_souls(&mut g, 1000);
    assert!(matches!(
        g.validate(&Command::UnlockEvolution { node: deep }),
        Err(CommandError::EvolutionTierLocked { tier: 3, .. })
    ));
}

fn guard_souls(g: &mut Game, souls: u64) {
    g.reincarnation.souls = souls;
    g.reincarnation.lifetime_souls = souls;
}

#[test]
fn insufficient_souls_is_reported() {
    let g = game();
    let id = g.reincarnation_rules().node_id("soul_echo").unwrap();
    assert_eq!(
        g.validate(&Command::UnlockEvolution { node: id }),
        Err(CommandError::InsufficientSouls {
            required: 2,
            available: 0
        })
    );
}

#[test]
fn unknown_node_is_reported() {
    let g = game();
    assert_eq!(
        g.validate(&Command::UnlockEvolution {
            node: EvolutionId(99)
        }),
        Err(CommandError::UnknownEvolution(EvolutionId(99)))
    );
}

#[test]
fn next_seed_is_stable_and_not_zero() {
    assert_eq!(next_seed(1), next_seed(1));
    assert_ne!(next_seed(1), next_seed(2));
    assert_ne!(next_seed(0), 0);
}
