use std::collections::BTreeSet;
use tm_spritegen::def::{Dir, Part};
use tm_spritegen::draw::track_half;
use tm_spritegen::image::Image;
use tm_spritegen::palette::Palette;

const PAL: &str = "Palette(colors: [
    ('.', \"clear\", 0x00000000),
    ('K', \"outline\", 0x000000FF),
    ('W', \"white\", 0xFFFFFFFF),
    ('g', \"rail\", 0x5A5A5AFF),
])";

const OUTLINE: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const RAIL: [u8; 4] = [0x5A, 0x5A, 0x5A, 255];

type Cut = (Image, (i32, i32));

type Pt = (i32, i32);
type Ends = (Dir, Pt, Pt);
type Chain = (Dir, Dir, [Pt; 3]);

const DIRS: [Ends; 8] = [
    (Dir::E, (7, 7), (15, 7)),
    (Dir::W, (7, 7), (-1, 7)),
    (Dir::N, (7, 7), (7, -1)),
    (Dir::S, (7, 7), (7, 15)),
    (Dir::NE, (7, 7), (15, -1)),
    (Dir::SW, (7, 7), (-1, 15)),
    (Dir::NW, (7, 7), (-1, -1)),
    (Dir::SE, (7, 7), (15, 15)),
];

fn shift(dir: Dir, lane: i32) -> (i32, i32) {
    let k = lane - 1;
    match dir {
        Dir::E | Dir::W => (0, 3 * k),
        Dir::N | Dir::S => (3 * k, 0),
        Dir::NE | Dir::SW => (2 * k, 2 * k),
        Dir::NW | Dir::SE => (2 * k, -2 * k),
    }
}

/// 設計の 1px を 1 画素にし、画素の中心だけで描いた半区間(形の検査用)。
fn cut(tile: u32, dir: Dir, part: Part) -> Cut {
    let pal = Palette::parse(PAL).unwrap();
    let (v, pivot) = track_half(tile, dir, part, &pal, "t").unwrap();
    (v.render_samples(1, 1), pivot)
}

fn at(c: &Cut, x: i32, y: i32) -> [u8; 4] {
    c.0.get(x + c.1.0, y + c.1.1)
}

fn band_f(dir: Dir, lane: u8, conv: bool) -> Cut {
    cut(16, dir, Part::BandFill { lane, conv })
}

fn band_o(dir: Dir, lane: u8, conv: bool) -> Cut {
    cut(16, dir, Part::BandOutline { lane, conv })
}

#[test]
fn rail_extents_follow_15_5_8() {
    for (dir, _, _) in DIRS {
        let f = cut(16, dir, Part::RailFill);
        let o = cut(16, dir, Part::RailOutline);
        let (fw, fh, ow, oh) = match dir {
            Dir::E | Dir::W => (12, 4, 14, 6),
            Dir::N | Dir::S => (4, 12, 6, 14),
            _ => (12, 12, 14, 14),
        };
        assert_eq!((f.0.w, f.0.h), (fw, fh), "{dir:?} fill");
        assert_eq!((o.0.w, o.0.h), (ow, oh), "{dir:?} outline");
    }
    let f = cut(16, Dir::E, Part::RailFill);
    assert_eq!(f.1, (-6, -6));
    assert_eq!(at(&f, 6, 6), RAIL);
    assert_eq!(at(&f, 5, 6), [0; 4]);
    assert_eq!(at(&f, 17, 9), RAIL);
    assert_eq!(at(&f, 18, 9), [0; 4]);
    let o = cut(16, Dir::E, Part::RailOutline);
    assert_eq!(o.1, (-5, -5));
    assert_eq!(at(&o, 5, 5), OUTLINE);
    assert_eq!(at(&o, 4, 5), [0; 4]);
    assert_eq!(at(&o, 18, 10), OUTLINE);
    assert_eq!(at(&o, 19, 10), [0; 4]);
}

fn brush(part: Part) -> i32 {
    match part {
        Part::RailOutline => 6,
        Part::RailFill | Part::BandOutline { .. } => 4,
        Part::BandFill { .. } => 2,
    }
}

/// どの半区間の絵も、中心から辺の中点(斜めは角)までの線分をブラシの幅でなぞった形の外接矩形にぴったり収まる。
/// 中心の側も辺の側も、ブラシの半分より先へは出ない(曲がりで線が中心を越えて X にならない)。
#[test]
fn ink_is_the_segment_from_the_center_to_the_edge_widened_by_the_brush() {
    for tile in [16, 20] {
        let h = tile / 2;
        for (dir, s, e) in DIRS {
            let step = ((e.0 - s.0).signum(), (e.1 - s.1).signum());
            let mut parts = vec![(Part::RailOutline, 1, false), (Part::RailFill, 1, false)];
            for lane in 0..3u8 {
                for conv in [false, true] {
                    parts.push((Part::BandOutline { lane, conv }, lane, conv));
                    parts.push((Part::BandFill { lane, conv }, lane, conv));
                }
            }
            for (part, lane, conv) in parts {
                let sh = shift(dir, i32::from(lane));
                let edge = (h + step.0 * h + sh.0, h + step.1 * h + sh.1);
                let center = if conv { (h, h) } else { (h + sh.0, h + sh.1) };
                let b = brush(part) / 2;
                let c = cut(tile as u32, dir, part);
                let want = (
                    center.0.min(edge.0) - b,
                    center.1.min(edge.1) - b,
                    center.0.max(edge.0) + b,
                    center.1.max(edge.1) + b,
                );
                let got = (-c.1.0, -c.1.1, -c.1.0 + c.0.w as i32, -c.1.1 + c.0.h as i32);
                assert_eq!(got, want, "tile {tile} {dir:?} {part:?}");
            }
        }
    }
}

#[test]
fn rail_straight_rows_are_5_to_10() {
    let f = cut(16, Dir::E, Part::RailFill);
    let o = cut(16, Dir::E, Part::RailOutline);
    for y in 0..16 {
        let fill = (6..=9).contains(&y);
        let out = (5..=10).contains(&y);
        assert_eq!(at(&f, 12, y) == RAIL, fill, "fill y={y}");
        assert_eq!(at(&o, 12, y) == OUTLINE, out, "outline y={y}");
    }
}

#[test]
fn band_lane_rows_and_columns_match_the_table() {
    let cases = [(0u8, 4, 3), (1, 7, 6), (2, 10, 9)];
    for (lane, fill0, out0) in cases {
        for (dir, horizontal) in [
            (Dir::E, true),
            (Dir::W, true),
            (Dir::N, false),
            (Dir::S, false),
        ] {
            let f = band_f(dir, lane, false);
            let o = band_o(dir, lane, false);
            for k in 0..16 {
                let (x, y) = if horizontal { (8, k) } else { (k, 8) };
                let in_fill = (fill0..fill0 + 2).contains(&k);
                let in_out = (out0..out0 + 4).contains(&k);
                assert_eq!(at(&f, x, y) == WHITE, in_fill, "{dir:?} l{lane} fill k={k}");
                assert_eq!(
                    at(&o, x, y) == OUTLINE,
                    in_out,
                    "{dir:?} l{lane} outline k={k}"
                );
            }
        }
    }
}

#[test]
fn band_start_and_end_follow_the_rules() {
    for (dir, s, e) in DIRS {
        let step = ((e.0 - s.0).signum(), (e.1 - s.1).signum());
        for lane in 0..3u8 {
            let sh = shift(dir, lane as i32);
            let (s2, e2) = ((s.0 + sh.0, s.1 + sh.1), (e.0 + sh.0, e.1 + sh.1));
            let before = |p: (i32, i32)| (p.0 - 3 * step.0, p.1 - 3 * step.1);
            let after = |p: (i32, i32)| (p.0 + 3 * step.0, p.1 + 3 * step.1);

            let par = band_f(dir, lane, false);
            assert_eq!(at(&par, s2.0, s2.1), WHITE, "{dir:?} l{lane}p start");
            assert_eq!(at(&par, e2.0, e2.1), WHITE, "{dir:?} l{lane}p end");
            assert_eq!(
                at(&par, before(s2).0, before(s2).1),
                [0; 4],
                "{dir:?} l{lane}p before"
            );
            assert_eq!(
                at(&par, after(e2).0, after(e2).1),
                [0; 4],
                "{dir:?} l{lane}p after"
            );

            let conv = band_f(dir, lane, true);
            assert_eq!(at(&conv, s.0, s.1), WHITE, "{dir:?} l{lane}c start");
            assert_eq!(at(&conv, e2.0, e2.1), WHITE, "{dir:?} l{lane}c end");
            assert_eq!(
                at(&conv, before(s).0, before(s).1),
                [0; 4],
                "{dir:?} l{lane}c before"
            );
            assert_eq!(
                at(&conv, after(e2).0, after(e2).1),
                [0; 4],
                "{dir:?} l{lane}c after"
            );
            if lane == 1 {
                assert_eq!(
                    par.0, conv.0,
                    "{dir:?} l1 parallel and converge are the same"
                );
            } else {
                assert_ne!(par.0, conv.0, "{dir:?} l{lane} differ");
            }
        }
    }
}

type Canvas = std::collections::BTreeMap<(i32, i32), [u8; 4]>;

fn paint(c: &mut Canvas, cut: &Cut, tile: (i32, i32)) {
    for y in 0..cut.0.h as i32 {
        for x in 0..cut.0.w as i32 {
            let p = cut.0.get(x, y);
            if p[3] != 0 {
                c.insert((tile.0 * 16 + x - cut.1.0, tile.1 * 16 + y - cut.1.1), p);
            }
        }
    }
}

type Halves = Vec<((i32, i32), Vec<(Dir, u8, bool)>)>;

fn draw_bands(c: &mut Canvas, tiles: &Halves) {
    for (tile, parts) in tiles {
        for (d, l, cv) in parts {
            paint(c, &band_o(*d, *l, *cv), *tile);
        }
    }
    for (tile, parts) in tiles {
        for (d, l, cv) in parts {
            paint(c, &band_f(*d, *l, *cv), *tile);
        }
    }
}

fn column(c: &Canvas, x: i32) -> Vec<(i32, [u8; 4])> {
    c.iter()
        .filter(|((cx, _), _)| *cx == x)
        .map(|((_, y), p)| (*y, *p))
        .collect()
}

fn straight_ew(lanes: &[u8]) -> Canvas {
    let mut c = Canvas::new();
    let mut tiles = Halves::new();
    for t in 0..3 {
        let mut parts = Vec::new();
        for &l in lanes {
            parts.extend([(Dir::E, l, false), (Dir::W, l, false)]);
        }
        tiles.push(((t, 0), parts));
    }
    draw_bands(&mut c, &tiles);
    c
}

#[test]
fn straight_bands_have_continuous_fill_and_outline_across_the_center() {
    for x in 16..32 {
        let single = column(&straight_ew(&[1]), x);
        assert_eq!(
            single,
            vec![(6, OUTLINE), (7, WHITE), (8, WHITE), (9, OUTLINE)],
            "single x={x}"
        );
    }
    for x in 16..32 {
        let triple = column(&straight_ew(&[0, 1, 2]), x);
        let expect: Vec<(i32, [u8; 4])> = (3..=12)
            .map(|y| {
                (
                    y,
                    if matches!(y, 4 | 5 | 7 | 8 | 10 | 11) {
                        WHITE
                    } else {
                        OUTLINE
                    },
                )
            })
            .collect();
        assert_eq!(triple, expect, "triple x={x}");
    }
}

fn diagonal_chain(a: Dir, b: Dir, tiles: [(i32, i32); 3], lanes: &[u8]) -> Canvas {
    let mut c = Canvas::new();
    let mut list = Halves::new();
    for t in tiles {
        let mut parts = Vec::new();
        for &l in lanes {
            parts.extend([(a, l, false), (b, l, false)]);
        }
        list.push((t, parts));
    }
    draw_bands(&mut c, &list);
    c
}

const CHAINS: [Chain; 2] = [
    (Dir::NE, Dir::SW, [(0, 2), (1, 1), (2, 0)]),
    (Dir::NW, Dir::SE, [(0, 0), (1, 1), (2, 2)]),
];

#[test]
fn straight_vertical_and_diagonal_bands_are_unbroken() {
    let mut v = Canvas::new();
    let tiles: Halves = (0..3)
        .map(|t| ((0, t), vec![(Dir::N, 1, false), (Dir::S, 1, false)]))
        .collect();
    draw_bands(&mut v, &tiles);
    for y in 16..32 {
        let row: Vec<_> = v
            .iter()
            .filter(|((_, cy), _)| *cy == y)
            .map(|((x, _), p)| (*x, *p))
            .collect();
        assert_eq!(
            row,
            vec![(6, OUTLINE), (7, WHITE), (8, WHITE), (9, OUTLINE)],
            "y={y}"
        );
    }
    for (a, b, tiles) in CHAINS {
        let c = diagonal_chain(a, b, tiles, &[1]);
        let mut prev: Option<i32> = None;
        for x in 16..32 {
            let fills: Vec<i32> = column(&c, x)
                .iter()
                .filter(|(_, p)| *p == WHITE)
                .map(|(y, _)| *y)
                .collect();
            assert!(!fills.is_empty(), "{a:?} x={x} has no fill");
            assert!(fills.len() <= 3, "{a:?} x={x}");
            if let Some(p) = prev {
                assert!(
                    (fills[0] - p).abs() <= 1,
                    "{a:?} x={x} jumps from {p} to {}",
                    fills[0]
                );
            }
            prev = Some(fills[0]);
        }
    }
}

#[test]
fn three_diagonal_lanes_stay_separate() {
    for (a, b, tiles) in CHAINS {
        let c = diagonal_chain(a, b, tiles, &[0, 1, 2]);
        for x in 18..30 {
            let col = column(&c, x);
            let mut groups = 0;
            let mut last = None;
            for (_, p) in &col {
                if *p == WHITE && last != Some(WHITE) {
                    groups += 1;
                }
                last = Some(*p);
            }
            assert_eq!(groups, 3, "{a:?} x={x}");
            let ys: Vec<i32> = col.iter().map(|(y, _)| *y).collect();
            assert_eq!(
                ys.len() as i32,
                ys[ys.len() - 1] - ys[0] + 1,
                "{a:?} x={x} has a hole"
            );
        }
    }
}

fn components(points: &BTreeSet<(i32, i32)>) -> usize {
    let mut left = points.clone();
    let mut n = 0;
    while let Some(&p) = left.iter().next() {
        n += 1;
        let mut stack = vec![p];
        left.remove(&p);
        while let Some((x, y)) = stack.pop() {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if left.remove(&(x + dx, y + dy)) {
                        stack.push((x + dx, y + dy));
                    }
                }
            }
        }
    }
    n
}

