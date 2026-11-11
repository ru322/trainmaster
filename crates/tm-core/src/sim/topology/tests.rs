use proptest::collection::vec;
use proptest::prelude::*;

use super::fixtures::*;
use super::*;
use crate::geom::GridPos;
use crate::ids::{CompanyLevel, Level};
use crate::sim::movement::{arrival_index, departure_leg};

use Direction::{Backward, Forward};

fn sec(from: u16, to: u16, tiles: u32) -> Section {
    Section {
        from,
        to,
        tiles: Tiles(tiles),
        length: PosUnits(u64::from(tiles) * TILE),
    }
}

/// S0(pl0) S1(pl1) S2(pl0) S3(pl1) S4(pl1) を x 方向に 3・4・3・2 マスの間隔で。
fn row_with_pass_stations() -> Vec<Station> {
    stations(&[(2, 5, 0), (5, 5, 1), (9, 5, 0), (12, 5, 1), (14, 5, 1)])
}

#[test]
fn rebuild_all_stops() {
    let st = row_with_pass_stations();
    let l = line(0, &[0, 1, 2, 3, 4], &st, 0);
    assert_eq!(l.stops, [true; 5]);
    assert_eq!(l.offsets, [0, 3, 7, 10, 12]);
    assert_eq!(
        l.sections,
        [sec(0, 1, 3), sec(1, 2, 4), sec(2, 3, 3), sec(3, 4, 2)]
    );
}

#[test]
fn rebuild_with_pass_stations_at_end_and_middle() {
    let st = row_with_pass_stations();
    let l = line(0, &[0, 1, 2, 3, 4], &st, 2);
    assert_eq!(l.stops, [false, true, false, true, true]);
    assert_eq!(l.offsets, [0, 3, 7, 10, 12]);
    assert_eq!(l.sections, [sec(1, 3, 7), sec(3, 4, 2)]);

    let l = line(1, &[4, 3, 2, 1, 0], &st, 2);
    assert_eq!(l.stops, [true, true, false, true, false]);
    assert_eq!(l.offsets, [0, 2, 5, 9, 12]);
    assert_eq!(l.sections, [sec(0, 1, 2), sec(1, 3, 7)]);
}

#[test]
fn rebuild_is_idempotent_and_matches_inv17() {
    let st = row_with_pass_stations();
    let mut l = line(0, &[0, 1, 2, 3, 4], &st, 2);
    let before = l.clone();
    rebuild_line_derived(&mut l, &st, &bal());
    assert_eq!(l, before);
    let mut acc = 0;
    for (i, off) in l.offsets.iter().enumerate() {
        assert_eq!(*off, acc);
        if let Some(s) = l.segments.get(i) {
            acc += s.len.0;
        }
    }
}

fn gp(x: i32, y: i32) -> GridPos {
    GridPos { x, y }
}

#[test]
fn add_line_tracks_ors_shared_and_crossing_cells() {
    let st = stations(&[
        (2, 2, 0),
        (8, 2, 0),
        (5, 0, 0),
        (5, 6, 0),
        (3, 2, 0),
        (7, 2, 0),
    ]);
    let lines = [
        line(0, &[0, 1], &st, 0),
        line(1, &[2, 3], &st, 0),
        line(2, &[4, 5], &st, 0),
    ];
    let (w, h) = map_size();
    let mut grid = TrackGrid::new(w, h);
    for l in lines.iter().rev() {
        for s in &l.segments {
            add_line_tracks(&mut grid, &s.tiles);
        }
    }
    assert_eq!(grid, tracks_from_lines(w, h, &lines));
    let cross = grid.get(gp(5, 2));
    for d in [Dir8::E, Dir8::W, Dir8::N, Dir8::S] {
        assert!(cross.contains(d), "{d:?}");
    }
    assert!(!cross.contains(Dir8::NE));
    let end = grid.get(gp(2, 2));
    assert!(end.contains(Dir8::E) && !end.contains(Dir8::W));
    let shared_end = grid.get(gp(3, 2));
    assert!(shared_end.contains(Dir8::E) && shared_end.contains(Dir8::W));
    assert!(grid.get(gp(9, 2)).is_empty());
}

