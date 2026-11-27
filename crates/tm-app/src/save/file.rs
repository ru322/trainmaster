//! セーブのファイル: 1 行目のヘッダと 2 行目以降の本体(RON)[10 §9.4、TD-47]。

use std::fmt;
use std::sync::Arc;

use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use tm_core::{
    BalanceTable, Game, GameSnapshot, RULES_VERSION, ReincarnationRules, SnapshotError, StateHasher,
};
use tm_tutorial::TutorialSave;

/// ヘッダ・本体の形か `TutorialSave`(段階の表を含む)を変えたら上げる。
pub const SAVE_FORMAT_VERSION: u32 = 1;
/// ヘッダの `magic`。
pub const SAVE_MAGIC: &str = "TMSAVE";
/// これより大きいファイルは読まない(`Game::restore` は列の長さに比例するため)。
pub const MAX_SAVE_BYTES: usize = 1 << 20;
/// これより大きい `seq` のセーブは読まない(壊れたヘッダで次の `seq` があふれないように)。
pub const MAX_SEQ: u64 = 1 << 48;

/// 1 行目。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveHeader {
    /// `SAVE_MAGIC`。
    pub magic: String,
    /// `SAVE_FORMAT_VERSION`。
    pub save_format: u32,
    /// 本体の `rules_version`。
    pub rules_version: u32,
    /// 本体の `config_hash`。
    pub config_hash: u64,
    /// 書いたビルドの版(`ReplayHeader::game_version` と同じ)。
    pub game_version: String,
    /// 保存するたびに 1 増える。A/B の新しい方を選ぶ。
    pub seq: u64,
    /// 本体の `tick`。
    pub tick: u64,
    /// 本体の `state_hash`。
    pub state_hash: u64,
    /// 2 行目以降のバイト列の FNV-1a 64。
    pub body_fnv: u64,
}

/// 2 行目以降。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveBody {
    /// ゲームの写し。
    pub game: GameSnapshot,
    /// チュートリアルの進み。
    pub tutorial: TutorialSave,
}

#[derive(Serialize)]
#[serde(rename = "SaveBody")]
struct BodyRef<'a> {
    game: &'a GameSnapshot,
    tutorial: &'a TutorialSave,
}

/// 版だけを先に読む(ほかの欄は読み飛ばすので、形の違う版でも版の違いとして分かる)。
#[derive(Deserialize)]
#[serde(rename = "SaveHeader")]
struct HeaderProbe {
    magic: String,
    save_format: u32,
}

/// セーブのファイル全体。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveFile {
    /// 1 行目。
    pub header: SaveHeader,
    /// 本体。
    pub body: SaveBody,
}

/// 検査を通って作り直したセーブ。
#[derive(Clone)]
pub struct Loaded {
    /// ヘッダの `seq`。
    pub seq: u64,
    /// `Game::restore` の結果。
    pub game: Game,
    /// チュートリアルの進み。
    pub tutorial: TutorialSave,
    /// ゲームのシード。
    pub seed: u64,
    /// 書いたビルドの版。
    pub game_version: String,
}

