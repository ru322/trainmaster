//! 環境変数による設定 [16 §1.12、APP-D-03、APP-D-04、APP-D-09]。

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use tm_core::{BalanceTable, ReincarnationRules};
use tm_input::Bindings;

use crate::error::AppError;
use crate::logging;
use crate::replay_out::ReplayPolicy;
use crate::save::SavePolicy;
use crate::seed::SeedPolicy;
use crate::version::VersionInfo;

/// SDL に依存しないアプリの設定。
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// ゲームの数値の表(既定は埋め込み)。
    pub balance: Arc<BalanceTable>,
    /// 転生の設定(既定は埋め込みの `config/reincarnation.toml`)。
    pub reincarnation: Arc<ReincarnationRules>,
    /// キー割り当て(v1 は `Bindings::default_v1()` だけ。APP-D-10)。
    pub bindings: Bindings,
    /// シードの決め方。
    pub seed: SeedPolicy,
    /// リプレイの記録と書き出し。
    pub replay: ReplayPolicy,
    /// 時間の倍率(1 = 実時間)。
    pub time_scale: u32,
    /// デバッグ表示。
    pub overlay: bool,
    /// このフレーム数で終わる。
    pub quit_after_frames: Option<u64>,
    /// セーブと `settings.ron` の置き場所(`TM_SAVE_DIR`)。既定は保存しない。
    pub save: SavePolicy,
    /// 「ゲームを終わる」の終わり方(`TM_EXIT_ACTION`)。
    pub exit_action: ExitAction,
    /// 版の情報。
    pub version: VersionInfo,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            balance: Arc::new(BalanceTable::builtin()),
            reincarnation: Arc::new(ReincarnationRules::builtin()),
            bindings: Bindings::default_v1(),
            seed: SeedPolicy::Os,
            replay: ReplayPolicy::Off,
            time_scale: 1,
            overlay: false,
            quit_after_frames: None,
            save: SavePolicy::Off,
            exit_action: ExitAction::Exit,
            version: VersionInfo::build(),
        }
    }
}

/// 「ゲームを終わる」の後にどうするか [16 §1.15]。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExitAction {
    /// アプリが終わるだけ(終了コード 0)。開発機。
    #[default]
    Exit,
    /// 電源を切る要求(終了コード 10)。電源を切るのは `tm-launch`。
    PowerOff,
}

/// 表示の方式(`SdlConfig::display` に写す)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayChoice {
    /// ビデオのドライバで決める(画面のない dummy・offscreen は 1280×720 のウィンドウ、それ以外は KMSDRM も開発機もフルスクリーン)。
    Auto,
    /// デスクトップの解像度のフルスクリーン。
    Fullscreen,
    /// 640×360 × `scale` のウィンドウ(3 で 1920×1080)。
    Windowed {
        /// 倍率(1..=6)。
        scale: u8,
    },
}

/// SDL のプラットフォームへの設定(bin が `SdlConfig` に写す)。値は既定を解決済み。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformEnv {
    /// `SDL_VIDEODRIVER=kmsdrm`(実機の既定を使う)。
    pub target: bool,
    /// 表示の方式。
    pub display: DisplayChoice,
    /// ウィンドウの倍率(`TM_WINDOW_SCALE`、640×360 の何倍か)。フルスクリーンから F11 で戻るウィンドウにも使う。
    pub window_scale: u8,
    /// 30fps に落とす。
    pub fps30: bool,
    /// vsync を使う。
    pub vsync: bool,
    /// `BatchMode::Copy` で描く。
    pub copy_batch: bool,
    /// `SDL_RENDER_DRIVER`。`None` は SDL に任せる。
    pub render_driver: Option<String>,
    /// 音を出す。
    pub audio: bool,
    /// 音のバッファのサンプル数。
    pub audio_buffer: u16,
    /// 開発用のキー。`None` はビルドの種類で決める。
    pub dev_hotkeys: Option<bool>,
}

/// 設定の警告。起動は止めない。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigWarning {
    /// 値が読めない・範囲外なので既定値にした。
    Invalid {
        /// 変数名。
        var: &'static str,
        /// 値。
        value: String,
    },
    /// devtools なしのビルドなので無視した。
    DevtoolsOnly {
        /// 変数名。
        var: &'static str,
    },
    /// `TM_SAVE_DIR` も `XDG_DATA_HOME` も `HOME` もないので保存しない。
    NoSaveDir,
}