#[test]
fn add_line_tracks_diagonal_bits() {
    let (w, h) = map_size();
    let mut grid = TrackGrid::new(w, h);
    add_line_tracks(&mut grid, &[gp(1, 3), gp(2, 2), gp(3, 2)]);
    assert!(grid.get(gp(1, 3)).contains(Dir8::NE));
    let mid = grid.get(gp(2, 2));
    assert!(mid.contains(Dir8::SW) && mid.contains(Dir8::E));
    assert!(grid.get(gp(3, 2)).contains(Dir8::W));
}

/// INV-04: `segments[i]` は `stations[i]` → `stations[i+1]` の向きで、保存した曲がり方で敷き直したマスと一致する。
fn check_inv04(l: &Line, st: &[Station]) {
    assert_eq!(l.segments.len(), l.stations.len() - 1);
    for (i, s) in l.segments.iter().enumerate() {
        assert_eq!((s.from, s.to), (l.stations[i], l.stations[i + 1]));
        let mut tiles = Vec::new();
        lay_track(
            st[s.from.index()].pos,
            st[s.to.index()].pos,
            s.bend,
            &mut tiles,
        );
        assert_eq!(s.tiles, tiles);
        assert_eq!(
            s.len.0,
            st[s.from.index()].pos.chebyshev(st[s.to.index()].pos)
        );
    }
}

#[test]
fn new_line_keeps_bends_as_given() {
    let st = stations(&[(10, 8, 0), (16, 10, 0), (13, 14, 0)]);
    let l = new_line(
        LineId(3),
        &ids(&[0, 1, 2]),
        &[Bend::DiagonalFirst, Bend::StraightFirst],
        &st,
        &bal(),
        (Tick(77), CompanyLevel(5)),
    );
    assert_eq!(l.segments[0].bend, Bend::DiagonalFirst);
    assert_eq!(l.segments[1].bend, Bend::StraightFirst);
    assert_eq!(
        (l.id, l.created_at, l.born_level),
        (LineId(3), Tick(77), CompanyLevel(5))
    );
    assert!(l.trains.is_empty());
    check_inv04(&l, &st);
}

#[test]
fn extend_head_stores_reversed_bend_and_tail_keeps_it() {
    let st = stations(&[(10, 8, 0), (16, 8, 0), (4, 5, 0), (20, 12, 0)]);
    let b = bal();
    let mut l = line(0, &[0, 1], &st, 0);

    let k = extend_shape(
        &mut l,
        &st,
        LineEnd::Head,
        StationId(2),
        Bend::DiagonalFirst,
    );
    assert_eq!(k, 0);
    assert_eq!(l.stations, ids(&[2, 0, 1]));
    assert_eq!(l.segments[0].bend, Bend::StraightFirst);
    let mut preview = Vec::new();
    lay_track(st[0].pos, st[2].pos, Bend::DiagonalFirst, &mut preview);
    preview.reverse();
    assert_eq!(l.segments[0].tiles, preview, "完成図と同じマス(10 TD-23)");

    let k = extend_shape(
        &mut l,
        &st,
        LineEnd::Tail,
        StationId(3),
        Bend::DiagonalFirst,
    );
    assert_eq!(k, 2);
    assert_eq!(l.stations, ids(&[2, 0, 1, 3]));
    assert_eq!(l.segments[2].bend, Bend::DiagonalFirst);
    lay_track(st[1].pos, st[3].pos, Bend::DiagonalFirst, &mut preview);
    assert_eq!(l.segments[2].tiles, preview);

    rebuild_line_derived(&mut l, &st, &b);
    check_inv04(&l, &st);
    assert_eq!(l.offsets, [0, 6, 12, 16]);
    assert_eq!(l.sections.len(), 3);
}

#[test]
fn serving_lines_stay_sorted() {
    let mut v = Vec::new();
    for l in [3, 1, 2, 1, 0] {
        insert_serving_line(&mut v, LineId(l));
    }
    assert_eq!(v, [LineId(0), LineId(1), LineId(2), LineId(3)]);
}

