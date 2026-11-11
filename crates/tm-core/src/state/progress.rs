use crate::ids::{CompanyLevel, Level};
use crate::param::{FeatureSet, ParamSet};
use crate::units::Money;

/// 「1びょうで ○円」の平均をとる秒数(10 TD-15)。
pub const INCOME_WINDOW_SECS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompanyState {
    pub level: CompanyLevel,
    /// R2。路線の数の上限 = `line_slots(L)`。
    pub line_slots: Level,
    /// これまでに線路を敷いたマスの数 N(10 TD-41)。`TrackGrid` の空でないマスの数(INV-20)。
    pub track_tiles: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Progress {
    pub unlocked: FeatureSet,
    pub upgraded_once: ParamSet,
    pub total_earned: Money,
    /// 所持金が届いたことのある最大の単位 k(0..=17。10 TD-14)。
    pub max_money_unit: u8,
    pub goal_reached: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stats {
    pub income_this_sec: Money,
    pub income_window: [Money; INCOME_WINDOW_SECS],
    pub window_head: u8,
    pub arrivals: u64,
    pub delivered: u128,
    /// 成功した `LevelUp` コマンドの数。
    pub purchases: u32,
}
