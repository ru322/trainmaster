//! 段階ごとに許すコマンドの規則(13 §6.4)。

use tm_core::{Command, ParamKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PermitRule {
    Nothing,
    CreateLineOf(u32),
    /// #4 の待つ間の保険: 路線が 0 本の間だけ `n` 駅の `CreateLine` を許す。
    CreateLineOfWhileNoLine(u32),
    ExtendOnly,
    LevelUpOf(ParamKey),
    All,
}

impl PermitRule {
    /// `lines_created` は受けた `LineCreated` の数。
    pub(crate) fn allows(self, cmd: &Command, lines_created: u32) -> bool {
        let create_of = |n: u32| matches!(cmd, Command::CreateLine { stations, .. } if stations.len() == n as usize);
        match self {
            PermitRule::All => true,
            PermitRule::Nothing => false,
            PermitRule::CreateLineOf(n) => create_of(n),
            PermitRule::CreateLineOfWhileNoLine(n) => lines_created == 0 && create_of(n),
            PermitRule::ExtendOnly => matches!(cmd, Command::ExtendLine { .. }),
            PermitRule::LevelUpOf(p) => {
                matches!(cmd, Command::LevelUp { param, .. } if *param == p)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tm_core::{Bend, BuyMode, LineEnd, LineId, StationId, UpgradeScope};

    use super::*;

    fn create(n: u32) -> Command {
        Command::CreateLine {
            stations: (0..n).map(StationId).collect(),
            bends: vec![Bend::StraightFirst; n.saturating_sub(1) as usize],
        }
    }

    fn extend() -> Command {
        Command::ExtendLine {
            line: LineId(0),
            end: LineEnd::Tail,
            station: StationId(2),
            bend: Bend::StraightFirst,
        }
    }

    fn level_up(param: ParamKey) -> Command {
        Command::LevelUp {
            param,
            scope: UpgradeScope::AllStations,
            mode: BuyMode::One,
        }
    }

    #[test]
    fn each_rule_allows_only_its_commands() {
        let cmds = [
            create(2),
            create(3),
            extend(),
            level_up(ParamKey::S2Fare),
            level_up(ParamKey::S1Spawn),
        ];
        let table: [(PermitRule, u32, [bool; 5]); 7] = [
            (PermitRule::All, 0, [true; 5]),
            (PermitRule::Nothing, 0, [false; 5]),
            (
                PermitRule::CreateLineOf(2),
                1,
                [true, false, false, false, false],
            ),
            (
                PermitRule::CreateLineOfWhileNoLine(2),
                0,
                [true, false, false, false, false],
            ),
            (PermitRule::CreateLineOfWhileNoLine(2), 1, [false; 5]),
            (
                PermitRule::ExtendOnly,
                0,
                [false, false, true, false, false],
            ),
            (
                PermitRule::LevelUpOf(ParamKey::S2Fare),
                0,
                [false, false, false, true, false],
            ),
        ];
        for (rule, lines, want) in table {
            let got = cmds.each_ref().map(|c| rule.allows(c, lines));
            assert_eq!(got, want, "{rule:?} lines={lines}");
        }
    }
}
