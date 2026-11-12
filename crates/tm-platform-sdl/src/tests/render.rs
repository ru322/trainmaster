//! R-02〜R-04、R-06(14 §9.4)。

use tm_platform_api::{AssetBundle, Color, DrawCmd, DrawList, LayerId, Platform};

use sdl2::event::Event;

use super::{
    EXACT, MODES, Path, Tolerance, assert_close, assert_same, at_logical, config, count_logs, dump,
    fill, path_config, rect, rgba, sdl_frame, set_path, soft_frame, sprite, take_logs, test_assets,
    with_platform,
};
use crate::config::BatchMode;
use crate::platform::SdlPlatform;

const BG: Color = rgba(10, 20, 30, 255);
const WHITE: Color = rgba(255, 255, 255, 255);

/// `lists` を 6 通りの方式で順に描き、どれも `SoftRaster` と `tol` の範囲で一致することを確かめる。
pub(super) fn check_all_modes(assets: &AssetBundle, lists: &[(String, DrawList)], tol: Tolerance) {
    for (batch, path) in MODES {
        with_platform(&path_config(batch, path), assets, |p| {
            set_path(p, path);
            for (name, list) in lists {
                let got = sdl_frame(p, list);
                let want = soft_frame(assets, list);
                let what = format!("{name}-{batch:?}-{path:?}");
                assert_close(&got, &want, path.rgb_only(), &what, tol);
                if batch == BatchMode::Geometry && path == Path::Window {
                    dump(&format!("{name}-soft"), &want);
                    dump(&format!("{name}-sdl"), &got);
                }
            }
        });
    }
}

