#![cfg(feature = "headless")]

mod common;

use common::{bundle, leak, rect};
use tm_platform_api::{
    AssetBundle, AtlasImage, Color, DrawCmd, DrawList, LayerId, RENDER_HEIGHT, RENDER_SCALE,
    RENDER_WIDTH, Rect, SoftRaster, SpriteAlt, SpriteId,
};

const K: usize = RENDER_SCALE as usize;
const NW: usize = RENDER_WIDTH as usize;

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

/// 描画の画素 `(x, y)`。
fn native(s: &SoftRaster, x: usize, y: usize) -> [u8; 4] {
    let i = (y * NW + x) * 4;
    s.pixels()[i..i + 4].try_into().unwrap()
}

/// 論理の画素 `(x, y)` の色。論理の画素の `K × K` の塊がそろっていることも確かめる。
fn px(s: &SoftRaster, x: usize, y: usize) -> [u8; 4] {
    let c = native(s, x * K, y * K);
    for j in 0..K {
        for i in 0..K {
            assert_eq!(
                native(s, x * K + i, y * K + j),
                c,
                "論理 ({x}, {y}) の塊がそろわない"
            );
        }
    }
    c
}

/// `pred` に合う描画の画素の数を、論理の画素の数にする。
fn lit(s: &SoftRaster, pred: impl Fn(&[u8]) -> bool) -> usize {
    let n = s.pixels().chunks(4).filter(|p| pred(p)).count();
    assert_eq!(n % (K * K), 0);
    n / (K * K)
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
    assert_eq!(s.pixels().len(), 1920 * 1080 * 4);
    assert_eq!((RENDER_WIDTH, RENDER_HEIGHT), (1920, 1080));
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
    assert_eq!(lit(&s, |p| p[0] == 255), 4 + 16 + 640 - 2);
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
        let n = lit(&s, |p| p[1] == 255) as i32;
        assert_eq!(n, dx.abs().max(dy.abs()) + 1, "{dx},{dy}");
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
    for w in 1..=13usize {
        let n = w * K;
        let inc = (5i64 << 16) / n as i64;
        for x in 0..n {
            let i = ((inc / 2 + x as i64 * inc) >> 16) as usize;
            for dy in 0..K {
                assert_eq!(native(&s, x, w * 2 * K + dy), STRIP[i], "w={w} x={x}");
            }
        }
        assert_eq!(native(&s, n, w * 2 * K), [0, 0, 0, 255]);
    }
    for (x, i) in [(0, 0), (2, 0), (3, 1), (8, 2), (14, 4)] {
        assert_eq!(
            native(&s, x, 5 * 2 * K),
            STRIP[i],
            "等倍の 5 画素は 3 画素ずつ"
        );
    }
    // 画素の中心で 5 画素の帯を見たときの、手で数えた並び(描画の幅 = 論理の幅 × 3)
    let want: [(usize, &str); 9] = [
        (1, "024"),
        (2, "012234"),
        (3, "001123344"),
        (4, "001112233344"),
        (5, "000111222333444"),
        (7, "000011112222233334444"),
        (9, "000001111112222233333344444"),
        (11, "000000011111122222223333334444444"),
        (13, "000000001111111122222223333333344444444"),
    ];
    for (w, row) in want {
        let got: String = (0..w * K)
            .map(|x| {
                let p = native(&s, x, w * 2 * K + 1);
                char::from(b'0' + STRIP.iter().position(|c| *c == p).unwrap() as u8)
            })
            .collect();
        assert_eq!(got, row, "論理の幅 {w}");
    }
    let s = draw(vec![sprite(1, r(0, 0, 5, 3), white)]);
    assert_eq!(px(&s, 4, 2), STRIP[4]);
    assert_eq!(px(&s, 4, 3), [0, 0, 0, 255]);
}