#[test]
fn station_index_and_stop_lookup() {
    let st = row_with_pass_stations();
    let l = line(0, &[0, 1, 2, 3, 4], &st, 2);
    assert_eq!(station_index(&l, StationId(3)), 3);
    assert_eq!(find_stop_at(&l, 3 * TILE), Some(1));
    assert_eq!(find_stop_at(&l, 0), None, "通過駅には重ならない");
    assert_eq!(find_stop_at(&l, 3 * TILE + 1), None);
    assert_eq!(section_arriving_at(&l, 3, Forward), 0);
    assert_eq!(section_arriving_at(&l, 3, Backward), 1);
}

fn dwell(at: u16, remaining: u32) -> TrainPhase {
    TrainPhase::Dwelling {
        at,
        remaining: Ticks(remaining),
    }
}

/// S0 —(4)— S1 —(6)— S2。S1 は最初は通過駅で、ホーム長の強化で停車駅になる(区間の分割)。
#[test]
fn relocate_on_section_split() {
    let b = bal();
    let mut st = stations(&[(2, 5, 1), (6, 5, 0), (12, 5, 1)]);
    let mut l = line(0, &[0, 1, 2], &st, 2);
    assert_eq!(l.sections, [sec(0, 2, 10)]);
    let run = TrainPhase::Running;
    let mut trains = vec![
        train(0, &l, 0, Forward, 100_000, run),
        train(1, &l, 0, Forward, 240_000, run),
        train(2, &l, 0, Forward, 300_000, run),
        train(3, &l, 0, Backward, 100_000, run),
        train(4, &l, 0, Backward, 360_000, run),
        train(5, &l, 0, Forward, 600_000, run),
        train(6, &l, 0, Backward, 600_000, run),
        train(7, &l, 0, Forward, 0, dwell(0, 30)),
        train(8, &l, 0, Backward, 0, dwell(2, 5)),
        train(9, &l, 0, Backward, 359_999, run),
        train(10, &l, 0, Forward, 239_999, run),
    ];
    l.trains = trains.iter().map(|t| t.id).collect();
    let before: Vec<i64> = trains.iter().map(|t| line_x(&l, t, StationId(0))).collect();

    let mut anchors = Vec::new();
    snapshot_anchors(&l, &trains, &mut anchors);
    assert_eq!(anchors[5].kind, AnchorKind::Arriving);
    assert_eq!(
        anchors[7].kind,
        AnchorKind::Dwelling {
            remaining: Ticks(30)
        }
    );
    assert_eq!(
        (anchors[3].dep_stop, anchors[3].arr_stop),
        (StationId(2), StationId(0))
    );
    st[1].levels.platform = Level(1);
    rebuild_line_derived(&mut l, &st, &b);
    assert_eq!(l.sections, [sec(0, 1, 4), sec(1, 2, 6)]);
    relocate_trains(&l, &anchors, &mut trains);

    let got: Vec<(u16, Direction, u64, TrainPhase)> = trains
        .iter()
        .map(|t| (t.section, t.dir, t.pos.0, t.phase))
        .collect();
    assert_eq!(
        got,
        [
            (0, Forward, 100_000, run),
            (1, Forward, 0, run),
            (1, Forward, 60_000, run),
            (1, Backward, 100_000, run),
            (0, Backward, 0, run),
            (1, Forward, 360_000, run),
            (0, Backward, 240_000, run),
            (0, Forward, 0, dwell(0, 30)),
            (1, Backward, 0, dwell(2, 5)),
            (1, Backward, 359_999, run),
            (0, Forward, 239_999, run),
        ]
    );
    for (i, t) in trains.iter().enumerate() {
        assert_eq!(line_x(&l, t, StationId(0)), before[i], "T{i} の位置");
    }
    assert_eq!(
        l.stations[arrival_index(&l, 0, Forward)],
        StationId(1),
        "前半の区間から出発"
    );
}

