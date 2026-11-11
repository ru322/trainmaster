#![cfg(feature = "headless")]

mod common;

use common::{bundle, leak, rect};
use tm_platform_api::{
    AssetBundle, AtlasImage, Color, DrawCmd, DrawList, LayerId, Rect, SoftRaster, SpriteId,
};

const BG: Color = rgba(10, 20, 30, 255);
const STRIP: [[u8; 4]; 5] = [
    [200, 10, 10, 255],
    [10, 200, 10, 255],
    [10, 10, 200, 255],
    [200, 200, 10, 255],
    [10, 200, 200, 255],
];

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

/// 白 2×2、5×1 の色の帯、半透明の 1 画素(14 §9.4 の比較で SDL から得た値と同じ束)。
fn assets() -> AssetBundle {
    let mut px = vec![0u8; 8 * 2 * 4];
    for i in [0, 1, 8, 9] {
        px[i * 4..i * 4 + 4].copy_from_slice(&[255; 4]);
    }
    for (i, c) in STRIP.iter().enumerate() {
        px[(2 + i) * 4..(3 + i) * 4].copy_from_slice(c);
    }
    px[7 * 4..8 * 4].copy_from_slice(&[200, 100, 50, 128]);
    bundle(
        vec![AtlasImage {
            width: 8,
            height: 2,
            rgba: leak(px),
        }],
        vec![
            rect(0, 0, 0, 2, 2),
            rect(0, 2, 0, 5, 1),
            rect(0, 7, 0, 1, 1),
        ],
        vec![],
    )
}

fn draw(cmds: Vec<DrawCmd>) -> SoftRaster {
    let mut s = SoftRaster::new();
    s.draw(&assets(), &DrawList { cmds });
    s
}

fn px(s: &SoftRaster, x: usize, y: usize) -> [u8; 4] {
    let i = (y * 640 + x) * 4;
    s.pixels()[i..i + 4].try_into().unwrap()
}

fn fill(x: i32, y: i32, w: u32, h: u32, color: Color) -> DrawCmd {
    DrawCmd::FillRect {
        rect: r(x, y, w, h),
        color,
    }
}

fn sprite(id: u16, dst: Rect, tint: Color) -> DrawCmd {
    DrawCmd::Sprite {
        id: SpriteId(id),
        dst,
        tint,
    }
}

fn begin(layer: u8, x: i32, y: i32, w: u32, h: u32) -> DrawCmd {
    DrawCmd::BeginCachedLayer {
        layer: LayerId(layer),
        revision: 1,
        x,
        y,
        w,
        h,
    }
}

fn draw_layer(layer: u8) -> DrawCmd {
    DrawCmd::DrawCachedLayer {
        layer: LayerId(layer),
    }
}

fn is(s: &SoftRaster, x: usize, y: usize, c: Color) -> bool {
    px(s, x, y) == [c.r, c.g, c.b, c.a]
}

#[test]
fn starts_opaque_black_and_clear_replaces_rgba() {
    let s = SoftRaster::new();
    assert_eq!(s.pixels().len(), 640 * 360 * 4);
    assert!(s.pixels().chunks(4).all(|p| p == [0, 0, 0, 255]));
    let s = draw(vec![DrawCmd::Clear(rgba(1, 2, 3, 4))]);
    assert!(s.pixels().chunks(4).all(|p| p == [1, 2, 3, 4]));
    let mut again = s.clone();
    again.draw(&assets(), &DrawList::default());
    assert_eq!(again.pixels(), SoftRaster::default().pixels());
}

#[test]
fn fill_blends_like_sdl_software() {
    let mut cmds = vec![DrawCmd::Clear(BG)];
    for (i, a) in [1u8, 50, 128, 200, 254].into_iter().enumerate() {
        cmds.push(fill(i as i32 * 4, 0, 4, 4, rgba(200, 101, 255, a)));
    }
    cmds.push(fill(50, 50, 3, 3, rgba(255, 0, 0, 128)));
    cmds.push(fill(51, 51, 3, 3, rgba(0, 0, 255, 128)));
    let s = draw(cmds);
    let want = [
        [9, 19, 30, 255],
        [47, 35, 74, 255],
        [104, 59, 142, 255],
        [158, 83, 206, 255],
        [199, 100, 254, 255],
    ];
    for (i, w) in want.into_iter().enumerate() {
        assert_eq!(px(&s, i * 4 + 3, 3), w, "{i}");
    }
    assert_eq!(px(&s, 20, 0), [10, 20, 30, 255]);
    assert_eq!(px(&s, 52, 52), [65, 4, 134, 255]);
    assert_eq!(px(&s, 50, 50), [132, 9, 14, 255]);
}

