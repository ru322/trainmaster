//! 路線の形: 線路のマス、停車駅・区間・累積距離の作り直し、列車の移し替え(11 §4)。

use crate::balance::BalanceTable;
use crate::command::LineEnd;
use crate::geom::{Bend, Dir8, GridPos, TrackSegment};
use crate::ids::{CompanyLevel, LineId, StationId, TrainId};
use crate::param::LineLevels;
use crate::state::{Direction, Line, Section, Station, TrackGrid, Train, TrainPhase, lay_track};
use crate::time::{POS_UNITS_PER_TILE, Tick, Ticks};
use crate::units::{PosUnits, Tiles, section_length};

/// 路線の形を変える前の列車の位置(11 §4.6)。駅は ID で持つので、先頭への延伸で添字がずれても困らない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TrainAnchor {
    pub train: TrainId,
    pub dep_stop: StationId,
    pub arr_stop: StationId,
    pub dir: Direction,
    pub pos: PosUnits,
    pub kind: AnchorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnchorKind {
    Running,
    /// 手順1で置いた直後の `pos == 区間長`。
    Arriving,
    Dwelling {
        remaining: Ticks,
    },
}

/// 停車駅の変化(`PassThroughAdded` / `PassThroughResolved` を出す順に並べるため)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PassChange {
    pub line: LineId,
    pub idx: u16,
    pub station: StationId,
    pub now_stop: bool,
}

/// `stations[0]` から `stations[i]` までの距離(`PosUnits`)。
pub(crate) fn offset_units(line: &Line, i: usize) -> u64 {
    u64::from(line.offsets[i]) * POS_UNITS_PER_TILE
}

/// `line.stations` の中の `s` の添字。路線にない駅なら panic する。
pub(crate) fn station_index(line: &Line, s: StationId) -> usize {
    line.stations
        .iter()
        .position(|&x| x == s)
        .unwrap_or_else(|| panic!("{s} は路線 {} にない", line.id))
}

/// `from` → `to` の線路。`bend` は `from` から見た曲がり方で、そのまま保存する(10 TD-23)。
pub(crate) fn make_segment(
    stations: &[Station],
    from: StationId,
    to: StationId,
    bend: Bend,
) -> TrackSegment {
    let mut tiles = Vec::new();
    lay_track(
        stations[from.index()].pos,
        stations[to.index()].pos,
        bend,
        &mut tiles,
    );
    let len = u32::try_from(tiles.len() - 1).expect("線路の長さが u32 を超えた");
    TrackSegment {
        from,
        to,
        bend,
        len: Tiles(len),
        tiles,
    }
}

/// 新しい路線(列車なし、レベル 0)。`bends[i]` は `ids[i]` → `ids[i+1]` の曲がり方(11 §4.3)。
pub(crate) fn new_line(
    id: LineId,
    ids: &[StationId],
    bends: &[Bend],
    stations: &[Station],
    bal: &BalanceTable,
    created: (Tick, CompanyLevel),
) -> Line {
    assert!(
        ids.len() >= 2 && bends.len() == ids.len() - 1,
        "路線の駅と曲がり方の数"
    );
    let segments = ids
        .windows(2)
        .zip(bends)
        .map(|(w, &b)| make_segment(stations, w[0], w[1], b))
        .collect();
    let mut line = Line {
        id,
        stations: ids.to_vec(),
        segments,
        levels: LineLevels::default(),
        trains: Vec::new(),
        stops: Vec::new(),
        sections: Vec::new(),
        offsets: Vec::new(),
        created_at: created.0,
        born_level: created.1,
    };
    rebuild_line_derived(&mut line, stations, bal);
    line
}

/// 端に駅 `s` を足す(11 §4.4)。`bend` は既存の端から見た曲がり方で、先頭では反転して保存する(10 TD-23)。
/// 導出データは作り直さない。足した区間の `segments` の添字を返す。
pub(crate) fn extend_shape(
    line: &mut Line,
    stations: &[Station],
    end: LineEnd,
    s: StationId,
    bend: Bend,
) -> usize {
    match end {
        LineEnd::Tail => {
            let tail = *line.stations.last().expect("路線に駅がある");
            line.segments.push(make_segment(stations, tail, s, bend));
            line.stations.push(s);
            line.segments.len() - 1
        }
        LineEnd::Head => {
            let head = line.stations[0];
            line.segments
                .insert(0, make_segment(stations, s, head, bend.reversed()));
            line.stations.insert(0, s);
            0
        }
    }
}

/// 導出データ(`stops`、`sections`、`offsets`)を作り直す(11 §4.5)。列車は触らない。
pub(crate) fn rebuild_line_derived(line: &mut Line, stations: &[Station], bal: &BalanceTable) {
    let cars = bal.cars(line.levels.cars).0;
    line.stops.clear();
    line.offsets.clear();
    let mut acc = 0u32;
    for (i, &s) in line.stations.iter().enumerate() {
        line.stops
            .push(bal.platform(stations[s.index()].levels.platform).0 >= cars);
        line.offsets.push(acc);
        if let Some(seg) = line.segments.get(i) {
            acc += seg.len.0;
        }
    }
    line.sections.clear();
    let mut prev: Option<usize> = None;
    for i in 0..line.stations.len() {
        if !line.stops[i] {
            continue;
        }
        if let Some(p) = prev {
            let tiles = Tiles(line.offsets[i] - line.offsets[p]);
            line.sections.push(Section {
                from: idx16(p),
                to: idx16(i),
                tiles,
                length: section_length(tiles),
            });
        }
        prev = Some(i);
    }
}