/// 白(0、2×2)と、論理 1px の赤(1、3×3)。1 の大きい版(6×6)は左半分が赤、右半分が緑。
fn alt_assets(alt: tm_platform_api::SpriteRect) -> AssetBundle {
    let (w, h) = (12usize, 6usize);
    let mut px = vec![0u8; w * h * 4];
    let mut put = |x: usize, y: usize, c: [u8; 4]| {
        px[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&c)
    };
    for y in 0..3 {
        for x in 0..3 {
            put(x, y, [255, 0, 0, 255]);
        }
    }
    for y in 0..6 {
        for x in 0..6 {
            put(
                3 + x,
                y,
                if x < 3 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                },
            );
        }
    }
    for (x, y) in [(9, 0), (10, 0), (9, 1), (10, 1)] {
        put(x, y, [255; 4]);
    }
    let mut b = bundle(
        vec![AtlasImage {
            width: w as u32,
            height: h as u32,
            rgba: leak(px),
        }],
        vec![rect(0, 9, 0, 2, 2), rect(0, 0, 0, 3, 3)],
        vec![],
    );
    b.alt = leak(vec![SpriteAlt {
        id: SpriteId(1),
        rect: alt,
    }]);
    b
}

#[test]
fn sprite_switches_to_the_alt_only_when_magnified_in_both_axes() {
    let white = rgba(255, 255, 255, 255);
    assert_eq!(
        alt_assets(rect(0, 3, 0, 3, 6)).validate(),
        Err(tm_platform_api::AssetError::SpriteAlt {
            sprite: SpriteId(1)
        }),
        "大きい版は縦横とも元より大きい"
    );
    let b = alt_assets(rect(0, 3, 0, 6, 6));
    assert_eq!(b.validate(), Ok(()));
    let mut s = SoftRaster::new();
    let one = |dst| DrawCmd::Sprite {
        id: SpriteId(1),
        dst,
        tint: white,
    };
    s.draw(
        &b,
        &DrawList {
            cmds: vec![one(r(0, 0, 1, 1)), one(r(10, 0, 2, 2)), one(r(20, 0, 9, 1))],
        },
    );
    assert_eq!(native(&s, 2, 2), [255, 0, 0, 255], "等倍は元の絵");
    assert_eq!(native(&s, 30, 0), [255, 0, 0, 255]);
    assert_eq!(native(&s, 33, 5), [0, 255, 0, 255], "2 倍は大きい版");
    assert_eq!(native(&s, 86, 2), [255, 0, 0, 255], "縦が等倍なら元の絵");
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
    assert_eq!(native(&s, 0, 20 * K), [19, 17, 25, 255], "5 画素の帯の左端");
    assert_eq!(native(&s, 4 * K, 20 * K), [59, 5, 9, 255]);
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
    assert_eq!(h, 14_117_076_021_316_968_440);
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
    assert_eq!(lit(&s, |p| p[0] == 255), 640 + 360 - 1);
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
        assert_eq!(lit(&s, |p| p[0] == 255), want.len(), "{dx},{dy}");
        for (x, y) in want {
            assert!(is(&s, x as usize, y as usize, white), "{dx},{dy} {x},{y}");
        }
    }
}

#[test]
fn quad_uses_fine_coordinates_and_moves_with_the_layer() {
    let red = rgba(255, 0, 0, 255);
    let quad = DrawCmd::Quad {
        pts: [(0, 0), (6, 0), (36, 30), (30, 30)],
        color: red,
    };
    let s = draw(vec![quad]);
    assert_eq!(s.pixels().chunks(4).filter(|p| p[0] == 255).count(), 30 * 6);
    assert_eq!(native(&s, 0, 0), [255, 0, 0, 255]);
    assert_eq!(native(&s, 6, 0), [0, 0, 0, 255]);
    assert_eq!(native(&s, 28, 29), [0, 0, 0, 255]);
    assert_eq!(
        native(&s, 29, 29),
        [255, 0, 0, 255],
        "中心が左の辺の上なら塗る"
    );
    assert_eq!(native(&s, 35, 29), [0, 0, 0, 255], "右の辺の上は塗らない");
    let s = draw(vec![
        DrawCmd::Clear(BG),
        begin(0, 100, 50, 10, 4),
        DrawCmd::Clear(rgba(100, 100, 100, 255)),
        quad,
        DrawCmd::EndCachedLayer,
        draw_layer(0),
    ]);
    assert_eq!(native(&s, 300, 150), [255, 0, 0, 255], "層の位置へ動く");
    assert_eq!(native(&s, 300 + 11, 150 + 11), [255, 0, 0, 255]);
    assert_eq!(native(&s, 300 + 23, 150 + 11), [100, 100, 100, 255]);
    assert_eq!(
        native(&s, 300 + 12, 150 + 12),
        [10, 20, 30, 255],
        "層の高さ 4 = 12 画素で切る"
    );
}
