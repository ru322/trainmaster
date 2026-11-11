//! 列車の移動・到着・発車・折り返し、周の座標、列車の配置(11 §5)。

use super::topology::{find_section, find_stop_at, offset_units, section_arriving_at};
use crate::balance::BalanceTable;
use crate::ids::{LineId, StationId, TrainId};
use crate::state::{Direction, GameState, Line, PaxMatrix, Train, TrainPhase};
use crate::time::Ticks;
use crate::units::{People, PosUnits};

/// 手順3: `TrainId` 昇順に停車の残りを減らし、発車し、`v‰` 進める。到着した列車を `arrived` に積む(11 §5.2)。
pub(crate) fn advance_all(st: &mut GameState, bal: &BalanceTable, arrived: &mut Vec<TrainId>) {
    advance_trains(&st.lines, &mut st.trains, bal, arrived);
}

/// `advance_all` の本体(`GameState` の他のフィールドを借りない)。
pub(crate) fn advance_trains(
    lines: &[Line],
    trains: &mut [Train],
    bal: &BalanceTable,
    arrived: &mut Vec<TrainId>,
) {
    arrived.clear();
    for tr in trains.iter_mut() {
        let line = &lines[tr.line.index()];
        debug_assert!(!line.sections.is_empty(), "INV-16");
        match tr.phase {
            TrainPhase::Dwelling { at, remaining } if remaining.0 > 0 => {
                tr.phase = TrainPhase::Dwelling {
                    at,
                    remaining: Ticks(remaining.0 - 1),
                };
                continue;
            }
            // SIM-D-04: 停車の最後の tick の次の tick に pos = 0 から発車し、この tick のうちに進む。
            TrainPhase::Dwelling { .. } => tr.phase = TrainPhase::Running,
            TrainPhase::Running => {}
        }
        let v = u64::from(bal.speed(line.levels.speed).0);
        let len = line.sections[usize::from(tr.section)].length.0;
        if tr.pos.0 + v >= len {
            tr.pos = PosUnits(len);
            arrived.push(tr.id);
        } else {
            tr.pos.0 += v;
        }
    }
}

/// 区間 `k` を向き `d` で着いた後に走る区間と向き。端の停車駅では反転する [02 D26]。
pub(crate) fn departure_leg(line: &Line, k: u16, d: Direction) -> (u16, Direction) {
    let n = line.sections.len();
    match d {
        Direction::Forward if usize::from(k) + 1 < n => (k + 1, Direction::Forward),
        Direction::Forward => (k, Direction::Backward),
        Direction::Backward if k > 0 => (k - 1, Direction::Backward),
        Direction::Backward => (k, Direction::Forward),
    }
}

/// 区間 `k` を向き `d` で走ったときに着く駅の添字。
pub(crate) fn arrival_index(line: &Line, k: u16, d: Direction) -> usize {
    let s = &line.sections[usize::from(k)];
    usize::from(match d {
        Direction::Forward => s.to,
        Direction::Backward => s.from,
    })
}

/// 区間 `k` を向き `d` で走るときの出発側の駅の添字。
pub(crate) fn departure_index(line: &Line, k: u16, d: Direction) -> usize {
    let s = &line.sections[usize::from(k)];
    usize::from(match d {
        Direction::Forward => s.from,
        Direction::Backward => s.to,
    })
}

/// 到着した列車を「出発する区間の `pos = 0`」に移し、次の停車駅(12 RT-D-12 の `train_next_stop`)を返す。
pub(crate) fn turn_at_arrival(line: &Line, tr: &mut Train) -> StationId {
    let (k, d) = departure_leg(line, tr.section, tr.dir);
    tr.section = k;
    tr.dir = d;
    tr.pos = PosUnits(0);
    line.stations[arrival_index(line, k, d)]
}

/// `turn_at_arrival` の後に、到着の tick を 1 tick 目として `dwell` tick 停車させる(11 §5.2)。`dwell ≥ 1`。
pub(crate) fn begin_dwell(line: &Line, tr: &mut Train, dwell: Ticks) {
    assert!(dwell.0 >= 1, "停車は 1 tick 以上(BV-15)");
    debug_assert_eq!(tr.pos, PosUnits(0), "出発する区間の pos = 0 で停車する");
    tr.phase = TrainPhase::Dwelling {
        at: u16::try_from(departure_index(line, tr.section, tr.dir)).expect("添字は u16"),
        remaining: Ticks(dwell.0 - 1),
    };
}

/// いまの区間の到着側の駅(停車中なら次に止まる駅)。
pub(crate) fn next_stop(line: &Line, tr: &Train) -> StationId {
    line.stations[arrival_index(line, tr.section, tr.dir)]
}

/// 最初と最後の停車駅の添字。
fn end_stops(line: &Line) -> (usize, usize) {
    debug_assert!(!line.sections.is_empty(), "INV-16");
    let last = line.sections.len() - 1;
    (
        usize::from(line.sections[0].from),
        usize::from(line.sections[last].to),
    )
}

/// 往復 1 周の長さ `P = 2L`(11 §5.4)。
pub(crate) fn cycle_len(line: &Line) -> u64 {
    let (f, t) = end_stops(line);
    2 * (offset_units(line, t) - offset_units(line, f))
}