fn begin(layer: u8, revision: u64, x: i32, y: i32, w: u32, h: u32) -> DrawCmd {
    DrawCmd::BeginCachedLayer {
        layer: LayerId(layer),
        revision,
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

fn line(x0: i32, y0: i32, x1: i32, y1: i32, color: Color) -> DrawCmd {
    DrawCmd::Line {
        x0,
        y0,
        x1,
        y1,
        color,
    }
}

/// R-02 の決まった `DrawList`: 矩形、半透明、線の 8 方向、スプライトの拡大と `tint`、画面の端、キャッシュ層。
fn r02_scene() -> DrawList {
    let mut cmds = vec![
        DrawCmd::Clear(BG),
        fill(10, 10, 20, 10, rgba(255, 0, 0, 255)),
        fill(40, 10, 20, 10, rgba(255, 255, 255, 128)),
        fill(50, 15, 20, 10, rgba(0, 0, 255, 77)),
        fill(70, 10, 0, 10, rgba(255, 0, 0, 255)),
        fill(-6, 330, 20, 40, rgba(0, 200, 100, 200)),
        fill(630, -4, 30, 10, rgba(200, 0, 100, 255)),
    ];
    let dirs = [
        (9, 0),
        (9, 4),
        (6, 6),
        (4, 9),
        (0, 9),
        (-4, 9),
        (-6, 6),
        (-9, 4),
        (-9, 0),
        (-9, -4),
        (-6, -6),
        (-4, -9),
        (0, -9),
        (4, -9),
        (6, -6),
        (9, -4),
    ];
    for (i, (dx, dy)) in dirs.into_iter().enumerate() {
        let (x0, y0) = (30 + 25 * i as i32, 60);
        let a = if i % 2 == 0 { 255 } else { 140 };
        cmds.push(line(x0, y0, x0 + dx, y0 + dy, rgba(255, 220, 0, a)));
    }
    cmds.push(line(-5, 100, 700, 120, rgba(0, 255, 255, 255)));
    let mut x = 10;
    for (id, w, h) in [
        (1, 5, 1),
        (1, 10, 3),
        (1, 13, 2),
        (3, 6, 5),
        (3, 12, 10),
        (3, 9, 7),
        (3, 4, 3),
        (2, 3, 3),
        (4, 16, 4),
        (4, 32, 8),
    ] {
        cmds.push(sprite(id, rect(x, 150, w, h), WHITE));
        cmds.push(sprite(id, rect(x, 170, w, h), rgba(77, 177, 255, 200)));
        cmds.push(sprite(id, rect(x, 190, w, h), rgba(255, 128, 0, 255)));
        x += w as i32 + 4;
    }
    cmds.push(sprite(3, rect(-4, 220, 12, 10), WHITE));
    cmds.push(sprite(3, rect(634, 355, 12, 10), WHITE));
    cmds.extend([
        begin(0, 1, 300, 240, 64, 32),
        DrawCmd::Clear(rgba(90, 90, 90, 255)),
        fill(-4, 2, 8, 2, rgba(255, 0, 0, 255)),
        fill(10, 10, 20, 10, rgba(255, 255, 255, 100)),
        line(0, 0, 63, 31, rgba(0, 255, 0, 255)),
        sprite(3, rect(58, 28, 12, 10), WHITE),
        DrawCmd::EndCachedLayer,
        draw_layer(0),
        fill(320, 250, 60, 4, rgba(0, 0, 0, 128)),
    ]);
    DrawList { cmds }
}

#[test]
fn r02_r03_snapshot_matches_soft_raster_in_every_mode() {
    let assets = test_assets();
    let mut transparent = r02_scene();
    transparent.cmds[0] = DrawCmd::Clear(rgba(30, 60, 90, 200));
    transparent.cmds.truncate(transparent.cmds.len() - 9);
    check_all_modes(assets, &[("r02".to_owned(), r02_scene())], EXACT);
    with_platform(&config(BatchMode::Geometry), assets, |p| {
        let got = sdl_frame(p, &transparent);
        assert_same(&got, &soft_frame(assets, &transparent), false, "r02-alpha");
        assert_eq!(&got[3..4], &[200]);
    });
}

/// R-06: 640×288 の層を `x = −cam_x`、`y = 32` に置く。層の中の `Clear`、層の外へはみ出す
/// スプライト、Begin〜End と `DrawCachedLayer` の間のスプライト、層の後に層の外へ描く矩形を含む。
fn r06_scene(cam_x: i32, revision: u64, between: bool) -> DrawList {
    let mut cmds = vec![
        DrawCmd::Clear(rgba(30, 40, 70, 255)),
        begin(0, revision, -cam_x, 32, 640, 288),
        DrawCmd::Clear(rgba(40, 92, 52, 255)),
    ];
    for g in (0..640).step_by(16) {
        cmds.push(line(g, 0, g, 287, rgba(60, 120, 72, 255)));
    }
    for g in (0..288).step_by(16) {
        cmds.push(line(0, g, 639, g, rgba(60, 120, 72, 255)));
    }
    cmds.extend([
        fill(32, 32, 576, 8, rgba(90, 90, 120, 255)),
        fill(100, 100, 40, 40, rgba(255, 255, 255, 90)),
        sprite(3, rect(-6, 50, 12, 10), WHITE),
        sprite(3, rect(634, 120, 12, 10), WHITE),
        sprite(4, rect(200, -2, 32, 8), rgba(255, 255, 0, 255)),
        sprite(4, rect(300, 284, 32, 8), rgba(255, 0, 255, 255)),
        line(-20, 10, 660, 280, rgba(255, 255, 255, 255)),
        DrawCmd::EndCachedLayer,
    ]);
    if between {
        cmds.push(sprite(3, rect(120, 120, 12, 10), WHITE));
    }
    cmds.extend([
        draw_layer(0),
        fill(0, 0, 640, 32, rgba(20, 28, 52, 255)),
        fill(416, 32, 224, 288, rgba(255, 255, 255, 96)),
        fill(600, 10, 20, 20, rgba(255, 0, 0, 255)),
        sprite(3, rect(4, 340, 12, 10), WHITE),
    ]);
    DrawList { cmds }
}

#[test]
fn r06_positioned_layer_matches_in_cache_direct_and_soft_raster() {
    let mut lists = Vec::new();
    for cam_x in [0, 37, 224] {
        for between in [false, true] {
            lists.push((
                format!("r06-cam{cam_x}-{between}"),
                r06_scene(cam_x, 1, between),
            ));
        }
    }
    check_all_modes(test_assets(), &lists, EXACT);
}

#[test]
fn r06_layer_is_not_redrawn_when_only_the_position_moves() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        for (i, cam_x) in [0, 37, 224, 37].into_iter().enumerate() {
            let list = r06_scene(cam_x, 7, false);
            let got = sdl_frame(p, &list);
            assert_same(&got, &soft_frame(test_assets(), &list), false, "move");
            assert_eq!(p.renderer_mut().layer_redraws, 1, "{i}");
        }
    });
}

