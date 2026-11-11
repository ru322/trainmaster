use super::*;

fn g(x: i32, y: i32) -> GridPos {
    GridPos { x, y }
}

fn row(y: i32, xs: std::ops::RangeInclusive<i32>) -> Vec<GridPos> {
    xs.map(|x| g(x, y)).collect()
}

#[test]
fn edge_key_is_the_same_from_both_sides() {
    for d in Dir8::ALL {
        let a = g(5, 5);
        let (dx, dy) = d.delta();
        let b = g(5 + dx, 5 + dy);
        assert_eq!(EdgeKey::between(a, b), EdgeKey::between(b, a));
        assert!(matches!(
            EdgeKey::between(a, b).dir,
            Dir8::E | Dir8::SE | Dir8::S | Dir8::SW
        ));
    }
    assert_eq!(
        EdgeKey::between(g(5, 5), g(4, 6)),
        EdgeKey {
            tile: g(5, 5),
            dir: Dir8::SW
        }
    );
}

#[test]
fn lanes_by_line_count() {
    let p = row(3, 1..=4);
    let lanes_for = |n: u32| {
        let paths: Vec<(LineId, &[GridPos])> = (0..n).map(|i| (LineId(i), p.as_slice())).collect();
        let lanes = EdgeLanes::build(&paths);
        (0..n)
            .map(|i| lanes.lane(g(2, 3), g(3, 3), LineId(i)))
            .collect::<Vec<_>>()
    };
    assert_eq!(lanes_for(1), [Some(1)]);
    assert_eq!(lanes_for(2), [Some(0), Some(2)]);
    assert_eq!(lanes_for(3), [Some(0), Some(1), Some(2)]);
    assert_eq!(
        lanes_for(4),
        [Some(0), Some(1), Some(2), None],
        "4 本目は描かない"
    );
}

#[test]
fn lanes_are_per_edge() {
    let a = row(3, 1..=6);
    let b = row(3, 4..=8);
    let paths = [(LineId(0), a.as_slice()), (LineId(1), b.as_slice())];
    let lanes = EdgeLanes::build(&paths);
    assert_eq!(lanes.lane(g(2, 3), g(3, 3), LineId(0)), Some(1));
    assert_eq!(lanes.lane(g(4, 3), g(5, 3), LineId(0)), Some(0));
    assert_eq!(lanes.lane(g(5, 4), g(4, 3), LineId(0)), None);
    assert_eq!(
        lanes.lane(g(5, 3), g(4, 3), LineId(1)),
        Some(2),
        "向きによらない"
    );
}

fn piece(pieces: &[BandPiece], t: GridPos, dir: Dir8, line: LineId) -> BandVariant {
    pieces
        .iter()
        .find(|p| p.tile == t && p.dir == dir && p.line == line)
        .map(|p| p.variant)
        .unwrap_or_else(|| panic!("{t:?} {dir:?} {line:?} がない"))
}

#[test]
fn parallel_when_straight_with_same_lane_else_converge() {
    let a = row(3, 1..=6);
    let b = row(3, 4..=8);
    let paths = [(LineId(0), a.as_slice()), (LineId(1), b.as_slice())];
    let lanes = EdgeLanes::build(&paths);
    let ps = band_pieces(&paths, &lanes);
    assert_eq!(piece(&ps, g(2, 3), Dir8::E, LineId(0)), BandVariant::Center);
    assert_eq!(
        piece(&ps, g(5, 3), Dir8::E, LineId(1)),
        BandVariant::Lane2,
        "共有の中は平行"
    );
    assert_eq!(piece(&ps, g(5, 3), Dir8::W, LineId(0)), BandVariant::Lane0);
    assert_eq!(
        piece(&ps, g(4, 3), Dir8::W, LineId(0)),
        BandVariant::Center,
        "レーンが変わるマスは集める"
    );
    assert_eq!(
        piece(&ps, g(4, 3), Dir8::E, LineId(0)),
        BandVariant::Lane0Conv
    );
    assert_eq!(
        piece(&ps, g(4, 3), Dir8::E, LineId(1)),
        BandVariant::Lane2Conv,
        "端の駅で終わる"
    );
    assert_eq!(
        piece(&ps, g(6, 3), Dir8::W, LineId(0)),
        BandVariant::Lane0Conv
    );
}