/// セーブを使わない理由。
#[derive(Clone, Debug, PartialEq)]
pub enum SaveError {
    /// `MAX_SAVE_BYTES` より大きい。
    TooLarge {
        /// バイト数。
        len: usize,
    },
    /// ヘッダの行が読めない。
    Header(String),
    /// `magic` が違う。
    Magic(String),
    /// `save_format` が違う。
    Format {
        /// ファイルの値。
        found: u32,
    },
    /// `rules_version` が違う。
    RulesVersion {
        /// ファイルの値。
        found: u32,
    },
    /// `config_hash` が違う。
    ConfigMismatch {
        /// ファイルの値。
        found: u64,
        /// いまの表の値。
        actual: u64,
    },
    /// 本体の FNV-1a がヘッダと違う。
    BodyFnv {
        /// ヘッダの値。
        expected: u64,
        /// 本体から計算した値。
        actual: u64,
    },
    /// 本体が RON として読めない。
    Body(String),
    /// ヘッダの `tick`・`state_hash` が本体と違う。
    HeaderMismatch,
    /// `seq` が `MAX_SEQ` より大きい。
    Seq {
        /// ファイルの値。
        found: u64,
    },
    /// `Game::restore` の失敗。
    Snapshot(SnapshotError),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::TooLarge { len } => write!(f, "大きすぎる({len} B)"),
            SaveError::Header(e) => write!(f, "ヘッダが読めない: {e}"),
            SaveError::Magic(m) => write!(f, "magic が違う: {m:?}"),
            SaveError::Format { found } => write!(f, "save_format が違う: {found}"),
            SaveError::RulesVersion { found } => write!(f, "rules_version が違う: {found}"),
            SaveError::ConfigMismatch { found, actual } => {
                write!(f, "config_hash が違う: {found:016x} / {actual:016x}")
            }
            SaveError::BodyFnv { expected, actual } => {
                write!(f, "body_fnv が違う: {expected:016x} / {actual:016x}")
            }
            SaveError::Body(e) => write!(f, "本体が読めない: {e}"),
            SaveError::HeaderMismatch => write!(f, "ヘッダと本体の tick・state_hash が違う"),
            SaveError::Seq { found } => write!(f, "seq が大きすぎる: {found}"),
            SaveError::Snapshot(e) => write!(f, "restore できない: {e:?}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// `StateHasher` と同じ FNV-1a 64。
pub fn body_fnv(body: &[u8]) -> u64 {
    let mut h = StateHasher::new();
    h.write_bytes(body);
    h.finish()
}

impl SaveFile {
    /// ファイルの中身を作る。ヘッダは 1 行、本体は 2 段まで改行する。
    pub fn encode(
        seq: u64,
        game: &GameSnapshot,
        tutorial: &TutorialSave,
        game_version: &str,
    ) -> String {
        let mut body = ron::ser::to_string_pretty(
            &BodyRef { game, tutorial },
            PrettyConfig::new().struct_names(true).depth_limit(2),
        )
        .expect("セーブの本体は RON に書ける");
        body.push('\n');
        let header = SaveHeader {
            magic: SAVE_MAGIC.to_owned(),
            save_format: SAVE_FORMAT_VERSION,
            rules_version: game.rules_version,
            config_hash: game.config_hash,
            game_version: game_version.to_owned(),
            seq,
            tick: game.tick,
            state_hash: game.state_hash,
            body_fnv: body_fnv(body.as_bytes()),
        };
        let mut out = ron::ser::to_string_pretty(
            &header,
            PrettyConfig::new().struct_names(true).compact_structs(true),
        )
        .expect("ヘッダは RON に書ける");
        out.push('\n');
        out.push_str(&body);
        out
    }

    /// 1 行目だけを読む(`seq` の比較用)。確かめるのは大きさ・`magic`・`save_format` だけ。
    pub fn header(bytes: &[u8]) -> Result<SaveHeader, SaveError> {
        Ok(split(bytes)?.0)
    }

    /// ヘッダの版と `config_hash` → `body_fnv` → 本体の RON → ヘッダと本体の一致、の順に検査して読む。
    pub fn decode(bytes: &[u8], balance: &BalanceTable) -> Result<SaveFile, SaveError> {
        let (header, body) = split(bytes)?;
        if header.rules_version != RULES_VERSION {
            return Err(SaveError::RulesVersion {
                found: header.rules_version,
            });
        }
        let actual = balance.config_hash();
        if header.config_hash != actual {
            return Err(SaveError::ConfigMismatch {
                found: header.config_hash,
                actual,
            });
        }
        let fnv = body_fnv(body);
        if fnv != header.body_fnv {
            return Err(SaveError::BodyFnv {
                expected: header.body_fnv,
                actual: fnv,
            });
        }
        let text = std::str::from_utf8(body).map_err(|e| SaveError::Body(e.to_string()))?;
        let body: SaveBody = ron::from_str(text).map_err(|e| SaveError::Body(e.to_string()))?;
        if body.game.tick != header.tick || body.game.state_hash != header.state_hash {
            return Err(SaveError::HeaderMismatch);
        }
        Ok(SaveFile { header, body })
    }

    /// `decode` に加えて `Game::restore`(導出データ、不変条件、`state_hash`)まで通す。
    /// 転生の設定は埋め込みのもの。
    pub fn load(bytes: &[u8], balance: &Arc<BalanceTable>) -> Result<Loaded, SaveError> {
        let f = Self::decode(bytes, balance)?;
        let game = Game::restore(Arc::clone(balance), &f.body.game).map_err(SaveError::Snapshot)?;
        Ok(Loaded {
            seq: f.header.seq,
            seed: game.config().seed,
            game,
            tutorial: f.body.tutorial,
            game_version: f.header.game_version,
        })
    }

    /// 転生の設定を指定して `decode` と `Game::restore` まで通す。
    pub fn load_with(
        bytes: &[u8],
        balance: &Arc<BalanceTable>,
        reincarnation: &Arc<ReincarnationRules>,
    ) -> Result<Loaded, SaveError> {
        let f = Self::decode(bytes, balance)?;
        let game = Game::restore_with_reincarnation(
            Arc::clone(balance),
            Arc::clone(reincarnation),
            &f.body.game,
        )
        .map_err(SaveError::Snapshot)?;
        Ok(Loaded {
            seq: f.header.seq,
            seed: game.config().seed,
            game,
            tutorial: f.body.tutorial,
            game_version: f.header.game_version,
        })
    }
}

fn split(bytes: &[u8]) -> Result<(SaveHeader, &[u8]), SaveError> {
    if bytes.len() > MAX_SAVE_BYTES {
        return Err(SaveError::TooLarge { len: bytes.len() });
    }
    let nl = bytes
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| SaveError::Header("改行がない".into()))?;
    let line = std::str::from_utf8(&bytes[..nl]).map_err(|e| SaveError::Header(e.to_string()))?;
    let probe: HeaderProbe = ron::from_str(line).map_err(|e| SaveError::Header(e.to_string()))?;
    if probe.magic != SAVE_MAGIC {
        return Err(SaveError::Magic(probe.magic));
    }
    if probe.save_format != SAVE_FORMAT_VERSION {
        return Err(SaveError::Format {
            found: probe.save_format,
        });
    }
    let header: SaveHeader = ron::from_str(line).map_err(|e| SaveError::Header(e.to_string()))?;
    if header.seq > MAX_SEQ {
        return Err(SaveError::Seq { found: header.seq });
    }
    Ok((header, &bytes[nl + 1..]))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tm_core::{GameConfig, GameEvent, ReincarnationRules, ReincarnationState};
    use tm_tutorial::Tutorial;

    pub(crate) fn game(seed: u64, ticks: u64) -> Game {
        let mut g = Game::new(GameConfig {
            seed,
            balance: Arc::new(BalanceTable::builtin()),
        });
        let mut ev: Vec<GameEvent> = Vec::new();
        for _ in 0..ticks {
            g.step(&mut ev);
        }
        g
    }

    pub(crate) fn encoded(seq: u64, g: &Game) -> Vec<u8> {
        SaveFile::encode(seq, &g.snapshot(), &Tutorial::new(true).save(), "test").into_bytes()
    }

    fn balance() -> Arc<BalanceTable> {
        Arc::new(BalanceTable::builtin())
    }

    fn lines(b: &[u8]) -> (String, String) {
        let s = String::from_utf8(b.to_vec()).unwrap();
        let (h, body) = s.split_once('\n').unwrap();
        (h.to_owned(), body.to_owned())
    }

    /// 本体を変えて、`body_fnv` を合わせる。
    fn with_body(b: &[u8], f: impl FnOnce(String) -> String) -> Vec<u8> {
        let (h, body) = lines(b);
        let body = f(body);
        let mut header: SaveHeader = ron::from_str(&h).unwrap();
        header.body_fnv = body_fnv(body.as_bytes());
        let h = ron::ser::to_string_pretty(
            &header,
            PrettyConfig::new().struct_names(true).compact_structs(true),
        )
        .unwrap();
        format!("{h}\n{body}").into_bytes()
    }

    fn with_header(b: &[u8], from: &str, to: &str) -> Vec<u8> {
        let (h, body) = lines(b);
        assert!(h.contains(from), "{h}");
        format!("{}\n{body}", h.replacen(from, to, 1)).into_bytes()
    }

    fn load(b: &[u8]) -> Result<Loaded, SaveError> {
        SaveFile::load(b, &balance())
    }

    #[test]
    fn header_is_one_line_and_body_round_trips() {
        let g = game(7, 300);
        let b = encoded(12, &g);
        let (h, body) = lines(&b);
        assert!(
            h.starts_with(&format!(
                "SaveHeader(magic: \"TMSAVE\", save_format: 1, rules_version: {RULES_VERSION}, config_hash: "
            )),
            "{h}"
        );
        assert!(
            h.contains("game_version: \"test\", seq: 12, tick: 300, "),
            "{h}"
        );
        assert!(
            body.starts_with("SaveBody(\n    game: GameSnapshot(\n"),
            "{body}"
        );
        assert!(body.contains("tutorial: TutorialSave("), "{body}");
        assert!(body.ends_with(")\n"));
        let f = SaveFile::decode(&b, &balance()).unwrap();
        assert_eq!(f.body.game, g.snapshot());
        assert_eq!(f.body.tutorial, Tutorial::new(true).save());
        assert_eq!(f.header.body_fnv, body_fnv(body.as_bytes()));
        let l = load(&b).unwrap();
        assert_eq!((l.seq, l.seed), (12, 7));
        assert_eq!(l.game.state_hash(), g.state_hash());
        assert_eq!(l.game.now(), g.now());
        assert_eq!(l.game_version, "test");
        assert_eq!(SaveFile::header(&b).unwrap(), f.header);
    }

    #[test]
    fn checks_in_order() {
        let g = game(3, 200);
        let b = encoded(1, &g);
        let err = |b: &[u8]| load(b).err().expect("使わない");
        assert_eq!(
            err(&with_header(&b, "TMSAVE", "TMSAVX")),
            SaveError::Magic("TMSAVX".into())
        );
        assert_eq!(
            err(&with_header(&b, "save_format: 1", "save_format: 2")),
            SaveError::Format { found: 2 }
        );
        let other = with_header(
            &b,
            &format!("rules_version: {RULES_VERSION}"),
            "rules_version: 2",
        );
        assert_eq!(err(&other), SaveError::RulesVersion { found: 2 });
        let h = SaveFile::header(&b).unwrap();
        let wrong = with_header(
            &b,
            &format!("config_hash: {}", h.config_hash),
            "config_hash: 5",
        );
        assert!(matches!(
            err(&wrong),
            SaveError::ConfigMismatch { found: 5, .. }
        ));
        let mut flipped = b.clone();
        let at = b.len() - 10;
        flipped[at] ^= 0x01;
        assert!(matches!(err(&flipped), SaveError::BodyFnv { .. }));
        let broken = with_body(&b, |s| s.replacen("SaveBody(", "SaveBody((", 1));
        assert!(matches!(err(&broken), SaveError::Body(_)));
        let money = format!("money_bits: {}", g.snapshot().money_bits);
        let changed = with_body(&b, |s| {
            s.replacen(&money, "money_bits: 4607182418800017408", 1)
        });
        assert!(
            matches!(
                err(&changed),
                SaveError::Snapshot(tm_core::SnapshotError::HashMismatch { .. })
            ),
            "{:?}",
            err(&changed)
        );
        let tick = with_header(&b, "tick: 200", "tick: 201");
        assert_eq!(err(&tick), SaveError::HeaderMismatch);
        let hash = with_header(
            &b,
            &format!("state_hash: {}", h.state_hash),
            "state_hash: 7",
        );
        assert_eq!(err(&hash), SaveError::HeaderMismatch);
        let seq = with_header(&b, "seq: 1,", &format!("seq: {},", u64::MAX));
        assert_eq!(err(&seq), SaveError::Seq { found: u64::MAX });
        let top = with_header(&b, "seq: 1,", &format!("seq: {MAX_SEQ},"));
        assert_eq!(load(&top).unwrap().seq, MAX_SEQ, "上限ちょうどは読む");
        assert!(matches!(err(b"no newline"), SaveError::Header(_)));
        assert!(matches!(err(b"(garbage\nbody"), SaveError::Header(_)));
        assert!(matches!(err(&[0xff, b'\n']), SaveError::Header(_)));
        let big = vec![b' '; MAX_SAVE_BYTES + 1];
        assert_eq!(
            err(&big),
            SaveError::TooLarge {
                len: MAX_SAVE_BYTES + 1
            }
        );
        for e in [SaveError::HeaderMismatch, SaveError::Format { found: 2 }] {
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn reincarnation_state_is_saved_and_checked() {
        let bal = Arc::new(BalanceTable::builtin());
        let rules = Arc::new(ReincarnationRules::builtin());
        let mut meta = ReincarnationState::new(rules.node_count());
        meta.souls = 4;
        meta.lifetime_souls = 9;
        meta.spent_souls = 5;
        meta.rebirths = 1;
        meta.raise(rules.node_id("seed_capital").unwrap());
        let g = Game::new_with_reincarnation_state(
            GameConfig {
                seed: 7,
                balance: Arc::clone(&bal),
            },
            Arc::clone(&rules),
            meta,
        );
        let b =
            SaveFile::encode(1, &g.snapshot(), &Tutorial::new(true).save(), "test").into_bytes();
        let l = SaveFile::load_with(&b, &bal, &rules).expect("転生の状態も読める");
        assert_eq!(l.game.reincarnation(), g.reincarnation());
        assert_eq!(l.game.reincarnation_hash(), g.reincarnation_hash());
        assert_eq!(l.game.state_hash(), g.state_hash());

        let mut spec = rules.spec().clone();
        spec.gain.base += 1;
        let other = Arc::new(ReincarnationRules::from_spec(spec).unwrap());
        assert!(matches!(
            SaveFile::load_with(&b, &bal, &other),
            Err(SaveError::Snapshot(
                tm_core::SnapshotError::ReincarnationConfigMismatch { .. }
            ))
        ));
    }

    #[test]
    fn other_version_is_found_even_if_the_header_changed_shape() {
        let g = game(3, 10);
        let b = encoded(1, &g);
        let (_, body) = lines(&b);
        let future =
            format!("SaveHeader(magic: \"TMSAVE\", save_format: 9, slots: [1, 2])\n{body}");
        assert_eq!(
            SaveFile::header(future.as_bytes()).err(),
            Some(SaveError::Format { found: 9 })
        );
        let unnamed = with_header(&b, "SaveHeader(", "(");
        assert!(load(&unnamed).is_ok(), "構造体の名前はなくてよい");
        let other_build = with_header(&b, "game_version: \"test\"", "game_version: \"v9\"");
        assert_eq!(load(&other_build).unwrap().game_version, "v9");
    }
}