fn small_layer(revision: u64, x: i32, w: u32) -> DrawList {
    DrawList {
        cmds: vec![
            DrawCmd::Clear(BG),
            begin(0, revision, x, 200, w, 8),
            DrawCmd::Clear(rgba(100, 100, 100, 255)),
            fill(-4, 2, 8, 2, rgba(255, 0, 0, 255)),
            DrawCmd::EndCachedLayer,
            draw_layer(0),
        ],
    }
}

fn present_and_check(p: &mut SdlPlatform<'_>, list: DrawList, redraws: u32) {
    p.present(&list).unwrap();
    assert_eq!(p.stats().layer_redraws, redraws);
    let got = p.capture_frame().unwrap();
    assert_same(&got, &soft_frame(test_assets(), &list), false, "r04");
}

#[test]
fn r04_revision_size_and_reset_control_redraws() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        present_and_check(p, small_layer(1, 300, 16), 1);
        present_and_check(p, small_layer(1, 300, 16), 1);
        present_and_check(p, small_layer(1, 310, 16), 1);
        present_and_check(p, small_layer(2, 310, 16), 2);
        present_and_check(p, small_layer(2, 310, 17), 3);
        assert_eq!(p.renderer_mut().layer_recreates(), 1);
        present_and_check(p, small_layer(2, 310, 17), 3);
        p.handle_event(Event::RenderTargetsReset { timestamp: 0 }, &mut Vec::new());
        present_and_check(p, small_layer(2, 310, 17), 4);
        present_and_check(p, small_layer(2, 310, 17), 4);
        p.handle_event(Event::RenderDeviceReset { timestamp: 0 }, &mut Vec::new());
        present_and_check(p, small_layer(2, 300, 17), 5);
        assert_eq!(p.renderer_mut().layer_size(0), Some((51, 24)), "描画の画素");
    });
}

fn two_layers(w0: u32, h0: u32) -> DrawList {
    layers_of((w0, h0), (16, 8))
}

fn layers_of((w0, h0): (u32, u32), (w1, h1): (u32, u32)) -> DrawList {
    DrawList {
        cmds: vec![
            DrawCmd::Clear(BG),
            begin(0, 1, -10, -10, w0, h0),
            DrawCmd::Clear(rgba(0, 0, 80, 255)),
            fill(5, 300, 8, 8, WHITE),
            DrawCmd::EndCachedLayer,
            begin(1, 1, 300, 200, w1, h1),
            DrawCmd::Clear(rgba(100, 100, 100, 255)),
            fill(-4, 2, 8, 2, rgba(255, 0, 0, 255)),
            DrawCmd::EndCachedLayer,
            draw_layer(0),
            draw_layer(1),
            fill(305, 190, 4, 30, rgba(0, 255, 0, 255)),
        ],
    }
}

#[test]
fn r04_layers_over_the_size_limit_are_drawn_inline() {
    let assets = test_assets();
    with_platform(&config(BatchMode::Geometry), assets, |p| {
        for (w, h) in [(683, 8), (8, 683), (683, 683)] {
            let list = two_layers(w, h);
            let got = sdl_frame(p, &list);
            assert_same(&got, &soft_frame(assets, &list), false, "limit");
            assert_eq!(
                p.renderer_mut().layer_size(0),
                None,
                "描画の画素で 2048 を超える"
            );
            assert_eq!(p.renderer_mut().layer_size(1), Some((48, 24)));
            assert_eq!(p.renderer_mut().layer_redraws, 1);
        }
        let logs = take_logs();
        assert_eq!(count_logs(&logs, "その場描き"), 1, "{logs:?}");
    });
}

/// 682×682(描画の画素で 2046×2046、16MiB)の層を `layers` の番号に並べたフレーム。
fn big_layers(layers: &[u8]) -> DrawList {
    let mut cmds = vec![DrawCmd::Clear(BG)];
    for &l in layers {
        cmds.extend([
            begin(l, 1, -10 + i32::from(l) * 50, -10, 682, 682),
            DrawCmd::Clear(rgba(0, 0, 80 + l * 40, 255)),
            fill(5 + i32::from(l) * 20, 300, 8, 8, WHITE),
            DrawCmd::EndCachedLayer,
            draw_layer(l),
        ]);
    }
    DrawList { cmds }
}