#[test]
fn converging_halves_of_a_bend_touch_at_the_center() {
    for (a, b) in [
        (Dir::E, Dir::N),
        (Dir::W, Dir::N),
        (Dir::E, Dir::S),
        (Dir::W, Dir::S),
        (Dir::E, Dir::NE),
        (Dir::N, Dir::NW),
        (Dir::S, Dir::SE),
        (Dir::W, Dir::SW),
    ] {
        for lane in [0u8, 2] {
            let mut c = Canvas::new();
            draw_bands(
                &mut c,
                &vec![((0, 0), vec![(a, lane, true), (b, lane, true)])],
            );
            let fills: BTreeSet<(i32, i32)> = c
                .iter()
                .filter(|(_, p)| **p == WHITE)
                .map(|(k, _)| *k)
                .collect();
            assert_eq!(components(&fills), 1, "{a:?}+{b:?} l{lane}");
        }
    }
}

#[test]
fn tile_20_uses_the_same_rules() {
    let f = cut(20, Dir::E, Part::RailFill);
    assert_eq!((f.0.w, f.0.h), (14, 4));
    assert_eq!(at(&f, 8, 9), RAIL);
    assert_eq!(at(&f, 21, 9), RAIL);
    let d = cut(20, Dir::NE, Part::RailFill);
    assert_eq!((d.0.w, d.0.h), (14, 14));
    assert_eq!(at(&d, 8, 11), RAIL);
    assert_eq!(at(&d, 21, -2), RAIL);
    let lane0 = cut(
        20,
        Dir::E,
        Part::BandFill {
            lane: 0,
            conv: false,
        },
    );
    assert_eq!(at(&lane0, 12, 6), WHITE);
    let lane2 = cut(
        20,
        Dir::E,
        Part::BandFill {
            lane: 2,
            conv: false,
        },
    );
    assert_eq!(at(&lane2, 12, 12), WHITE);
}

