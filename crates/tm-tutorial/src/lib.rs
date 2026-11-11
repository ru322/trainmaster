//! チュートリアルの段階を進める状態機械と、UI との境界の型。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod anchor;
mod directive;
mod location;
mod permits;
mod save;
mod stages;
mod tutorial;
mod ui_event;

pub use anchor::{MenuAnchor, SettingsItem, UiAnchor};
pub use directive::{DirectiveKind, TutorialDirective, TutorialStageId};
pub use save::TutorialSave;
pub use stages::{stage_pages, stage_title};
pub use tutorial::Tutorial;
pub use ui_event::UiEvent;