#[test]
fn r04_layers_over_the_budget_are_drawn_inline() {
    let assets = test_assets();
    with_platform(&config(BatchMode::Geometry), assets, |p| {
        let list = big_layers(&[0, 1, 2]);
        let got = sdl_frame(p, &list);
        assert_same(&got, &soft_frame(assets, &list), false, "budget");
        assert_eq!(p.renderer_mut().layer_size(0), Some((2046, 2046)));
        assert_eq!(
            p.renderer_mut().layer_size(1),
            Some((2046, 2046)),
            "2 枚は 32MiB に入る"
        );
        assert_eq!(
            p.renderer_mut().layer_size(2),
            None,
            "3 枚目は合計 32MiB を超える"
        );
        assert_eq!(p.renderer_mut().layer_redraws, 2);
    });
}

#[test]
fn inline_layer_restores_clip_for_later_commands() {
    let assets = test_assets();
    let list = small_layer(1, 300, 683);
    let mut after = list.clone();
    after.cmds.push(fill(10, 300, 6, 6, WHITE));
    after.cmds.push(sprite(3, rect(20, 300, 6, 5), WHITE));
    for (batch, path) in MODES {
        with_platform(&path_config(batch, path), assets, |p| {
            set_path(p, path);
            let got = sdl_frame(p, &after);
            assert_same(&got, &soft_frame(assets, &after), path.rgb_only(), "clip");
            let i = at_logical(10, 300);
            assert_eq!(&got[i..i + 3], &[255, 255, 255]);
        });
    }
}

#[test]
fn direct_clear_does_not_paint_the_letterbox() {
    for path in [Path::Window, Path::NoTargets] {
        with_platform(
            &path_config(BatchMode::Geometry, path),
            test_assets(),
            |p| {
                set_path(p, path);
                p.canvas_mut().window_mut().set_size(2000, 1200).unwrap();
                let list = DrawList {
                    cmds: vec![
                        DrawCmd::Clear(rgba(200, 0, 0, 255)),
                        fill(0, 0, 4, 4, WHITE),
                    ],
                };
                let r = p.draw_only(&list);
                assert_eq!(r.error, None);
                let frame = p.capture_frame().unwrap();
                assert_same(&frame, &soft_frame(test_assets(), &list), true, "letterbox");
                let (w, px) = window_pixels(p);
                assert_eq!(w, 2000);
                assert_eq!(at(w, &px, 39, 59), [0, 0, 0], "帯は黒のまま");
                assert_eq!(at(w, &px, 1999, 1199), [0, 0, 0]);
                assert_eq!(
                    at(w, &px, 40, 60),
                    [255, 255, 255],
                    "フレームは (40, 60) から等倍"
                );
                assert_eq!(at(w, &px, 40 + 11, 60 + 11), [255, 255, 255]);
                assert_eq!(at(w, &px, 40 + 12, 60 + 12), [200, 0, 0]);
                assert_eq!(at(w, &px, 1959, 1139), [200, 0, 0]);
                assert_eq!(at(w, &px, 1960, 1140), [0, 0, 0]);
            },
        );
    }
}

/// 層の不透明な地の上に、色を掛けない淡い画素(アルファ 1〜254)を等倍で描く。層を Blend で写すと 4 までずれる組み合わせ。
fn faint_texels_in_layer() -> DrawList {
    let mut cmds = vec![DrawCmd::Clear(BG)];
    for (l, (bg, x)) in [(rgba(0, 255, 0, 255), 10), (rgba(255, 255, 255, 255), 200)]
        .into_iter()
        .enumerate()
    {
        cmds.push(begin(l as u8, 1, x, 20, 64, 16));
        cmds.push(DrawCmd::Clear(bg));
        for i in 0..8 {
            cmds.push(sprite(5, rect(i * 5, 2, 4, 1), WHITE));
            cmds.push(sprite(5, rect(i * 5, 5, 8, 2), WHITE));
        }
        cmds.push(DrawCmd::EndCachedLayer);
        cmds.push(draw_layer(l as u8));
    }
    DrawList { cmds }
}

