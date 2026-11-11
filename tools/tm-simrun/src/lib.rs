//! `tm-core` を高速に回し、自動プレイでゲームのペースを数値で確かめる開発用ツール。
//! 戦略は 01 付録 A.3(`strategy`)、停止と建設の時間は A.4(`scenario`)、流量モデルは A.1(`flow`)、セーブの読み込みは `save`。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod check;
pub mod flow;
pub mod report;
pub mod save;
pub mod scenario;
pub mod strategy;

pub use flow::FlowSim;
pub use report::{Report, csv_header};
pub use scenario::{Idle, Pause, Runner, Scenario, run, run_many};
pub use strategy::{Decision, GameWorld, Player, Route, World, decide, decide_route, decide_with};
