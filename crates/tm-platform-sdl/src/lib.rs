//! `Platform` トレイトの SDL2 実装(ウィンドウ、描画、入力、音、時計)。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(target_endian = "big")]
compile_error!(
    "アトラスを RGBA32(リトルエンディアンの ABGR8888)で転送するため、ビッグエンディアンには対応しない"
);

mod audio;
mod clock;
mod config;
mod core;
mod diag;
mod input;
mod pad_mappings;
mod platform;
mod render;
#[cfg(test)]
mod tests;

pub use crate::config::{AudioConfig, BatchMode, DisplayMode, FrameCap, SdlConfig};
pub use crate::core::SdlCore;
pub use crate::platform::SdlPlatform;