#[test]
fn r02_faint_texels_inside_a_cached_layer_stay_within_two() {
    check_all_modes(
        test_assets(),
        &[("faint".to_owned(), faint_texels_in_layer())],
        EXACT,
    );
}

#[test]
fn stacked_translucent_texels_drift_at_most_two_per_layer_of_blending() {
    let mut cmds = vec![DrawCmd::Clear(rgba(0, 255, 0, 255))];
    for n in 1..=8 {
        for _ in 0..n {
            cmds.push(sprite(5, rect(n * 10, 10, 4, 1), WHITE));
            cmds.push(sprite(5, rect(n * 10, 20, 4, 1), rgba(255, 255, 255, 200)));
        }
    }
    let list = DrawList { cmds };
    let want = soft_frame(test_assets(), &list);
    for (batch, path) in MODES {
        with_platform(&path_config(batch, path), test_assets(), |p| {
            set_path(p, path);
            let got = sdl_frame(p, &list);
            for n in 1..=8usize {
                for (x, y) in (0..4).flat_map(|i| [(n * 10 + i, 10), (n * 10 + i, 20)]) {
                    let i = at_logical(x, y);
                    let d = (0..3).map(|k| got[i + k].abs_diff(want[i + k])).max();
                    assert!(
                        usize::from(d.unwrap()) <= 2 * n,
                        "{batch:?} {path:?} n={n} ({x}, {y}): {:?} {:?}",
                        &got[i..i + 4],
                        &want[i..i + 4]
                    );
                }
            }
        });
    }
}

#[test]
fn lines_inside_a_wide_layer_are_clipped_to_the_layer() {
    let mut cmds = vec![
        DrawCmd::Clear(BG),
        begin(0, 1, -400, -20, 1024, 400),
        DrawCmd::Clear(rgba(40, 92, 52, 255)),
    ];
    for k in 0..12 {
        let x = 640 + k * 30;
        cmds.push(line(x, 0, x + 20, 399, rgba(255, 255, 0, 255)));
        cmds.push(line(
            0,
            k * 33,
            1023,
            399 - k * 33,
            rgba(255, 255, 255, 255),
        ));
    }
    cmds.push(line(1023, 0, 1023, 399, rgba(255, 0, 0, 255)));
    cmds.push(DrawCmd::EndCachedLayer);
    cmds.push(draw_layer(0));
    check_all_modes(
        test_assets(),
        &[("wide".to_owned(), DrawList { cmds })],
        EXACT,
    );
}

/// ウィンドウ全体(帯を含む)の画素。
fn window_pixels(p: &mut SdlPlatform<'_>) -> (u32, Vec<u8>) {
    let c = p.canvas_mut();
    let logical = c.logical_size();
    c.set_logical_size(0, 0).unwrap();
    let (w, _) = c.output_size().unwrap();
    let px = c
        .read_pixels(None, sdl2::pixels::PixelFormatEnum::RGBA32)
        .unwrap();
    c.set_logical_size(logical.0, logical.1).unwrap();
    (w, px)
}

fn at(w: u32, px: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * w + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2]]
}

#[test]
fn r05_direct_frames_are_integer_scaled_with_letterbox_and_no_blend() {
    with_platform(
        &path_config(BatchMode::Geometry, Path::Window),
        test_assets(),
        |p| {
            let list = DrawList {
                cmds: vec![
                    DrawCmd::Clear(rgba(200, 100, 50, 100)),
                    fill(0, 0, 1, 1, WHITE),
                    fill(639, 359, 1, 1, WHITE),
                ],
            };
            for (size, s, x0, y0) in [((1920, 1200), 1, 0, 60), ((3840, 2160), 2, 0, 0)] {
                p.canvas_mut()
                    .window_mut()
                    .set_size(size.0, size.1)
                    .unwrap();
                assert_eq!(p.draw_only(&list).error, None);
                let (w, px) = window_pixels(p);
                assert_eq!(w, size.0);
                if y0 > 0 {
                    assert_eq!(at(w, &px, 5, y0 - 1), [0, 0, 0], "帯");
                    assert_eq!(at(w, &px, 5, y0 + 1080), [0, 0, 0], "帯");
                }
                assert_eq!(at(w, &px, x0, y0), [255, 255, 255]);
                assert_eq!(
                    at(w, &px, x0 + 3 * s - 1, y0 + 3 * s - 1),
                    [255, 255, 255],
                    "{s} 倍"
                );
                assert_eq!(
                    at(w, &px, x0 + 3 * s, y0 + 3 * s),
                    [200, 100, 50],
                    "アルファを掛けない"
                );
                assert_eq!(
                    at(w, &px, x0 + 1920 * s - 1, y0 + 1080 * s - 1),
                    [255, 255, 255]
                );
            }
        },
    );
}

