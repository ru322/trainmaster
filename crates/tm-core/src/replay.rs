use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::balance::BalanceTable;
use crate::command::Command;
use crate::event::GameEvent;
use crate::game::{Game, GameConfig};
use crate::snapshot::{GameSnapshot, SnapshotError};
use crate::time::{TICKS_PER_SEC, Tick};

/// リプレイログの形式の版(2 = `start` を足した。10 §9.1)。
pub const REPLAY_FORMAT_VERSION: u32 = 2;

/// `state_hash` を記録する間隔(10 秒ごと。10 TD-24)。
pub const CHECKPOINT_INTERVAL_TICKS: u64 = 600;

/// リプレイログ(10 §9.1)。ファイル I/O は呼び出し側。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplayLog {
    /// 版・シード・設定。
    pub header: ReplayHeader,
    /// 「続きから」の起点(10 §9.4)。`None` なら `Game::new` から。`Some` ならシードはヘッダと同じで、
    /// エントリの tick は写しの tick より大きい。
    pub start: Option<GameSnapshot>,
    /// tick の非減少順。同じ tick の中は `enqueue` した順。
    pub entries: Vec<ReplayEntry>,
    /// tick の昇順。
    pub checkpoints: Vec<Checkpoint>,
}

/// リプレイログのヘッダ。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplayHeader {
    /// `REPLAY_FORMAT_VERSION`。
    pub format: u32,
    /// `RULES_VERSION`。
    pub rules_version: u32,
    /// ゲームの版(`TM_BUILD_VERSION` か `CARGO_PKG_VERSION`)。
    pub game_version: String,
    /// ビルドに使った rustc の `rustc --version` の 1 行全体。
    pub rustc_version: String,
    /// `TICKS_PER_SEC`。
    pub ticks_per_sec: u32,
    /// 混ぜた後の最終シード [02 D24]。
    pub seed: u64,
    /// `BalanceTable::config_hash()`。
    pub config_hash: u64,
}

/// 1 つのコマンド。`tick` は `enqueue` したときの `next_tick()`。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplayEntry {
    /// 適用する tick。
    pub tick: Tick,
    /// コマンド。
    pub cmd: Command,
}

/// その tick の終わりの `state_hash`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// tick。
    pub tick: Tick,
    /// `state_hash()`。
    pub state_hash: u64,
}

/// リプレイログの読み込み・再生のエラー。
#[derive(Clone, Debug, PartialEq)]
pub enum ReplayError {
    /// RON の誤り。
    Parse(String),
    /// ヘッダの版が合わない。
    VersionMismatch {
        /// 合わなかったフィールド名。
        field: &'static str,
    },
    /// config hash が合わない。
    ConfigMismatch {
        /// ログの値。
        expected: u64,
        /// いまの表の値。
        actual: u64,
    },
    /// エントリの tick が減っている。
    UnorderedEntries {
        /// そのエントリの位置。
        index: u32,
    },
    /// チェックポイントの tick が減っている。
    UnorderedCheckpoints {
        /// そのチェックポイントの位置。
        index: u32,
    },
    /// チェックポイントの `state_hash` が合わない。
    Diverged {
        /// tick。
        tick: Tick,
        /// ログの値。
        expected: u64,
        /// 再生した値。
        actual: u64,
    },
    /// `start` の写しから作り直せない(シードがヘッダと違うときは `Malformed`)。
    Snapshot(SnapshotError),
}

impl ReplayLog {
    /// 空のログ。
    pub fn new(header: ReplayHeader) -> Self {
        ReplayLog {
            header,
            start: None,
            entries: Vec::new(),
            checkpoints: Vec::new(),
        }
    }

    /// コマンドを 1 つ記録する。
    pub fn record(&mut self, tick: Tick, cmd: &Command) {
        self.entries.push(ReplayEntry {
            tick,
            cmd: cmd.clone(),
        });
    }

    /// `game` の `now()` と `state_hash()` を足す。
    pub fn checkpoint(&mut self, game: &Game) {
        self.checkpoints.push(Checkpoint {
            tick: game.now(),
            state_hash: game.state_hash(),
        });
    }

