use crate::ids::{LineId, TrainId};
use crate::time::Ticks;
use crate::units::{People, PosUnits};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Train {
    pub id: TrainId,
    pub line: LineId,
    /// Running: いま走っている区間。Dwelling: 出発する区間(10 §3.6)。
    pub section: u16,
    pub dir: Direction,
    /// 区間の始点(進行方向の出発側の停車駅)からの距離。
    pub pos: PosUnits,
    pub phase: TrainPhase,
    /// 導出: `onboard` の行の合計。
    pub riders: People,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    /// `stations` の添字が増える向き。
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrainPhase {
    Running,
    /// `at` = 停車している駅の添字、`remaining` = この tick を含めない残り。
    Dwelling {
        at: u16,
        remaining: Ticks,
    },
}