#[test]
fn r05_small_windows_get_the_frame_scaled_down() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        p.canvas_mut().window_mut().set_size(1280, 720).unwrap();
        let list = DrawList {
            cmds: vec![
                DrawCmd::Clear(rgba(200, 0, 0, 255)),
                fill(320, 0, 320, 360, WHITE),
            ],
        };
        assert_eq!(p.draw_only(&list).error, None);
        assert_eq!(p.canvas_mut().logical_size(), (0, 0));
        let (w, px) = window_pixels(p);
        assert_eq!(at(w, &px, 100, 300), [200, 0, 0]);
        assert_eq!(at(w, &px, 1000, 300), [255, 255, 255]);
        assert_eq!(
            p.capture_frame().unwrap().len(),
            1920 * 1080 * 4,
            "フレームテクスチャは描画の解像度"
        );
    });
}

#[test]
fn small_windows_without_render_targets_use_the_logical_size() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        p.canvas_mut().window_mut().set_size(1280, 720).unwrap();
        p.force_direct();
        let list = DrawList {
            cmds: vec![
                DrawCmd::Clear(rgba(200, 0, 0, 255)),
                fill(630, 350, 10, 10, WHITE),
            ],
        };
        assert_eq!(p.draw_only(&list).error, None);
        assert_eq!(p.canvas_mut().logical_size(), (1920, 1080));
        let (w, px) = window_pixels(p);
        assert_eq!(at(w, &px, 1270, 710), [255, 255, 255]);
        assert_eq!(at(w, &px, 1259, 699), [200, 0, 0]);
        assert!(
            p.capture_frame().is_err(),
            "縮めた絵は描画の解像度では読めない"
        );
        p.renderer_mut().inject.frame_tex = false;
        p.handle_event(Event::RenderDeviceReset { timestamp: 0 }, &mut Vec::new());
        assert!(!p.renderer_mut().is_direct());
        assert_eq!(p.draw_only(&list).error, None);
        assert_eq!(
            p.canvas_mut().logical_size(),
            (0, 0),
            "フレームテクスチャに戻ると論理サイズを外す"
        );
        let (w, px) = window_pixels(p);
        assert_eq!(at(w, &px, 1272, 712), [255, 255, 255]);
    });
}

#[test]
fn far_coordinates_do_not_overflow() {
    let red = rgba(255, 0, 0, 255);
    let list = DrawList {
        cmds: vec![
            DrawCmd::Clear(BG),
            line(i32::MIN, 10, i32::MAX, 12, red),
            line(20, i32::MAX, 22, i32::MIN, red),
            fill(i32::MIN, 300, u32::MAX, 2, red),
            begin(0, 1, i32::MIN + 10, 0, 2000, 8),
            DrawCmd::Clear(rgba(0, 0, 255, 255)),
            fill(-100, 0, 4, 4, red),
            line(i32::MIN, 0, i32::MAX, 0, red),
            sprite(3, rect(i32::MAX - 2, 0, 6, 5), WHITE),
            DrawCmd::EndCachedLayer,
            draw_layer(0),
            begin(1, 1, i32::MAX - 5, 0, 16, 8),
            DrawCmd::Clear(rgba(0, 0, 255, 255)),
            DrawCmd::EndCachedLayer,
            draw_layer(1),
            sprite(3, rect(i32::MAX - 2, 0, 6, 5), WHITE),
            sprite(3, rect(i32::MIN, 0, 6, 5), WHITE),
        ],
    };
    check_all_modes(test_assets(), &[("far".to_owned(), list)], EXACT);
}