    /// RON のテキスト。シナリオのゴールデンと同じ `depth_limit(2)`(エントリと写しの中身は 1 行ずつ)。
    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::new().depth_limit(2))
            .expect("ReplayLog は RON に書ける")
    }

    /// RON のテキストから読む。
    pub fn from_ron(src: &str) -> Result<Self, ReplayError> {
        ron::from_str(src).map_err(|e| ReplayError::Parse(e.to_string()))
    }
}

/// ログを `until` まで再生する(10 §9.1)。`Game::new`(`start` があれば `Game::restore`)から、各 tick のエントリを積んで `step`。
/// ヘッダ・順序・写し・チェックポイントが合わなければ `Err`。`until` が写しの tick より前なら写しの状態を返す。
pub fn replay(
    log: &ReplayLog,
    balance: Arc<BalanceTable>,
    until: Tick,
    events: &mut Vec<GameEvent>,
) -> Result<Game, ReplayError> {
    let h = &log.header;
    if h.format != REPLAY_FORMAT_VERSION {
        return Err(ReplayError::VersionMismatch { field: "format" });
    }
    if h.rules_version != crate::RULES_VERSION {
        return Err(ReplayError::VersionMismatch {
            field: "rules_version",
        });
    }
    if h.ticks_per_sec != TICKS_PER_SEC {
        return Err(ReplayError::VersionMismatch {
            field: "ticks_per_sec",
        });
    }
    let actual = balance.config_hash();
    if h.config_hash != actual {
        return Err(ReplayError::ConfigMismatch {
            expected: h.config_hash,
            actual,
        });
    }
    if let Some(s) = &log.start
        && s.seed != h.seed
    {
        return Err(ReplayError::Snapshot(SnapshotError::Malformed {
            detail: format!("start.seed {} != header.seed {}", s.seed, h.seed),
        }));
    }
    let mut prev = Tick(log.start.as_ref().map_or(0, |s| s.tick).saturating_add(1));
    for (i, e) in log.entries.iter().enumerate() {
        if e.tick < prev {
            return Err(ReplayError::UnorderedEntries {
                index: u32::try_from(i).expect("エントリの数は u32"),
            });
        }
        prev = e.tick;
    }
    if let Some(i) = log
        .checkpoints
        .windows(2)
        .position(|w| w[1].tick < w[0].tick)
    {
        return Err(ReplayError::UnorderedCheckpoints {
            index: u32::try_from(i + 1).expect("チェックポイントの数は u32"),
        });
    }

    let mut game = match &log.start {
        None => Game::new(GameConfig {
            seed: h.seed,
            balance,
        }),
        Some(s) => Game::restore(balance, s).map_err(ReplayError::Snapshot)?,
    };
    let mut entries = log.entries.iter().peekable();
    let mut checkpoints = log
        .checkpoints
        .iter()
        .filter(|c| c.tick <= until)
        .peekable();
    loop {
        while let Some(c) = checkpoints.next_if(|c| c.tick <= game.now()) {
            let actual = game.state_hash();
            if c.tick == game.now() && c.state_hash != actual {
                return Err(ReplayError::Diverged {
                    tick: c.tick,
                    expected: c.state_hash,
                    actual,
                });
            }
        }
        if game.now() >= until {
            return Ok(game);
        }
        let t = game.next_tick();
        while let Some(e) = entries.next_if(|e| e.tick == t) {
            game.enqueue(e.cmd.clone());
        }
        game.step(events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::LineEnd;
    use crate::geom::Bend;
    use crate::ids::{LineId, StationId};
    use crate::param::{BuyMode, ParamKey, UpgradeScope};

    fn sample() -> ReplayLog {
        let mut log = ReplayLog::new(ReplayHeader {
            format: REPLAY_FORMAT_VERSION,
            rules_version: crate::RULES_VERSION,
            game_version: "0.1.0".to_string(),
            rustc_version: "rustc 1.88.0 (6b00bc388 2025-06-23)".to_string(),
            ticks_per_sec: crate::time::TICKS_PER_SEC,
            seed: 0xDEAD_BEEF_0123_4567,
            config_hash: 0x0123_4567_89ab_cdef,
        });
        log.record(
            Tick(1),
            &Command::CreateLine {
                stations: vec![StationId(0), StationId(1)],
                bends: vec![Bend::DiagonalFirst],
            },
        );
        log.record(
            Tick(1),
            &Command::LevelUp {
                param: ParamKey::S2Fare,
                scope: UpgradeScope::AllStations,
                mode: BuyMode::Max,
            },
        );
        log.record(
            Tick(90),
            &Command::ExtendLine {
                line: LineId(0),
                end: LineEnd::Head,
                station: StationId(2),
                bend: Bend::StraightFirst,
            },
        );
        log.checkpoints.push(Checkpoint {
            tick: Tick(600),
            state_hash: u64::MAX,
        });
        log
    }

    #[test]
    fn ron_round_trip() {
        let log = sample();
        let text = log.to_ron();
        assert!(
            text.contains(
                "\n        (tick: 1, cmd: LevelUp(param: S2Fare, scope: AllStations, mode: Max)),\n"
            ),
            "{text}"
        );
        assert!(text.contains("seed: 16045690981116495207"), "{text}");
        assert_eq!(ReplayLog::from_ron(&text).unwrap(), log);
    }

    #[test]
    fn reads_hand_written_log() {
        let text = r#"(
            header: (format: 1, rules_version: 1, game_version: "v1.0.0", rustc_version: "rustc 1.88.0",
                     ticks_per_sec: 60, seed: 1, config_hash: 2),
            entries: [
                (tick: 5, cmd: LevelUp(param: S2Fare, scope: AllStations, mode: Max)),
                (tick: 5, cmd: LevelUp(param: S1Spawn, scope: Station(3), mode: One)),
            ],
            checkpoints: [(tick: 600, state_hash: 42)],
        )"#;
        let log = ReplayLog::from_ron(text).unwrap();
        assert_eq!(log.start, None);
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.entries[1].tick, Tick(5));
        assert_eq!(
            log.entries[1].cmd,
            Command::LevelUp {
                param: ParamKey::S1Spawn,
                scope: UpgradeScope::Station(StationId(3)),
                mode: BuyMode::One
            }
        );
        assert_eq!(
            log.checkpoints,
            vec![Checkpoint {
                tick: Tick(600),
                state_hash: 42
            }]
        );
    }

    /// 版 1 のログ(`start` がない)は読めるが、再生は版の違いで拒否する(10 §9.1)。
    #[test]
    fn version_1_log_is_read_but_not_replayed() {
        let mut log = sample();
        log.header.format = 1;
        log.header.config_hash = BalanceTable::builtin().config_hash();
        let text = log.to_ron().replace("    start: None,\n", "");
        assert!(!text.contains("start"), "{text}");
        let back = ReplayLog::from_ron(&text).unwrap();
        assert_eq!(back, log);
        assert_eq!(
            replay(
                &back,
                Arc::new(BalanceTable::builtin()),
                Tick(10),
                &mut Vec::new()
            )
            .map(|_| ()),
            Err(ReplayError::VersionMismatch { field: "format" })
        );
    }

    #[test]
    fn start_snapshot_round_trips() {
        let mut log = sample();
        let g = Game::new(GameConfig {
            seed: log.header.seed,
            balance: Arc::new(BalanceTable::builtin()),
        });
        log.start = Some(g.snapshot());
        let text = log.to_ron();
        assert!(text.contains("start: Some("), "{text}");
        assert!(text.lines().count() < 60, "{text}");
        assert_eq!(ReplayLog::from_ron(&text).unwrap(), log);
    }

    #[test]
    fn parse_error() {
        assert!(matches!(
            ReplayLog::from_ron("(header: 1)"),
            Err(ReplayError::Parse(_))
        ));
    }
}