/// 先頭への延伸: 添字が 1 つずれても位置は変わらない。先頭で停車中の列車は決めたとおり折り返す。
#[test]
fn relocate_on_head_extension() {
    let b = bal();
    let st = stations(&[(10, 8, 0), (14, 8, 0), (20, 8, 0), (5, 8, 0)]);
    let mut l = line(0, &[0, 1, 2], &st, 0);
    let run = TrainPhase::Running;
    let mut trains = vec![
        train(0, &l, 0, Forward, 100_000, run),
        train(1, &l, 1, Backward, 50_000, run),
        train(2, &l, 0, Forward, 0, dwell(0, 10)),
        train(3, &l, 0, Backward, 200_000, run),
        train(4, &l, 0, Backward, 240_000, run),
    ];
    l.trains = trains.iter().map(|t| t.id).collect();
    let before: Vec<i64> = trains.iter().map(|t| line_x(&l, t, StationId(0))).collect();
    let mut anchors = Vec::new();
    snapshot_anchors(&l, &trains, &mut anchors);
    extend_shape(
        &mut l,
        &st,
        LineEnd::Head,
        StationId(3),
        Bend::StraightFirst,
    );
    rebuild_line_derived(&mut l, &st, &b);
    relocate_trains(&l, &anchors, &mut trains);

    let got: Vec<(u16, Direction, u64, TrainPhase)> = trains
        .iter()
        .map(|t| (t.section, t.dir, t.pos.0, t.phase))
        .collect();
    assert_eq!(
        got,
        [
            (1, Forward, 100_000, run),
            (2, Backward, 50_000, run),
            (1, Forward, 0, dwell(1, 10)),
            (1, Backward, 200_000, run),
            (1, Backward, 240_000, run),
        ]
    );
    for (i, t) in trains.iter().enumerate() {
        assert_eq!(line_x(&l, t, StationId(0)), before[i], "T{i} の位置");
    }
    assert_eq!(
        departure_leg(&l, 1, Backward),
        (0, Backward),
        "旧先頭に着いた列車は新しい先頭へ進む"
    );
}

/// 末尾への延伸: 末尾で停車中(折り返し済み)の列車はこの回は折り返す。走っている列車は新しい末尾まで進む。
#[test]
fn relocate_on_tail_extension() {
    let b = bal();
    let st = stations(&[(10, 8, 0), (14, 8, 0), (20, 8, 0), (25, 10, 0)]);
    let mut l = line(0, &[0, 1, 2], &st, 0);
    let mut trains = vec![
        train(0, &l, 1, Backward, 0, dwell(2, 59)),
        train(1, &l, 1, Forward, 10, TrainPhase::Running),
        train(2, &l, 1, Forward, 360_000, TrainPhase::Running),
    ];
    l.trains = trains.iter().map(|t| t.id).collect();
    let mut anchors = Vec::new();
    snapshot_anchors(&l, &trains, &mut anchors);
    let old = trains.clone();
    extend_shape(
        &mut l,
        &st,
        LineEnd::Tail,
        StationId(3),
        Bend::DiagonalFirst,
    );
    rebuild_line_derived(&mut l, &st, &b);
    relocate_trains(&l, &anchors, &mut trains);
    assert_eq!(trains, old, "末尾への延伸では添字も位置も変わらない");
    assert_eq!(departure_leg(&l, 1, Forward), (2, Forward));
    assert_eq!(l.stations[arrival_index(&l, 1, Backward)], StationId(1));
}

fn map_size() -> (u8, u8) {
    let b = bal();
    (b.spec().map.width, b.spec().map.height)
}

fn cells(n: usize) -> impl Strategy<Value = Vec<(i32, i32)>> {
    let (w, h) = map_size();
    let (w, h) = (i32::from(w), i32::from(h));
    proptest::sample::subsequence((0..w * h).collect::<Vec<i32>>(), n)
        .prop_shuffle()
        .prop_map(move |v| v.into_iter().map(|c| (c % w, c / w)).collect())
}

fn config() -> ProptestConfig {
    let mut c = ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    if matches!(c.rng_seed, proptest::test_runner::RngSeed::Random) {
        c.rng_seed = proptest::test_runner::RngSeed::Fixed(1);
    }
    c
}

#[derive(Clone, Debug)]
struct Case {
    cells: Vec<(i32, i32)>,
    platforms: Vec<u8>,
    cars: u8,
    op: u8,
    pick: usize,
    bend: bool,
    trains: Vec<(u16, bool, u8, u32)>,
}

