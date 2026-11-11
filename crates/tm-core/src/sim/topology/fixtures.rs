//! topology・movement のテストで使う駅・路線・列車の組み立て。

use super::*;
use crate::geom::GridPos;
use crate::ids::{CompanyLevel, Level};
use crate::param::StationLevels;
use crate::time::POS_UNITS_PER_TILE as U;
use crate::units::{MilliPeople, People};

pub(crate) fn bal() -> BalanceTable {
    BalanceTable::builtin()
}

/// 位置とホーム長のレベルから駅を作る(`StationId` は並びの順)。
pub(crate) fn stations(cells: &[(i32, i32, u8)]) -> Vec<Station> {
    cells
        .iter()
        .enumerate()
        .map(|(i, &(x, y, platform))| Station {
            id: StationId::from_index(i),
            pos: GridPos { x, y },
            spawned_at: Tick::ZERO,
            levels: StationLevels {
                platform: Level(platform),
                ..StationLevels::default()
            },
            spawn_acc: MilliPeople::ZERO,
            waiting_total: People::ZERO,
            serving_lines: Vec::new(),
        })
        .collect()
}

pub(crate) fn ids(v: &[u32]) -> Vec<StationId> {
    v.iter().map(|&i| StationId(i)).collect()
}

/// 両数のレベル `cars` の路線(曲がり方はすべて `StraightFirst`)。
pub(crate) fn line(id: u32, order: &[u32], st: &[Station], cars: u8) -> Line {
    let order = ids(order);
    let bends = vec![Bend::StraightFirst; order.len() - 1];
    let mut l = new_line(
        LineId(id),
        &order,
        &bends,
        st,
        &bal(),
        (Tick::ZERO, CompanyLevel::ZERO),
    );
    l.levels.cars = Level(cars);
    rebuild_line_derived(&mut l, st, &bal());
    l
}

pub(crate) fn train(
    id: u32,
    line: &Line,
    section: u16,
    dir: Direction,
    pos: u64,
    phase: TrainPhase,
) -> Train {
    Train {
        id: TrainId(id),
        line: line.id,
        section,
        dir,
        pos: PosUnits(pos),
        phase,
        riders: People::ZERO,
    }
}

/// 列車の路線の上の位置(駅 `origin` から測った符号つきの距離)。
pub(crate) fn line_x(line: &Line, tr: &Train, origin: StationId) -> i64 {
    let s = &line.sections[usize::from(tr.section)];
    let x = match tr.dir {
        Direction::Forward => offset_units(line, usize::from(s.from)) + tr.pos.0,
        Direction::Backward => offset_units(line, usize::from(s.to)) - tr.pos.0,
    };
    x as i64 - offset_units(line, station_index(line, origin)) as i64
}

/// INV-10(step の終わり。`allow_arriving` なら手順1〜4 の間の `pos == 区間長` も許す)。
pub(crate) fn check_inv10(line: &Line, tr: &Train, dwell_max: Ticks, allow_arriving: bool) {
    let k = usize::from(tr.section);
    assert!(k < line.sections.len(), "INV-10: 区間の添字 {tr:?}");
    let len = line.sections[k].length;
    match tr.phase {
        TrainPhase::Running => assert!(
            tr.pos < len || (allow_arriving && tr.pos == len),
            "INV-10: Running の pos {tr:?}"
        ),
        TrainPhase::Dwelling { at, remaining } => {
            assert_eq!(tr.pos, PosUnits(0), "INV-10: Dwelling の pos {tr:?}");
            let s = &line.sections[k];
            let dep = match tr.dir {
                Direction::Forward => s.from,
                Direction::Backward => s.to,
            };
            assert_eq!(at, dep, "INV-10: at が出発側の駅でない {tr:?}");
            assert!(remaining < dwell_max, "INV-10: remaining {tr:?}");
        }
    }
}

pub(crate) const TILE: u64 = U;
