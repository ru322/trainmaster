//! 物理入力を抽象アクションに変換する。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod action;
mod bindings;
mod mapper;
mod stick;

pub use action::{Action, ActionFrame, ActionSet, InputDevice};
pub use bindings::{Bindings, RepeatRule};
pub use mapper::InputMapper;