fn case() -> impl Strategy<Value = Case> {
    (2usize..=6).prop_flat_map(|k| {
        (
            cells(k + 1),
            vec(0u8..=3, k + 1),
            0u8..=2,
            0u8..3,
            any::<usize>(),
            any::<bool>(),
            vec((any::<u16>(), any::<bool>(), 0u8..3, any::<u32>()), 1..6),
        )
            .prop_map(|(cells, platforms, cars, op, pick, bend, trains)| Case {
                cells,
                platforms,
                cars,
                op,
                pick,
                bend,
                trains,
            })
    })
}

proptest! {
    #![proptest_config(config())]

    /// 11 §11.2 の移し替えの保存: 延伸・停車駅の追加の前後で、走行中の列車の位置と向きが変わらない。
    #[test]
    fn relocation_preserves_positions(c in case()) {
        let b = bal();
        let k = c.cells.len() - 1;
        let mut cells: Vec<(i32, i32, u8)> = c.cells.iter().zip(&c.platforms)
            .map(|(&(x, y), &p)| (x, y, p)).collect();
        cells[0].2 = cells[0].2.max(c.cars);
        cells[k - 1].2 = cells[k - 1].2.max(c.cars);
        let mut st = stations(&cells);
        let order: Vec<u32> = (0..k as u32).collect();
        let mut l = line(0, &order, &st, c.cars);
        let mut trains = Vec::new();
        for (i, &(s, fwd, kind, r)) in c.trains.iter().enumerate() {
            let k = s % l.sections.len() as u16;
            let dir = if fwd { Forward } else { Backward };
            let len = l.sections[usize::from(k)].length.0;
            let (pos, phase) = match kind {
                0 => (u64::from(r) % len, TrainPhase::Running),
                1 => (len, TrainPhase::Running),
                _ => {
                    let s = &l.sections[usize::from(k)];
                    let at = if fwd { s.from } else { s.to };
                    (0, dwell(at, r % 600))
                }
            };
            trains.push(train(i as u32, &l, k, dir, pos, phase));
        }
        l.trains = trains.iter().map(|t| t.id).collect();
        let origin = StationId(0);
        let before = trains.clone();
        let old_line = l.clone();
        let mut anchors = Vec::new();
        snapshot_anchors(&l, &trains, &mut anchors);
        let bend = if c.bend { Bend::DiagonalFirst } else { Bend::StraightFirst };
        match c.op {
            0 => { extend_shape(&mut l, &st, LineEnd::Head, StationId(k as u32), bend); }
            1 => { extend_shape(&mut l, &st, LineEnd::Tail, StationId(k as u32), bend); }
            _ => {
                let pass: Vec<usize> = (0..k).filter(|&i| !l.stops[i]).collect();
                if !pass.is_empty() {
                    st[pass[c.pick % pass.len()]].levels.platform = Level(8);
                }
            }
        }
        rebuild_line_derived(&mut l, &st, &b);
        relocate_trains(&l, &anchors, &mut trains);
        check_inv04(&l, &st);
        for (old, new) in before.iter().zip(&trains) {
            let s = &l.sections[usize::from(new.section)];
            prop_assert_eq!(old.dir, new.dir);
            match old.phase {
                TrainPhase::Dwelling { at, remaining } => {
                    let TrainPhase::Dwelling { at: new_at, remaining: r } = new.phase else {
                        panic!("停車中でなくなった {new:?}");
                    };
                    prop_assert_eq!(r, remaining);
                    prop_assert_eq!(l.stations[usize::from(new_at)], old_line.stations[usize::from(at)]);
                    prop_assert_eq!(new.pos, PosUnits(0));
                    prop_assert_eq!(new_at, if new.dir == Forward { s.from } else { s.to });
                }
                TrainPhase::Running if old.pos == old_line.sections[usize::from(old.section)].length => {
                    prop_assert_eq!(new.phase, TrainPhase::Running);
                    prop_assert_eq!(new.pos, s.length);
                    prop_assert_eq!(
                        l.stations[arrival_index(&l, new.section, new.dir)],
                        old_line.stations[arrival_index(&old_line, old.section, old.dir)]
                    );
                }
                TrainPhase::Running => {
                    prop_assert_eq!(new.phase, TrainPhase::Running);
                    prop_assert!(new.pos < s.length);
                    prop_assert_eq!(line_x(&l, new, origin), line_x(&old_line, old, origin));
                }
            }
        }
    }
}
