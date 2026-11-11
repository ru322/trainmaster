//! プラットフォーム境界の型と `Platform` トレイト。feature `headless` でテスト用の実装を含む。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod assets;
mod audio;
mod draw;
mod geom;
#[cfg(feature = "headless")]
mod headless;
mod input;
mod platform;
mod time;

pub use assets::{AssetBundle, AssetError, AtlasImage, SpriteRect};
pub use audio::{AudioBus, AudioCmd, SoundData, SoundId, SoundKind};
pub use draw::{DrawCmd, DrawList, LayerId, SpriteId};
pub use geom::{Color, LOGICAL_HEIGHT, LOGICAL_WIDTH, Rect};
#[cfg(feature = "headless")]
pub use headless::{
    AssetSummary, AudioViolation, HeadlessClock, HeadlessConfig, HeadlessPlatform, RecordPolicy,
    SoftRaster,
};
pub use input::{Key, PadAxis, PadButton, PadId, PhysicalInput};
pub use platform::{AudioStats, Platform, PlatformError, PlatformStats};
pub use time::MonoTime;