#[test]
fn turns_converge() {
    let p = vec![g(1, 1), g(2, 1), g(3, 2), g(3, 3)];
    let q = p.clone();
    let paths = [(LineId(0), p.as_slice()), (LineId(2), q.as_slice())];
    let ps = band_pieces(&paths, &EdgeLanes::build(&paths));
    assert_eq!(
        piece(&ps, g(2, 1), Dir8::SE, LineId(0)),
        BandVariant::Lane0Conv
    );
    assert_eq!(
        piece(&ps, g(3, 2), Dir8::NW, LineId(2)),
        BandVariant::Lane2Conv
    );
    assert_eq!(
        piece(&ps, g(3, 2), Dir8::S, LineId(2)),
        BandVariant::Lane2Conv
    );
}

#[test]
fn pieces_are_in_drawing_order() {
    let a = vec![g(5, 2), g(4, 3), g(3, 3), g(2, 3)];
    let b = row(3, 1..=6);
    let paths = [(LineId(1), a.as_slice()), (LineId(0), b.as_slice())];
    let ps = band_pieces(&paths, &EdgeLanes::build(&paths));
    let keys: Vec<_> = ps
        .iter()
        .map(|p| (p.tile.y, p.tile.x, p.dir, p.line))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "行優先 → Dir8 → LineId");
    assert_eq!(ps.len(), 2 * 3 + 2 * 5);
}

#[test]
fn line_path_joins_segments_once() {
    let seg = |from: u32, to: u32, tiles: Vec<GridPos>| TrackSegment {
        from: tm_core::StationId(from),
        to: tm_core::StationId(to),
        bend: tm_core::Bend::StraightFirst,
        len: tm_core::Tiles(tiles.len() as u32 - 1),
        tiles,
    };
    let segs = [seg(0, 1, row(1, 1..=3)), seg(1, 2, vec![g(3, 1), g(4, 2)])];
    let mut out = Vec::new();
    line_path(&segs, &mut out);
    assert_eq!(out, vec![g(1, 1), g(2, 1), g(3, 1), g(4, 2)]);
}

#[test]
fn train_position_is_integer_pixels() {
    let u = POS_UNITS_PER_TILE as i64;
    let half = TILE / 2;
    let zero = Cam::default();
    assert_eq!(
        train_screen(MapPoint { x: 0, y: 0 }, zero),
        (MAP_X + half, MAP_Y + half)
    );
    assert_eq!(
        train_screen(MapPoint { x: 3 * u, y: 2 * u }, zero),
        tile_center(g(3, 2), zero)
    );
    assert_eq!(
        train_screen(
            MapPoint {
                x: 3 * u + u / 2,
                y: 2 * u
            },
            Cam::at(37, 21)
        ),
        (
            MAP_X + 3 * TILE + half + half - 37,
            MAP_Y + 2 * TILE + half - 21
        )
    );
}

#[test]
fn off_map_covers_the_rest_of_the_layer() {
    assert_eq!(
        off_map(MAP_COLS, MAP_ROWS).count(),
        0,
        "40 × 18 なら塗らない"
    );
    let parts: Vec<Rect> = off_map(32, MAP_ROWS).collect();
    assert_eq!(
        parts,
        [rect(32 * TILE, 0, (8 * TILE) as u32, WORLD_H as u32)],
        "32 × 18 なら右の 8 マス"
    );
    let parts: Vec<Rect> = off_map(32, 16).collect();
    let area: u32 = parts.iter().map(|r| r.w * r.h).sum();
    assert_eq!(
        area as i32,
        crate::layout::WORLD_W * WORLD_H - 32 * 16 * TILE * TILE
    );
}