impl fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigWarning::Invalid { var, value } => {
                write!(f, "{var}={value:?} は使えない値。既定値を使う")
            }
            ConfigWarning::DevtoolsOnly { var } => {
                write!(f, "{var} は devtools のビルドだけで使える。無視した")
            }
            ConfigWarning::NoSaveDir => {
                write!(f, "TM_SAVE_DIR がなく HOME もないので保存しない")
            }
        }
    }
}

/// devtools のビルドだけで読む変数。
pub const DEVTOOLS_VARS: [&str; 8] = [
    "TM_DEV_HOTKEYS",
    "TM_SEED",
    "TM_REPLAY",
    "TM_REPLAY_DIR",
    "TM_BALANCE_FILE",
    "TM_DEBUG_OVERLAY",
    "TM_TIME_SCALE",
    "TM_QUIT_AFTER_FRAMES",
];

struct Env {
    vars: BTreeMap<String, String>,
    warnings: Vec<ConfigWarning>,
}

impl Env {
    fn get(&self, var: &'static str) -> Option<&str> {
        self.vars.get(var).map(|v| v.trim())
    }

    fn parse<T>(&mut self, var: &'static str, f: impl FnOnce(&str) -> Option<T>) -> Option<T> {
        let raw = self.get(var)?.to_owned();
        let v = f(&raw.to_ascii_lowercase());
        if v.is_none() {
            self.warnings
                .push(ConfigWarning::Invalid { var, value: raw });
        }
        v
    }

    fn on_off(&mut self, var: &'static str) -> Option<bool> {
        self.parse(var, |s| match s {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        })
    }

    fn range<T: std::str::FromStr + PartialOrd>(
        &mut self,
        var: &'static str,
        lo: T,
        hi: T,
    ) -> Option<T> {
        self.parse(var, |s| s.parse().ok().filter(|v| *v >= lo && *v <= hi))
    }

    fn raw(&mut self, var: &'static str) -> Option<String> {
        let v = self.get(var)?.to_owned();
        if v.is_empty() {
            self.warnings.push(ConfigWarning::Invalid { var, value: v });
            return None;
        }
        Some(v)
    }
}

/// `TM_SEED`: 10 進か `0x` の 16 進の u64。
pub fn parse_seed(s: &str) -> Option<u64> {
    let s = s.trim().replace('_', "");
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

fn platform_env(env: &mut Env) -> PlatformEnv {
    let target = env
        .vars
        .get("SDL_VIDEODRIVER")
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("kmsdrm"));
    let scale = env.range("TM_WINDOW_SCALE", 1u8, 6).unwrap_or(2);
    let display = env
        .parse("TM_DISPLAY", |s| match s {
            "auto" => Some(DisplayChoice::Auto),
            "fullscreen" => Some(DisplayChoice::Fullscreen),
            "window" => Some(DisplayChoice::Windowed { scale }),
            _ => None,
        })
        .unwrap_or(DisplayChoice::Auto);
    let fps30 = env
        .parse("TM_FPS", |s| match s {
            "60" => Some(false),
            "30" => Some(true),
            _ => None,
        })
        .unwrap_or(false);
    let vsync = env.on_off("TM_VSYNC").unwrap_or(true);
    let copy_batch = env
        .parse("TM_RENDER_BATCH", |s| match s {
            "geometry" => Some(false),
            "copy" => Some(true),
            _ => None,
        })
        .unwrap_or(false);
    let default_driver = target.then(|| "opengles2".to_owned());
    let render_driver = match env.get("TM_RENDER_DRIVER").map(str::to_owned) {
        None => default_driver,
        Some(v) if v.eq_ignore_ascii_case("auto") => None,
        Some(v)
            if !v.is_empty()
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') =>
        {
            Some(v)
        }
        Some(v) => {
            env.warnings.push(ConfigWarning::Invalid {
                var: "TM_RENDER_DRIVER",
                value: v,
            });
            default_driver
        }
    };
    let audio = env.on_off("TM_AUDIO").unwrap_or(true);
    let audio_buffer = env
        .parse("TM_AUDIO_BUFFER", |s| {
            s.parse()
                .ok()
                .filter(|n| matches!(n, 512 | 1024 | 2048 | 4096))
        })
        .unwrap_or(1024);
    let _ = env.parse("TM_LOG", logging::parse_level);
    PlatformEnv {
        target,
        display,
        window_scale: scale,
        fps30,
        vsync,
        copy_batch,
        render_driver,
        audio,
        audio_buffer,
        dev_hotkeys: None,
    }
}