#[test]
fn r04_layer_height_and_width_limits_follow_the_renderer() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        for (max, w, h, cached) in [
            ((1024, 48), 64, 16, true),
            ((1024, 48), 64, 17, false),
            ((48, 1024), 16, 64, true),
            ((48, 1024), 17, 64, false),
        ] {
            p.renderer_mut().set_max_tex(max);
            let list = DrawList {
                cmds: vec![
                    DrawCmd::Clear(BG),
                    begin(0, 1, 10, 10, w, h),
                    DrawCmd::Clear(rgba(100, 100, 100, 255)),
                    DrawCmd::EndCachedLayer,
                    draw_layer(0),
                ],
            };
            let got = sdl_frame(p, &list);
            assert_same(&got, &soft_frame(test_assets(), &list), false, "max_tex");
            let size = p.renderer_mut().layer_size(0);
            assert_eq!(size.is_some(), cached, "{max:?} {w}×{h}");
        }
    });
}

#[test]
fn layers_absent_this_frame_give_way_to_the_budget() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        sdl_frame(p, &big_layers(&[0, 1]));
        assert_eq!(p.renderer_mut().layer_size(0), Some((2046, 2046)));
        assert_eq!(p.renderer_mut().layer_size(1), Some((2046, 2046)));
        let list = big_layers(&[2]);
        for _ in 0..2 {
            let got = sdl_frame(p, &list);
            assert_same(&got, &soft_frame(test_assets(), &list), false, "budget");
        }
        assert_eq!(p.renderer_mut().layer_size(0), None);
        assert_eq!(p.renderer_mut().layer_size(1), None);
        assert_eq!(p.renderer_mut().layer_size(2), Some((2046, 2046)));
        assert_eq!(p.renderer_mut().layer_redraws, 3);
    });
}

/// 細かい座標の四角形(斜めの帯、三角形、層の中、画面の外へはみ出すもの、半透明の重なり)。
#[test]
fn quads_match_soft_raster_in_every_mode() {
    let quad = |pts: [(i32, i32); 4], color| DrawCmd::Quad { pts, color };
    let list = DrawList {
        cmds: vec![
            DrawCmd::Clear(BG),
            quad(
                [(30, 30), (54, 30), (354, 330), (330, 330)],
                rgba(255, 220, 0, 255),
            ),
            quad(
                [(400, 40), (500, 40), (450, 140), (450, 140)],
                rgba(0, 200, 255, 160),
            ),
            quad(
                [(420, 60), (520, 60), (470, 160), (470, 160)],
                rgba(255, 0, 0, 120),
            ),
            quad(
                [(-50, 600), (200, 590), (210, 640), (-40, 650)],
                rgba(0, 255, 0, 255),
            ),
            quad(
                [(1900, 1000), (2100, 1000), (2100, 1200), (1900, 1200)],
                WHITE,
            ),
            begin(0, 1, 200, 200, 60, 40),
            DrawCmd::Clear(rgba(60, 60, 60, 255)),
            quad(
                [(0, 0), (12, 0), (192, 120), (180, 120)],
                rgba(255, 255, 255, 255),
            ),
            DrawCmd::EndCachedLayer,
            draw_layer(0),
        ],
    };
    check_all_modes(test_assets(), &[("quads".to_owned(), list)], EXACT);
}

