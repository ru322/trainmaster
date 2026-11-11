use crate::geom::GridPos;
use crate::ids::{LineId, StationId};
use crate::param::StationLevels;
use crate::time::Tick;
use crate::units::{MilliPeople, People};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Station {
    pub id: StationId,
    pub pos: GridPos,
    /// 最初の駅は `Tick(0)`。
    pub spawned_at: Tick,
    pub levels: StationLevels,
    pub spawn_acc: MilliPeople,
    /// 導出: `waiting` の行の合計。
    pub waiting_total: People,
    /// 導出: この駅を含む路線(`LineId` 昇順。通過する路線も含む)。
    pub serving_lines: Vec<LineId>,
}

impl Station {
    /// 路線に入っているか [01 D-26]。
    pub fn is_connected(&self) -> bool {
        !self.serving_lines.is_empty()
    }
}