#[test]
fn fill_is_clipped_and_empty_rects_draw_nothing() {
    let red = rgba(255, 0, 0, 255);
    let s = draw(vec![
        fill(-5, -5, 7, 7, red),
        fill(636, 356, 10, 10, red),
        fill(100, 100, 0, 9, red),
        fill(110, 100, 9, 0, red),
        fill(i32::MIN, 0, u32::MAX, 1, red),
    ]);
    assert!(is(&s, 0, 0, red) && is(&s, 1, 1, red) && !is(&s, 2, 2, red));
    assert!(is(&s, 639, 359, red) && is(&s, 636, 356, red));
    assert!(!is(&s, 100, 100, red) && !is(&s, 110, 100, red));
    let lit = s.pixels().chunks(4).filter(|p| p[0] == 255).count();
    assert_eq!(lit, 4 + 16 + 640 - 2);
}

#[test]
fn lines_include_both_ends_in_eight_directions() {
    let green = rgba(0, 255, 0, 255);
    for (dx, dy) in [
        (9, 0),
        (9, 4),
        (6, 6),
        (4, 9),
        (0, 9),
        (-4, 9),
        (-6, 6),
        (-9, -4),
    ] {
        let s = draw(vec![DrawCmd::Line {
            x0: 100,
            y0: 100,
            x1: 100 + dx,
            y1: 100 + dy,
            color: green,
        }]);
        let lit = s.pixels().chunks(4).filter(|p| p[1] == 255).count() as i32;
        assert_eq!(lit, dx.abs().max(dy.abs()) + 1, "{dx},{dy}");
        assert!(is(&s, 100, 100, green), "{dx},{dy}");
        let (ex, ey) = ((100 + dx) as usize, (100 + dy) as usize);
        assert!(is(&s, ex, ey, green), "{dx},{dy}");
    }
    let s = draw(vec![DrawCmd::Line {
        x0: 0,
        y0: 0,
        x1: 7,
        y1: 2,
        color: green,
    }]);
    for (x, y) in [(0, 0), (1, 0), (2, 1), (5, 1), (6, 2), (7, 2)] {
        assert!(is(&s, x, y, green), "{x},{y}");
    }
}

#[test]
fn sprite_is_scaled_nearest_from_pixel_centers() {
    let white = rgba(255, 255, 255, 255);
    let cmds = (1..=13)
        .map(|w| sprite(1, r(0, w as i32 * 2, w, 1), white))
        .collect();
    let s = draw(cmds);
    let want: [&[usize]; 13] = [
        &[2],
        &[1, 3],
        &[0, 2, 4],
        &[0, 1, 3, 4],
        &[0, 1, 2, 3, 4],
        &[0, 1, 2, 2, 3, 4],
        &[0, 1, 1, 2, 3, 3, 4],
        &[0, 0, 1, 2, 2, 3, 4, 4],
        &[0, 0, 1, 1, 2, 3, 3, 4, 4],
        &[0, 0, 1, 1, 2, 2, 3, 3, 4, 4],
        &[0, 0, 1, 1, 2, 2, 2, 3, 3, 4, 4],
        &[0, 0, 1, 1, 1, 2, 2, 3, 3, 3, 4, 4],
        &[0, 0, 0, 1, 1, 2, 2, 2, 3, 3, 4, 4, 4],
    ];
    for (k, row) in want.iter().enumerate() {
        let y = (k + 1) * 2;
        for (x, &i) in row.iter().enumerate() {
            assert_eq!(px(&s, x, y), STRIP[i], "w={} x={x}", k + 1);
        }
        assert_eq!(px(&s, row.len(), y), [0, 0, 0, 255]);
    }
    let s = draw(vec![sprite(1, r(0, 0, 5, 3), white)]);
    assert_eq!(px(&s, 4, 2), STRIP[4]);
    assert_eq!(px(&s, 4, 3), [0, 0, 0, 255]);
}