#[test]
fn lane_out_of_range_is_rejected() {
    let pal = Palette::parse(PAL).unwrap();
    let part = Part::BandFill {
        lane: 3,
        conv: false,
    };
    assert!(track_half(16, Dir::E, part, &pal, "t").is_err());
}

fn all_parts() -> Vec<(Part, Part)> {
    let mut v = vec![
        (Part::RailOutline, Part::RailOutline),
        (Part::RailFill, Part::RailFill),
    ];
    for lane in 0..3u8 {
        for conv in [false, true] {
            v.push((
                Part::BandOutline { lane, conv },
                Part::BandOutline {
                    lane: 2 - lane,
                    conv,
                },
            ));
            v.push((
                Part::BandFill { lane, conv },
                Part::BandFill {
                    lane: 2 - lane,
                    conv,
                },
            ));
        }
    }
    v
}

#[test]
fn diagonals_are_mirror_images_of_each_other() {
    for (east, west) in [(Dir::NE, Dir::NW), (Dir::SW, Dir::SE)] {
        for (part, mirrored) in all_parts() {
            let a = cut(16, east, part);
            let b = cut(16, west, mirrored);
            for y in -6..22 {
                for x in -6..22 {
                    assert_eq!(
                        at(&a, x, y),
                        at(&b, 15 - x, y),
                        "{east:?} {part:?} at ({x}, {y})"
                    );
                }
            }
        }
    }
}

