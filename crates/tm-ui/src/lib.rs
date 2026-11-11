//! 画面遷移、レイアウト、`DrawList` の生成、音の命令への対応付け。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod anchor;
mod anim;
mod audio;
mod camera;
mod draw;
mod hud;
mod input_ctx;
mod layout;
mod request;
mod scene;
mod settings;
mod sound_map;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
mod text;
mod tutorial_bridge;
mod ui;

#[cfg(test)]
mod tests;

pub use request::{AppRequest, AppStatus};
pub use settings::Settings;
pub use ui::Ui;