fn idx16(i: usize) -> u16 {
    u16::try_from(i).expect("路線の駅の数が u16 を超えた")
}

/// 線路のマスの列を `grid` に OR で書く(11 §4.2)。
pub(crate) fn add_line_tracks(grid: &mut TrackGrid, tiles: &[GridPos]) {
    for w in tiles.windows(2) {
        let d = Dir8::from_delta(w[1].x - w[0].x, w[1].y - w[0].y);
        grid.or(w[0], d);
        grid.or(w[1], d.opposite());
    }
}

/// 全路線の線路から作り直した `TrackGrid`(INV-13 の比較用)。
pub(crate) fn tracks_from_lines(width: u8, height: u8, lines: &[Line]) -> TrackGrid {
    let mut grid = TrackGrid::new(width, height);
    for seg in lines.iter().flat_map(|l| &l.segments) {
        add_line_tracks(&mut grid, &seg.tiles);
    }
    grid
}

/// `serving_lines` に `l` を `LineId` 昇順で足す(すでにあれば何もしない)。
pub(crate) fn insert_serving_line(serving: &mut Vec<LineId>, l: LineId) {
    if let Err(i) = serving.binary_search(&l) {
        serving.insert(i, l);
    }
}

/// 路線の形を変える前に、その路線の列車の位置を記録する(11 §4.6)。
pub(crate) fn snapshot_anchors(line: &Line, trains: &[Train], out: &mut Vec<TrainAnchor>) {
    out.clear();
    debug_assert!(!line.sections.is_empty(), "INV-16");
    for &t in &line.trains {
        let tr = &trains[t.index()];
        let sec = &line.sections[usize::from(tr.section)];
        let (dep, arr) = match tr.dir {
            Direction::Forward => (sec.from, sec.to),
            Direction::Backward => (sec.to, sec.from),
        };
        let kind = match tr.phase {
            TrainPhase::Dwelling { remaining, .. } => AnchorKind::Dwelling { remaining },
            TrainPhase::Running if tr.pos == sec.length => AnchorKind::Arriving,
            TrainPhase::Running => AnchorKind::Running,
        };
        out.push(TrainAnchor {
            train: t,
            dep_stop: line.stations[usize::from(dep)],
            arr_stop: line.stations[usize::from(arr)],
            dir: tr.dir,
            pos: tr.pos,
            kind,
        });
    }
}

/// 作り直した区間の上で列車の位置を求め直す(11 §4.6、SIM-D-06)。区間は出発側の端を含み、到着側の端を含まない。
pub(crate) fn relocate_trains(line: &Line, anchors: &[TrainAnchor], trains: &mut [Train]) {
    debug_assert!(!line.sections.is_empty(), "INV-16");
    for a in anchors {
        let tr = &mut trains[a.train.index()];
        tr.dir = a.dir;
        if a.kind == AnchorKind::Arriving {
            let j = station_index(line, a.arr_stop);
            debug_assert!(line.stops[j], "到着側の駅が通過駅になった");
            let k = section_arriving_at(line, j, a.dir);
            tr.section = k;
            tr.pos = line.sections[usize::from(k)].length;
            tr.phase = TrainPhase::Running;
            continue;
        }
        let i = station_index(line, a.dep_stop);
        let base = offset_units(line, i);
        let off = |j: u16| offset_units(line, usize::from(j));
        let (k, pos) = match a.dir {
            Direction::Forward => {
                let x = base + a.pos.0;
                let k = find_section(line, |s| off(s.from) <= x && x < off(s.to));
                (k, x - off(line.sections[usize::from(k)].from))
            }
            Direction::Backward => {
                let x = base - a.pos.0;
                let k = find_section(line, |s| off(s.from) < x && x <= off(s.to));
                (k, off(line.sections[usize::from(k)].to) - x)
            }
        };
        tr.section = k;
        tr.pos = PosUnits(pos);
        tr.phase = match a.kind {
            AnchorKind::Dwelling { remaining } => {
                debug_assert!(line.stops[i], "停車中の駅が通過駅になった");
                TrainPhase::Dwelling {
                    at: idx16(i),
                    remaining,
                }
            }
            _ => TrainPhase::Running,
        };
    }
}

/// `pred` を満たす最初の区間。なければ panic する。
pub(crate) fn find_section(line: &Line, pred: impl Fn(&Section) -> bool) -> u16 {
    let k = line
        .sections
        .iter()
        .position(pred)
        .expect("位置を含む区間がない");
    idx16(k)
}

/// 位置 `x`(`stations[0]` からの距離)にちょうど重なる停車駅の添字。
pub(crate) fn find_stop_at(line: &Line, x: u64) -> Option<usize> {
    (0..line.stations.len()).find(|&j| line.stops[j] && offset_units(line, j) == x)
}

/// 停車駅 `j` に向き `dir` で着く区間。
pub(crate) fn section_arriving_at(line: &Line, j: usize, dir: Direction) -> u16 {
    let j = idx16(j);
    match dir {
        Direction::Forward => find_section(line, |s| s.to == j),
        Direction::Backward => find_section(line, |s| s.from == j),
    }
}

#[cfg(test)]
pub(crate) mod fixtures;

#[cfg(test)]
mod tests;