#[test]
fn diagonal_lines_lie_on_the_center_diagonals() {
    for dir in [Dir::NE, Dir::SW, Dir::NW, Dir::SE] {
        let fill = cut(
            16,
            dir,
            Part::BandFill {
                lane: 1,
                conv: false,
            },
        );
        let on_line = ink(&fill, (0, 0));
        assert!(!on_line.is_empty());
        for (x, y) in on_line {
            let ok = if [Dir::NE, Dir::SW].contains(&dir) {
                (14..=16).contains(&(x + y))
            } else {
                (-1..=1).contains(&(x - y))
            };
            assert!(ok, "{dir:?} ({x}, {y})");
        }
    }
}

type Scene = Vec<((i32, i32), Vec<(Dir, u8, bool)>)>;

fn bend_scene(lanes: &[u8], bend: (Dir, Dir)) -> Scene {
    let through = |a: Dir, b: Dir| -> Vec<(Dir, u8, bool)> {
        lanes
            .iter()
            .flat_map(|&l| [(a, l, false), (b, l, false)])
            .collect()
    };
    let corner: Vec<(Dir, u8, bool)> = lanes
        .iter()
        .flat_map(|&l| [(bend.0, l, true), (bend.1, l, true)])
        .collect();
    vec![
        ((0, 1), through(Dir::W, Dir::E)),
        ((1, 1), corner),
        ((1, 0), through(Dir::N, Dir::S)),
        ((1, -1), through(Dir::N, Dir::S)),
    ]
}