/// 列車の周の座標 `c ∈ [0, P)`。停車中は止まっている駅、到着した直後は着く駅の座標。
pub(crate) fn cycle_coord(line: &Line, tr: &Train) -> u64 {
    let (f, t) = end_stops(line);
    let (lf, lt) = (offset_units(line, f), offset_units(line, t));
    let sec = &line.sections[usize::from(tr.section)];
    let c = match tr.dir {
        Direction::Forward => offset_units(line, usize::from(sec.from)) - lf + tr.pos.0,
        Direction::Backward => {
            (lt - lf) + (lt - offset_units(line, usize::from(sec.to))) + tr.pos.0
        }
    };
    c % (2 * (lt - lf))
}

/// 周の座標 `c` に置く列車の区間・向き・位置。停車駅に重なれば、その駅に着く区間の終点に置く(SIM-D-05)。
pub(crate) fn place_at(line: &Line, c: u64) -> (u16, Direction, PosUnits) {
    let (f, t) = end_stops(line);
    let (lf, lt) = (offset_units(line, f), offset_units(line, t));
    let l = lt - lf;
    debug_assert!(c < 2 * l, "周の座標が 1 周を超えた");
    let off = |j: u16| offset_units(line, usize::from(j));
    let end = |k: u16| line.sections[usize::from(k)].length;
    let last = u16::try_from(line.sections.len() - 1).expect("区間の数は u16");
    if c < l {
        let x = lf + c;
        if let Some(j) = find_stop_at(line, x) {
            if c == 0 {
                return (0, Direction::Backward, end(0));
            }
            let k = section_arriving_at(line, j, Direction::Forward);
            return (k, Direction::Forward, end(k));
        }
        let k = find_section(line, |s| off(s.from) <= x && x < off(s.to));
        (
            k,
            Direction::Forward,
            PosUnits(x - off(line.sections[usize::from(k)].from)),
        )
    } else {
        let x = lt - (c - l);
        if let Some(j) = find_stop_at(line, x) {
            if c == l {
                return (last, Direction::Forward, end(last));
            }
            let k = section_arriving_at(line, j, Direction::Backward);
            return (k, Direction::Backward, end(k));
        }
        let k = find_section(line, |s| off(s.from) < x && x <= off(s.to));
        (
            k,
            Direction::Backward,
            PosUnits(off(line.sections[usize::from(k)].to) - x),
        )
    }
}

/// 路線を作ったときの `k` 本目(`0..n`)の座標: 1 周 ÷ `n` を切り捨てて等分(02 D28)。
pub(crate) fn initial_coord(line: &Line, k: u32, n: u32) -> u64 {
    u64::from(k) * (cycle_len(line) / u64::from(n))
}

/// R1 で足す列車の座標: いちばん大きい隙間(同じなら始まりが小さい方)の真ん中(SIM-D-07)。
pub(crate) fn added_coord(line: &Line, trains: &[Train], coords: &mut Vec<(u64, TrainId)>) -> u64 {
    let p = cycle_len(line);
    coords.clear();
    coords.extend(
        line.trains
            .iter()
            .map(|&t| (cycle_coord(line, &trains[t.index()]), t)),
    );
    coords.sort_unstable();
    let mut best = (0u64, 0u64);
    for (i, &(start, _)) in coords.iter().enumerate() {
        let end = coords.get(i + 1).map_or(coords[0].0 + p, |&(c, _)| c);
        if end - start > best.0 {
            best = (end - start, start);
        }
    }
    (best.1 + best.0 / 2) % p
}

/// 列車を足す: `TrainId = trains.len()`、乗客 0 人の `onboard` の行、`line.trains` への追加。
pub(crate) fn push_train(
    trains: &mut Vec<Train>,
    onboard: &mut PaxMatrix,
    line: &mut Line,
    (section, dir, pos): (u16, Direction, PosUnits),
) -> TrainId {
    let id = TrainId::from_index(trains.len());
    trains.push(Train {
        id,
        line: line.id,
        section,
        dir,
        pos,
        phase: TrainPhase::Running,
        riders: People::ZERO,
    });
    onboard.push_row();
    line.trains.push(id);
    id
}

/// 路線を作ったときの `k` 本目(`0..n`)を置く(11 §5.5)。
pub(crate) fn place_initial_train(st: &mut GameState, l: LineId, k: u32, n: u32) -> TrainId {
    let line = &mut st.lines[l.index()];
    let at = place_at(line, initial_coord(line, k, n));
    push_train(&mut st.trains, &mut st.onboard, line, at)
}

/// R1 で 1 本足す(11 §5.6)。既存の列車は動かさない。
pub(crate) fn place_added_train(
    st: &mut GameState,
    l: LineId,
    coords: &mut Vec<(u64, TrainId)>,
) -> TrainId {
    let line = &mut st.lines[l.index()];
    let at = place_at(line, added_coord(line, &st.trains, coords));
    push_train(&mut st.trains, &mut st.onboard, line, at)
}

#[cfg(test)]
mod tests;