/// `TM_SAVE_DIR`: `off`、パス、なければ `$XDG_DATA_HOME/trainmaster` か `$HOME/.local/share/trainmaster`。
/// ディレクトリは `App::new` が作る(作れなければ警告して保存しない)。
fn save_policy(env: &mut Env) -> SavePolicy {
    if let Some(v) = env.raw("TM_SAVE_DIR") {
        return if v.eq_ignore_ascii_case("off") {
            SavePolicy::Off
        } else {
            SavePolicy::Dir(PathBuf::from(v))
        };
    }
    let xdg = env
        .get("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    let home = env
        .get("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .map(|h| h.join(".local/share"));
    match xdg.or(home) {
        Some(base) => SavePolicy::Dir(base.join("trainmaster")),
        None => {
            env.warnings.push(ConfigWarning::NoSaveDir);
            SavePolicy::Off
        }
    }
}

/// tm-launch の `tr '[:space:]'` と同じ ASCII の空白。
fn is_shell_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0B' | '\x0C' | '\r')
}

/// `TM_EXIT_ACTION`: 前後の ASCII の空白だけを除き、大文字小文字を区別しない(tm-launch と同じ規則。03 §5.3)。
fn exit_action(env: &mut Env) -> ExitAction {
    let Some(v) = env.vars.get("TM_EXIT_ACTION") else {
        return ExitAction::Exit;
    };
    let v = v.trim_matches(is_shell_space).to_owned();
    match v.to_ascii_lowercase().as_str() {
        "poweroff" => ExitAction::PowerOff,
        "exit" => ExitAction::Exit,
        _ => {
            env.warnings.push(ConfigWarning::Invalid {
                var: "TM_EXIT_ACTION",
                value: v,
            });
            ExitAction::Exit
        }
    }
}

/// `TM_REINCARNATION_FILE`: 転生の設定の TOML。配布用のビルドでも読む。
fn reincarnation_config(env: &mut Env, app: &mut AppConfig) -> Result<(), AppError> {
    if let Some(path) = env.raw("TM_REINCARNATION_FILE") {
        let src = std::fs::read_to_string(&path).map_err(|e| {
            AppError::Config(format!("TM_REINCARNATION_FILE={path} を読めない: {e}"))
        })?;
        app.reincarnation =
            Arc::new(ReincarnationRules::from_toml_str(&src).map_err(AppError::Reincarnation)?);
    }
    Ok(())
}

fn devtools(env: &mut Env, app: &mut AppConfig, penv: &mut PlatformEnv) -> Result<(), AppError> {
    penv.dev_hotkeys = env.on_off("TM_DEV_HOTKEYS");
    if let Some(s) = env.parse("TM_SEED", parse_seed) {
        app.seed = SeedPolicy::Fixed(s);
    }
    let dir = env
        .raw("TM_REPLAY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    app.replay = if env.on_off("TM_REPLAY").unwrap_or(true) {
        ReplayPolicy::Dir(dir)
    } else {
        ReplayPolicy::Off
    };
    if let Some(path) = env.raw("TM_BALANCE_FILE") {
        let src = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Config(format!("TM_BALANCE_FILE={path} を読めない: {e}")))?;
        app.balance = Arc::new(BalanceTable::from_ron_str(&src).map_err(AppError::Balance)?);
    }
    app.overlay = env.on_off("TM_DEBUG_OVERLAY").unwrap_or(false);
    app.time_scale = env.range("TM_TIME_SCALE", 1u32, 16).unwrap_or(1);
    app.quit_after_frames = env.range("TM_QUIT_AFTER_FRAMES", 1u64, u64::MAX);
    Ok(())
}

impl AppConfig {
    /// 環境変数から設定を作る。`Err` は devtools の `TM_BALANCE_FILE` が読めない・検証を通らないときだけ。
    pub fn from_env(
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Result<(AppConfig, PlatformEnv, Vec<ConfigWarning>), AppError> {
        let mut env = Env {
            vars: vars
                .into_iter()
                .filter(|(k, _)| {
                    k.starts_with("TM_")
                        || matches!(k.as_str(), "SDL_VIDEODRIVER" | "HOME" | "XDG_DATA_HOME")
                })
                .collect(),
            warnings: Vec::new(),
        };
        let mut app = AppConfig::default();
        let mut penv = platform_env(&mut env);
        app.save = save_policy(&mut env);
        app.exit_action = exit_action(&mut env);
        reincarnation_config(&mut env, &mut app)?;
        if cfg!(feature = "devtools") {
            devtools(&mut env, &mut app, &mut penv)?;
        } else {
            for var in DEVTOOLS_VARS {
                if env.vars.contains_key(var) {
                    env.warnings.push(ConfigWarning::DevtoolsOnly { var });
                }
            }
        }
        Ok((app, penv, env.warnings))
    }
}
