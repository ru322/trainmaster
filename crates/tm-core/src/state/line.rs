use crate::geom::TrackSegment;
use crate::ids::{CompanyLevel, LineId, StationId, TrainId};
use crate::param::LineLevels;
use crate::time::Tick;
use crate::units::{PosUnits, Tiles};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    pub id: LineId,
    /// Head から Tail の順。2 個以上、重複なし。
    pub stations: Vec<StationId>,
    /// `segments[i]` は `stations[i]` → `stations[i+1]` の向きで保存(10 TD-23)。
    pub segments: Vec<TrackSegment>,
    pub levels: LineLevels,
    pub trains: Vec<TrainId>,
    /// 導出: 停車するか(`stations` と同じ長さ)。
    pub stops: Vec<bool>,
    /// 導出: 隣り合う停車駅の間 [02 D28]。
    pub sections: Vec<Section>,
    /// 導出: 各駅の `stations[0]` からの累積マス。
    pub offsets: Vec<u32>,
    pub created_at: Tick,
    /// 作ったときの会社レベル(強化の値段の下限。M8)。
    pub born_level: CompanyLevel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Section {
    /// `stations` の添字(停車駅)。
    pub from: u16,
    /// 次の停車駅の添字(`from < to`)。
    pub to: u16,
    /// 通過駅の分を含む長さ。
    pub tiles: Tiles,
    pub length: PosUnits,
}