#[test]
fn sprite_tint_and_alpha_like_sdl_software() {
    let mut cmds = vec![DrawCmd::Clear(BG)];
    for (i, a) in [50u8, 254, 255].into_iter().enumerate() {
        let x = i as i32 * 4;
        cmds.push(sprite(2, r(x, 10, 4, 4), rgba(255, 255, 255, a)));
        cmds.push(sprite(1, r(x, 20, 4, 4), rgba(77, 177, 255, a)));
    }
    cmds.push(sprite(2, r(40, 30, 2, 2), rgba(100, 255, 3, 255)));
    cmds.push(sprite(9, r(50, 30, 2, 2), rgba(255, 255, 255, 255)));
    cmds.push(sprite(1, r(60, 30, 0, 2), rgba(255, 255, 255, 255)));
    let s = draw(cmds);
    assert_eq!(px(&s, 0, 10), [28, 27, 31, 255]);
    assert_eq!(px(&s, 4, 10), [104, 59, 39, 255]);
    assert_eq!(px(&s, 8, 10), [104, 59, 39, 255]);
    assert_eq!(px(&s, 0, 20), [19, 17, 25, 255]);
    assert_eq!(px(&s, 4, 20), [59, 5, 9, 255]);
    assert_eq!(px(&s, 41, 31), [43, 59, 14, 255]);
    assert_eq!(px(&s, 50, 30), [10, 20, 30, 255]);
    assert_eq!(px(&s, 60, 30), [10, 20, 30, 255]);
}

fn layer_scene(between: bool) -> Vec<DrawCmd> {
    let gray = rgba(100, 100, 100, 255);
    let mut cmds = vec![
        DrawCmd::Clear(BG),
        begin(0, 300, 200, 16, 8),
        DrawCmd::Clear(gray),
        fill(-4, 2, 8, 2, rgba(255, 0, 0, 255)),
        sprite(1, r(14, 6, 5, 3), rgba(255, 255, 255, 255)),
        DrawCmd::EndCachedLayer,
    ];
    if between {
        cmds.push(fill(305, 203, 2, 2, rgba(0, 0, 255, 255)));
    }
    cmds.push(draw_layer(0));
    cmds.push(fill(320, 200, 4, 4, rgba(255, 255, 0, 255)));
    cmds
}

#[test]
fn layer_is_drawn_inline_at_draw_cached_layer() {
    let s = draw(layer_scene(true));
    let gray = rgba(100, 100, 100, 255);
    assert!(is(&s, 300, 200, gray) && is(&s, 313, 207, gray));
    assert!(
        is(&s, 305, 203, gray),
        "Begin〜End の後に描いたものは層に隠れる"
    );
    assert!(is(&s, 300, 202, rgba(255, 0, 0, 255)));
    assert!(is(&s, 303, 203, rgba(255, 0, 0, 255)));
    assert!(is(&s, 299, 202, BG), "層の外は切り取る");
    assert_eq!(px(&s, 314, 206), STRIP[0]);
    assert_eq!(px(&s, 315, 207), STRIP[1]);
    assert!(is(&s, 316, 206, BG) && is(&s, 314, 208, BG));
    assert!(is(&s, 0, 0, BG), "層の Clear は画面全体を塗らない");
    assert!(is(&s, 320, 200, rgba(255, 255, 0, 255)));
    assert_eq!(draw(layer_scene(false)).pixels(), s.pixels());
}

#[test]
fn layer_rules_for_missing_empty_duplicate_and_bad_ids() {
    let base = draw(vec![DrawCmd::Clear(BG)]);
    let gray = rgba(100, 100, 100, 255);
    let content = [DrawCmd::Clear(gray), DrawCmd::EndCachedLayer];
    let mut no_begin = vec![DrawCmd::Clear(BG), draw_layer(0)];
    no_begin.push(draw_layer(4));
    assert_eq!(draw(no_begin).pixels(), base.pixels());
    for b in [
        begin(0, 10, 10, 0, 8),
        begin(0, 10, 10, 8, 0),
        begin(4, 10, 10, 8, 8),
    ] {
        let mut cmds = vec![DrawCmd::Clear(BG), b];
        cmds.extend(content);
        cmds.push(draw_layer(b_layer(&b)));
        assert_eq!(draw(cmds).pixels(), base.pixels(), "{b:?}");
    }
    let mut dup = vec![DrawCmd::Clear(BG), begin(1, 0, 0, 4, 4)];
    dup.extend(content);
    dup.push(begin(1, 20, 0, 4, 4));
    dup.push(DrawCmd::Clear(rgba(255, 0, 0, 255)));
    dup.push(DrawCmd::EndCachedLayer);
    dup.push(draw_layer(1));
    let s = draw(dup);
    assert!(is(&s, 3, 3, gray) && is(&s, 20, 0, BG));
}

fn b_layer(b: &DrawCmd) -> u8 {
    match b {
        DrawCmd::BeginCachedLayer { layer, .. } => layer.0,
        _ => unreachable!(),
    }
}