#[test]
fn split_layers_keep_everything_that_touches_them() {
    let layer = vec![
        DrawCmd::Clear(palette::GROUND_A),
        DrawCmd::FillRect {
            rect: rect(0, 0, TILE as u32, TILE as u32),
            color: palette::GROUND_B,
        },
        DrawCmd::Sprite {
            id: sprite::STN_BODY,
            dst: rect(LAYER_W - 4, 10, 28, 28),
            tint: palette::WHITE,
        },
        DrawCmd::FillRect {
            rect: rect(LAYER_W + 40, 0, TILE as u32, TILE as u32),
            color: palette::GROUND_B,
        },
        DrawCmd::Line {
            x0: LAYER_W - 8,
            y0: 5,
            x1: LAYER_W + 8,
            y1: 5,
            color: palette::BLACK,
        },
    ];
    let a = split_layer(&layer, 0);
    let b = split_layer(&layer, 1);
    assert_eq!(a.len(), 4, "左: Clear、左のマス、境目の駅、境目の線");
    assert_eq!(b.len(), 4, "右: Clear、境目の駅、右のマス、境目の線");
    assert!(
        matches!(b[3], DrawCmd::Line { x0: -8, x1: 8, .. }),
        "境目にかかる線も両方に入れる"
    );
    assert_eq!(a[0], layer[0]);
    assert_eq!(b[0], layer[0], "どの層も不透明の Clear で始める");
    assert!(
        matches!(b[1], DrawCmd::Sprite { dst, .. } if dst == rect(-4, 10, 28, 28)),
        "境目にかかる絵は右の層の座標にずらして両方に入れる"
    );
    assert!(matches!(b[2], DrawCmd::FillRect { rect: r, .. } if r.x == 40));
}

#[test]
fn pulse_stays_in_range() {
    for ms in 0..=2000u64 {
        assert!(pulse(MonoTime(ms * 1000), 1000, 176).r >= 176);
    }
    assert_eq!(pulse(MonoTime(0), 1000, 176).r, 255);
    assert_eq!(pulse(MonoTime(500_000), 1000, 176).r, 176);
    assert_eq!(pulse(MonoTime(1_000_000), 1000, 176).r, 255);
}

#[test]
fn dots_follow_the_share_of_the_station_capacity() {
    assert_eq!(waiting_dots(0, 10), None);
    for n in 1..=10 {
        assert_eq!(waiting_dots(n, 10), Some(sprite::PAX_DOTS[n as usize]));
    }
    assert_eq!(waiting_dots(11, 10), Some(sprite::PAX_DOTS[10]));
    assert_eq!(waiting_dots(1, 1 << 50), Some(sprite::PAX_DOTS[1]));
    assert_eq!(waiting_dots(3 << 47, 1 << 50), Some(sprite::PAX_DOTS[4]));
    assert_eq!(
        waiting_dots((1 << 50) - 1, 1 << 50),
        Some(sprite::PAX_DOTS[10])
    );
    assert_eq!(waiting_dots(u64::MAX, 1 << 50), Some(sprite::PAX_DOTS[10]));
    assert_eq!(waiting_dots(5, 0), Some(sprite::PAX_DOTS[10]));
}

#[test]
fn people_tag_only_above_ten() {
    assert_eq!(waiting_marks(0), (None, false));
    assert_eq!(waiting_marks(1), (Some(sprite::PAX_DOTS[1]), false));
    assert_eq!(waiting_marks(10), (Some(sprite::PAX_DOTS[10]), false));
    assert_eq!(waiting_marks(11), (Some(sprite::PAX_DOTS[10]), true));
}

fn boxes_of(badges: &[(LineId, i32, i32)]) -> Vec<(i32, i32, i32, i32)> {
    badges.iter().map(|&(_, x, y)| badge_box(x, y)).collect()
}

#[test]
fn badges_do_not_overlap_each_other_or_the_dots() {
    let from_west = row(4, 1..=5);
    let from_west2 = row(4, 2..=5);
    let from_east = row(4, 5..=9);
    let diag = vec![g(2, 7), g(3, 6), g(4, 5), g(5, 4)];
    let paths = vec![from_west, from_west2, from_east, diag];
    let badges = badge_positions(&paths);
    assert_eq!(badges.len(), 8);
    let boxes = boxes_of(&badges);
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(!overlaps(*a, *b), "札が重なる {a:?} {b:?}");
        }
    }
    let (cx, cy) = (5 * TILE + TILE / 2, 4 * TILE + TILE / 2);
    let dots = (cx + DOTS_BOX.0, cy + DOTS_BOX.1, DOTS_BOX.2, DOTS_BOX.3);
    for b in &boxes {
        assert!(!overlaps(*b, dots), "札が人の点と重なる {b:?}");
    }
}