fn rails_of(scene: &Scene) -> Vec<((i32, i32), Vec<Dir>)> {
    scene
        .iter()
        .map(|(t, parts)| {
            let mut dirs: Vec<Dir> = Vec::new();
            for (d, _, _) in parts {
                if !dirs.contains(d) {
                    dirs.push(*d);
                }
            }
            (*t, dirs)
        })
        .collect()
}

struct Painted {
    canvas: Canvas,
    fill_ink: BTreeSet<(i32, i32)>,
    band_ink: BTreeSet<(i32, i32)>,
    rail_fill_ink: BTreeSet<(i32, i32)>,
}

fn ink(c: &Cut, tile: (i32, i32)) -> BTreeSet<(i32, i32)> {
    let mut out = BTreeSet::new();
    for y in 0..c.0.h as i32 {
        for x in 0..c.0.w as i32 {
            if c.0.get(x, y)[3] != 0 {
                out.insert((tile.0 * 16 + x - c.1.0, tile.1 * 16 + y - c.1.1));
            }
        }
    }
    out
}

fn paint_stage(p: &mut Painted, scene: &Scene, stage: usize, tiles: &[usize]) {
    let rails = rails_of(scene);
    for &i in tiles {
        let (tile, parts) = &scene[i];
        match stage {
            0 | 1 => {
                for d in &rails[i].1 {
                    let part = if stage == 0 {
                        Part::RailOutline
                    } else {
                        Part::RailFill
                    };
                    let c = cut(16, *d, part);
                    if stage == 1 {
                        p.rail_fill_ink.extend(ink(&c, *tile));
                    }
                    paint(&mut p.canvas, &c, *tile);
                }
            }
            2 => {
                for (d, l, cv) in parts {
                    let c = band_o(*d, *l, *cv);
                    p.band_ink.extend(ink(&c, *tile));
                    paint(&mut p.canvas, &c, *tile);
                }
            }
            _ => {
                for (d, l, cv) in parts {
                    let c = band_f(*d, *l, *cv);
                    let i = ink(&c, *tile);
                    p.band_ink.extend(i.iter().copied());
                    p.fill_ink.extend(i);
                    paint(&mut p.canvas, &c, *tile);
                }
            }
        }
    }
}

