//! セーブに入れるチュートリアルの進み(13 §6.8、UI-D-37)。I/O はしない。

use serde::{Deserialize, Serialize};

/// チュートリアルの進み。`Tutorial::save` で作り、`Tutorial::restore` で戻す(13 §6.8)。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TutorialSave {
    pub(crate) stage_idx: u8,
    pub(crate) step: SavedStep,
    pub(crate) unshown: bool,
    pub(crate) replay_pending: bool,
    pub(crate) latch: SavedLatches,
    pub(crate) side: SavedSide,
    /// 見せたページを (段階の番号, ページ) で持つ。`TextId` の番号は `ja.toml` のキーで変わるため。
    pub(crate) shown: Vec<(u8, u8)>,
    pub(crate) shown_stages: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SavedStep {
    Wait,
    Explain { page: u8 },
    Guide { sub: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SavedSide {
    NotYet,
    Showing { page: u8 },
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedLatches {
    pub(crate) first_income: bool,
    pub(crate) purchases: u32,
    pub(crate) bought_max: bool,
    /// `ParamSet` のビット。
    pub(crate) bought: u16,
    pub(crate) lines_created: u32,
    pub(crate) max_line_station_count: u32,
    pub(crate) extended: bool,
    pub(crate) max_gauge_percent: u8,
    pub(crate) company_seen: u8,
    pub(crate) goal: bool,
    pub(crate) newest_station: Option<u32>,
}