#[test]
fn nested_or_unclosed_layers_are_ignored_to_their_end() {
    let red = rgba(255, 0, 0, 255);
    let base = draw(vec![DrawCmd::Clear(BG), fill(0, 100, 4, 4, red)]);
    let nested = vec![
        DrawCmd::Clear(BG),
        begin(0, 0, 0, 8, 8),
        fill(0, 0, 2, 2, red),
        begin(1, 10, 0, 8, 8),
        fill(10, 0, 2, 2, red),
        DrawCmd::EndCachedLayer,
        fill(20, 0, 2, 2, red),
        DrawCmd::EndCachedLayer,
        DrawCmd::EndCachedLayer,
        fill(0, 100, 4, 4, red),
        draw_layer(0),
        draw_layer(1),
    ];
    assert_eq!(draw(nested).pixels(), base.pixels());
    let unclosed = vec![
        DrawCmd::Clear(BG),
        fill(0, 100, 4, 4, red),
        begin(0, 0, 0, 8, 8),
        fill(0, 0, 2, 2, red),
        draw_layer(0),
    ];
    assert_eq!(draw(unclosed).pixels(), base.pixels());
}

#[test]
fn draw_is_deterministic() {
    let mut cmds = layer_scene(true);
    for i in 0..200 {
        cmds.push(DrawCmd::Line {
            x0: (i * 37) % 640,
            y0: (i * 11) % 360,
            x1: (i * 53) % 640 - 20,
            y1: (i * 29) % 360,
            color: rgba(i as u8, 255 - i as u8, 128, (i * 7) as u8),
        });
        cmds.push(sprite(
            (i % 3) as u16,
            r(
                (i * 13) % 640,
                (i * 17) % 360,
                (i % 9) as u32 + 1,
                (i % 5) as u32 + 1,
            ),
            rgba(255, (i * 3) as u8, 200, (i * 5) as u8),
        ));
    }
    let a = draw(cmds.clone());
    let b = draw(cmds);
    assert_eq!(a.pixels(), b.pixels());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &v in a.pixels() {
        h = (h ^ u64::from(v)).wrapping_mul(0x0100_0000_01b3);
    }
    assert_eq!(h, 15_715_361_307_091_074_346);
}

#[test]
fn far_coordinates_do_not_overflow_or_walk_the_whole_line() {
    let red = rgba(255, 0, 0, 255);
    let s = draw(vec![
        DrawCmd::Line {
            x0: i32::MIN,
            y0: 10,
            x1: i32::MAX,
            y1: 10,
            color: red,
        },
        DrawCmd::Line {
            x0: 20,
            y0: i32::MAX,
            x1: 20,
            y1: i32::MIN,
            color: red,
        },
        begin(0, i32::MIN + 10, 0, 2000, 8),
        DrawCmd::Clear(rgba(0, 0, 255, 255)),
        fill(-100, 0, 4, 4, red),
        sprite(1, r(i32::MAX - 2, 0, 5, 1), rgba(255, 255, 255, 255)),
        DrawCmd::EndCachedLayer,
        draw_layer(0),
    ]);
    let lit = s.pixels().chunks(4).filter(|p| p[0] == 255).count();
    assert_eq!(lit, 640 + 360 - 1);
    assert!(is(&s, 639, 10, red) && is(&s, 20, 359, red));
    assert!(!s.pixels().chunks(4).any(|p| p[2] == 255));
}

#[test]
fn line_pixels_match_the_stepping_bresenham() {
    let white = rgba(255, 255, 255, 255);
    for (dx, dy) in [
        (37, 11),
        (-37, 11),
        (11, -37),
        (-11, -37),
        (25, 25),
        (0, -9),
        (7, 0),
    ] {
        let s = draw(vec![DrawCmd::Line {
            x0: 300,
            y0: 150,
            x1: 300 + dx,
            y1: 150 + dy,
            color: white,
        }]);
        let (adx, ady) = (i32::abs(dx), -i32::abs(dy));
        let (sx, sy) = (if dx < 0 { -1 } else { 1 }, if dy < 0 { -1 } else { 1 });
        let (mut x, mut y, mut err) = (300, 150, adx + ady);
        let mut want = vec![(x, y)];
        while (x, y) != (300 + dx, 150 + dy) {
            let e2 = 2 * err;
            if e2 >= ady {
                err += ady;
                x += sx;
            }
            if e2 <= adx {
                err += adx;
                y += sy;
            }
            want.push((x, y));
        }
        let lit = s.pixels().chunks(4).filter(|p| p[0] == 255).count();
        assert_eq!(lit, want.len(), "{dx},{dy}");
        for (x, y) in want {
            assert!(is(&s, x as usize, y as usize, white), "{dx},{dy} {x},{y}");
        }
    }
}