fn painted(scene: &Scene, four_passes: bool) -> Painted {
    let mut p = Painted {
        canvas: Canvas::new(),
        fill_ink: BTreeSet::new(),
        band_ink: BTreeSet::new(),
        rail_fill_ink: BTreeSet::new(),
    };
    let all: Vec<usize> = (0..scene.len()).collect();
    if four_passes {
        for stage in 0..4 {
            paint_stage(&mut p, scene, stage, &all);
        }
    } else {
        for i in &all {
            for stage in 0..4 {
                paint_stage(&mut p, scene, stage, &[*i]);
            }
        }
    }
    p
}

fn broken_pixels(p: &Painted) -> (usize, usize) {
    let fill_not_white = p
        .fill_ink
        .iter()
        .filter(|k| p.canvas.get(k) != Some(&WHITE))
        .count();
    let rail_black = p
        .rail_fill_ink
        .iter()
        .filter(|k| !p.band_ink.contains(k) && p.canvas.get(k) == Some(&OUTLINE))
        .count();
    (fill_not_white, rail_black)
}

#[test]
fn four_passes_leave_no_black_across_bands_or_seams() {
    for lanes in [vec![0u8, 2], vec![0, 1, 2]] {
        for bend in [(Dir::W, Dir::N), (Dir::E, Dir::N)] {
            let scene = bend_scene(&lanes, bend);
            let good = broken_pixels(&painted(&scene, true));
            assert_eq!(good, (0, 0), "four passes {lanes:?} {bend:?}");
            let bad = broken_pixels(&painted(&scene, false));
            assert!(
                bad.0 > 0,
                "per-tile order must damage band fills {lanes:?} {bend:?}"
            );
            if lanes.len() == 2 {
                assert!(
                    bad.1 > 0,
                    "per-tile order must leave black seams in the rail {bend:?}"
                );
            }
        }
    }
}

#[test]
fn bend_bands_do_not_cross_each_other() {
    for lanes in [vec![0u8, 2], vec![0, 1, 2]] {
        let p = painted(&bend_scene(&lanes, (Dir::W, Dir::N)), true);
        let fills: BTreeSet<(i32, i32)> = p
            .canvas
            .iter()
            .filter(|(_, v)| **v == WHITE)
            .map(|(k, _)| *k)
            .collect();
        assert!(
            components(&fills) <= lanes.len(),
            "{lanes:?}: {}",
            components(&fills)
        );
    }
}

/// 描画の倍率で描いた半区間(設計の 1px = `r` 画素、アンチエイリアスあり)。
fn cut_at(r: u32, dir: Dir, part: Part) -> Cut {
    let pal = Palette::parse(PAL).unwrap();
    let (v, pivot) = track_half(16, dir, part, &pal, "t").unwrap();
    (v.render(r), pivot)
}

