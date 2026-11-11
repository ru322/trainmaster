//! メインループと各クレートの組み立て。実行ファイル `trainmaster` は feature `sdl` のときだけビルドする。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod app;
pub mod clock;
mod config;
mod error;
pub mod logging;
pub mod overlay;
pub mod profiler;
mod replay_out;
pub mod save;
pub mod seed;
mod version;

pub use app::{App, AppExit, AppStats, EXIT_POWER_OFF, FrameOutcome, ShutdownReason, run};
pub use config::{
    AppConfig, ConfigWarning, DEVTOOLS_VARS, DisplayChoice, ExitAction, PlatformEnv, parse_seed,
};
pub use error::AppError;
pub use replay_out::{ReplayPolicy, ReplaySink, file_name as replay_file_name};
pub use save::{FsStore, MemStore, NullStore, SAVE_FORMAT_VERSION, SavePolicy, SaveStore, Slot};
pub use seed::{SeedPolicy, SeedSource};
pub use version::VersionInfo;