/// 黒い帯のあるウィンドウ(1920×1080 の整数倍でない出力)でも、層・スプライト・層の中身は帯に出ない。
/// 層をその場で描いた後も、切り取りはフレームの範囲に戻る。
#[test]
fn bars_stay_black_with_layers_and_sprites() {
    for (batch, path) in MODES {
        if path == Path::Frame {
            continue;
        }
        with_platform(&path_config(batch, path), test_assets(), |p| {
            set_path(p, path);
            p.canvas_mut().window_mut().set_size(2000, 1200).unwrap();
            let list = DrawList {
                cmds: vec![
                    DrawCmd::Clear(rgba(200, 0, 0, 255)),
                    begin(0, 1, 600, 330, 100, 100),
                    DrawCmd::Clear(rgba(0, 200, 0, 255)),
                    DrawCmd::EndCachedLayer,
                    draw_layer(0),
                    begin(1, 1, -50, -50, 100, 100),
                    DrawCmd::Clear(rgba(0, 0, 200, 255)),
                    DrawCmd::EndCachedLayer,
                    draw_layer(1),
                    sprite(3, rect(300, 350, 20, 20), WHITE),
                    fill(-20, 100, 10, 10, WHITE),
                    DrawCmd::Quad {
                        pts: [(1900, 1000), (2200, 1000), (2200, 1300), (1900, 1300)],
                        color: WHITE,
                    },
                ],
            };
            assert_eq!(p.draw_only(&list).error, None);
            let (w, px) = window_pixels(p);
            for (what, x, y) in [
                ("右の帯(層 0)", 1990, 1100),
                ("下の帯(層 0)", 1900, 1190),
                ("上の帯(層 1)", 60, 10),
                ("左の帯(層 1)", 10, 100),
                ("下の帯(スプライト)", 40 + 3 * 305, 1150),
                ("左の帯(矩形)", 5, 60 + 3 * 105),
                ("右下の帯(四角形)", 1965, 1145),
            ] {
                assert_eq!(at(w, &px, x, y), [0, 0, 0], "{batch:?} {path:?} {what}");
            }
            assert_eq!(at(w, &px, 1900, 1100), [0, 200, 0], "フレームの中の層 0");
            assert_eq!(at(w, &px, 40, 60), [0, 0, 200], "フレームの中の層 1");
            assert_eq!(
                at(w, &px, 1959, 1139),
                [255, 255, 255],
                "フレームの中の四角形"
            );
            let list2 = DrawList {
                cmds: vec![
                    DrawCmd::Clear(rgba(200, 0, 0, 255)),
                    begin(0, 1, 600, 330, 700, 100),
                    DrawCmd::Clear(rgba(0, 200, 0, 255)),
                    DrawCmd::EndCachedLayer,
                    draw_layer(0),
                    sprite(0, rect(630, 350, 20, 20), WHITE),
                ],
            };
            assert_eq!(p.draw_only(&list2).error, None);
            let (w, px) = window_pixels(p);
            assert_eq!(
                at(w, &px, 1990, 1100),
                [0, 0, 0],
                "{path:?}: その場描きの層の後も帯は黒"
            );
            assert_eq!(
                at(w, &px, 1975, 1150),
                [0, 0, 0],
                "{path:?}: その場描きの層の後のスプライト"
            );
            assert_eq!(
                at(w, &px, 1955, 1130),
                [255, 255, 255],
                "{path:?}: スプライトはフレームの中には描く"
            );
        });
    }
}

/// 帯のない出力では `Clear` はウィンドウ全体を塗る(矩形 1 枚を減らす)。
#[test]
fn clear_without_bars_paints_the_whole_window() {
    with_platform(
        &path_config(BatchMode::Geometry, Path::Window),
        test_assets(),
        |p| {
            let list = DrawList {
                cmds: vec![DrawCmd::Clear(rgba(10, 20, 30, 255))],
            };
            assert_eq!(p.draw_only(&list).error, None);
            let (w, px) = window_pixels(p);
            assert_eq!(at(w, &px, 0, 0), [10, 20, 30]);
            assert_eq!(at(w, &px, 1919, 1079), [10, 20, 30]);
        },
    );
}

/// 小さいウィンドウへは線形補間で縮める(1 画素の縞は中間の色になる。この縞では半分)。最近傍では黒か白だけが残る。
#[test]
fn frame_texture_is_scaled_down_with_linear_filtering() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        p.canvas_mut().window_mut().set_size(1280, 720).unwrap();
        let mut cmds = vec![DrawCmd::Clear(rgba(0, 0, 0, 255))];
        for i in 0..640 {
            cmds.push(DrawCmd::Quad {
                pts: [(3 * i, 0), (3 * i + 1, 0), (3 * i + 1, 1080), (3 * i, 1080)],
                color: WHITE,
            });
        }
        assert_eq!(p.draw_only(&DrawList { cmds }).error, None);
        let (w, px) = window_pixels(p);
        let row: Vec<u8> = (100..400).map(|x| at(w, &px, x, 300)[0]).collect();
        let gray = row.iter().filter(|&&v| v > 20 && v < 235).count();
        assert!(
            gray * 3 > row.len(),
            "中間の色が 3 分の 1 を超える: {gray}/{}",
            row.len()
        );
    });
}