/// 6 倍(マップの 1 マス 96 画素)の絵は、形の検査の絵と同じ外接矩形を 6 倍にした大きさで、縦横の縁はくっきり、斜めの縁だけ中間のアルファを持つ。
#[test]
fn native_halves_keep_the_box_and_antialias_only_slanted_edges() {
    for (dir, _, _) in DIRS {
        for part in [
            Part::RailOutline,
            Part::RailFill,
            Part::BandFill {
                lane: 0,
                conv: true,
            },
        ] {
            let one = cut(16, dir, part);
            let six = cut_at(6, dir, part);
            assert_eq!(
                (six.0.w, six.0.h),
                (one.0.w * 6, one.0.h * 6),
                "{dir:?} {part:?}"
            );
            assert_eq!(six.1, one.1);
            let partial = six.0.px.iter().filter(|p| p[3] != 0 && p[3] != 255).count();
            let slanted = !matches!(dir, Dir::E | Dir::W | Dir::N | Dir::S)
                || matches!(part, Part::BandFill { conv: true, .. });
            assert_eq!(partial > 0, slanted, "{dir:?} {part:?}: {partial}");
        }
    }
}

/// 斜めにつないだ 3 マスを 6 倍で重ねると、帯の中身の芯(線の上)は継ぎ目でも欠けずに不透明のまま。
#[test]
fn native_diagonal_chain_has_no_seam_at_the_corners() {
    let mut c: std::collections::BTreeMap<(i32, i32), u8> = Default::default();
    for (k, (dir_a, dir_b)) in [(Dir::NE, Dir::SW); 3].into_iter().enumerate() {
        let tile = (k as i32, -(k as i32));
        for d in [dir_a, dir_b] {
            let h = cut_at(
                6,
                d,
                Part::BandFill {
                    lane: 1,
                    conv: false,
                },
            );
            for y in 0..h.0.h as i32 {
                for x in 0..h.0.w as i32 {
                    let a = h.0.get(x, y)[3];
                    let key = (tile.0 * 96 + x - h.1.0 * 6, tile.1 * 96 + y - h.1.1 * 6);
                    let e = c.entry(key).or_insert(0);
                    *e = (*e).max(a);
                }
            }
        }
    }
    for t in 48..(96 * 2 + 48) {
        assert_eq!(c.get(&(t, 96 - t)).copied(), Some(255), "芯 t={t}");
        assert_eq!(c.get(&(t, 96 - t + 4)).copied(), Some(255), "芯の脇 t={t}");
    }
}

/// 1 本の帯の縁(黒)は、同じ太さの線路の中身(灰色)を縁のアンチエイリアスまで覆う(斜めの縁に灰色の筋が出ない)。
/// 赤い地に線路の縁 → 線路の中身 → 帯の縁の順に 6 倍で重ね、灰色が混ざると緑と青が 0 でなくなる。
#[test]
fn band_outline_hides_the_rail_fill_edges_on_diagonals() {
    for dir in [Dir::NE, Dir::NW, Dir::SW, Dir::SE, Dir::E, Dir::N] {
        let parts = [
            cut_at(6, dir, Part::RailOutline),
            cut_at(6, dir, Part::RailFill),
            cut_at(
                6,
                dir,
                Part::BandOutline {
                    lane: 1,
                    conv: false,
                },
            ),
        ];
        let mut c: std::collections::BTreeMap<(i32, i32), [u32; 3]> = Default::default();
        for (img, pivot) in &parts {
            for y in 0..img.h as i32 {
                for x in 0..img.w as i32 {
                    let p = img.get(x, y);
                    let key = (x - pivot.0 * 6, y - pivot.1 * 6);
                    let d = c.entry(key).or_insert([255, 0, 0]);
                    let a = u32::from(p[3]);
                    for k in 0..3 {
                        d[k] = (u32::from(p[k]) * a + d[k] * (255 - a)) / 255;
                    }
                }
            }
        }
        let gray = c.values().filter(|v| v[1] > 2 || v[2] > 2).count();
        assert_eq!(gray, 0, "{dir:?}: 灰色の混ざる画素 {gray}");
    }
}
