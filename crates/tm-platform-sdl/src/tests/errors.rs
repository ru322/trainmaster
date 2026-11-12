//! エラー処理(14 §3.8): 描画の失敗、層の描き直しと作り直しの失敗、直接描画モードへの切り替え、不正な層。

use sdl2::event::Event;
use tm_platform_api::{DrawCmd, DrawList, LayerId, Platform, PlatformError};

use super::{
    MODES, assert_same, config, count_logs, fill, path_config, rgba, sdl_frame, set_path,
    soft_frame, take_logs, test_assets, with_platform,
};
use crate::config::BatchMode;

fn layer(revision: u64, w: u32) -> DrawList {
    DrawList {
        cmds: vec![
            DrawCmd::Clear(rgba(10, 20, 30, 255)),
            DrawCmd::BeginCachedLayer {
                layer: LayerId(0),
                revision,
                x: 300,
                y: 200,
                w,
                h: 8,
            },
            DrawCmd::Clear(rgba(100, 100, 100, 255)),
            fill(-4, 2, 8, 2, rgba(255, 0, 0, 255)),
            fill(14, 0, 8, 8, rgba(0, 0, 255, 255)),
            DrawCmd::EndCachedLayer,
            DrawCmd::DrawCachedLayer { layer: LayerId(0) },
            fill(330, 200, 4, 4, rgba(255, 255, 255, 255)),
        ],
    }
}

#[test]
fn sixty_failed_frames_in_a_row_are_an_error() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        let list = layer(1, 16);
        p.renderer_mut().inject.frame_draw = true;
        for _ in 0..59 {
            assert_eq!(p.present(&list), Ok(()));
        }
        p.renderer_mut().inject.frame_draw = false;
        assert_eq!(p.present(&list), Ok(()));
        p.renderer_mut().inject.frame_draw = true;
        for _ in 0..59 {
            assert_eq!(p.present(&list), Ok(()));
        }
        assert!(matches!(p.present(&list), Err(PlatformError::Render(_))));
        assert_eq!(count_logs(&take_logs(), "描画のエラー"), 1);
    });
}

#[test]
fn failed_layer_redraw_is_retried_next_frame() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        let list = layer(1, 16);
        p.renderer_mut().inject.layer_draw = true;
        assert_eq!(p.present(&list), Ok(()));
        assert_eq!(p.stats().layer_redraws, 1);
        p.renderer_mut().inject.layer_draw = false;
        assert_eq!(p.present(&list), Ok(()));
        assert_eq!(p.stats().layer_redraws, 2);
        assert_eq!(p.present(&list), Ok(()));
        assert_eq!(p.stats().layer_redraws, 2);
        let got = p.capture_frame().unwrap();
        assert_same(&got, &soft_frame(test_assets(), &list), false, "retry");
    });
}

#[test]
fn layer_that_cannot_be_recreated_is_drawn_inline() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        p.renderer_mut().set_max_tex((48, 48));
        p.present(&layer(1, 16)).unwrap();
        assert_eq!(p.renderer_mut().layer_size(0), Some((48, 24)));
        for _ in 0..3 {
            let list = layer(1, 17);
            p.present(&list).unwrap();
            let got = p.capture_frame().unwrap();
            assert_same(&got, &soft_frame(test_assets(), &list), false, "recreate");
        }
        assert_eq!(
            p.renderer_mut().layer_size(0),
            None,
            "古いテクスチャは捨てる"
        );
        assert_eq!(p.renderer_mut().layer_recreates(), 1);
        assert_eq!(p.stats().layer_redraws, 1);
        let logs = take_logs();
        assert_eq!(count_logs(&logs, "層 0 はその場描きにする"), 1, "{logs:?}");
    });
}

#[test]
fn device_reset_without_render_targets_falls_back_to_direct_mode() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        let list = layer(1, 16);
        p.present(&list).unwrap();
        p.renderer_mut().inject.frame_tex = true;
        p.handle_event(Event::RenderDeviceReset { timestamp: 0 }, &mut Vec::new());
        assert!(p.renderer_mut().is_direct());
        let logs = take_logs();
        assert_eq!(
            count_logs(&logs, "レンダーターゲットに対応していない"),
            1,
            "{logs:?}"
        );
        p.canvas_mut().window_mut().set_size(1920, 1080).unwrap();
        for _ in 0..2 {
            p.present(&list).unwrap();
            let got = p.capture_frame().unwrap();
            assert_same(&got, &soft_frame(test_assets(), &list), true, "fallback");
        }
        assert_eq!(p.stats().layer_redraws, 1);
    });
}

fn begin(layer: u8, x: i32) -> DrawCmd {
    DrawCmd::BeginCachedLayer {
        layer: LayerId(layer),
        revision: 1,
        x,
        y: 10,
        w: 8,
        h: 8,
    }
}

fn malformed() -> Vec<(&'static str, Vec<DrawCmd>, bool)> {
    let bg = DrawCmd::Clear(rgba(10, 20, 30, 255));
    let red = |x| fill(x, 0, 4, 4, rgba(255, 0, 0, 255));
    let draw = |l| DrawCmd::DrawCachedLayer { layer: LayerId(l) };
    let end = DrawCmd::EndCachedLayer;
    vec![
        (
            "nested",
            vec![
                bg,
                begin(0, 0),
                red(0),
                begin(1, 20),
                red(1),
                end,
                red(2),
                end,
                red(40),
                draw(0),
                draw(1),
            ],
            true,
        ),
        (
            "unclosed",
            vec![bg, red(40), begin(0, 0), red(0), draw(0)],
            true,
        ),
        ("stray-end", vec![bg, end, red(40), end], false),
        (
            "duplicate",
            vec![
                bg,
                begin(2, 0),
                DrawCmd::Clear(rgba(0, 0, 255, 255)),
                end,
                begin(2, 30),
                red(0),
                end,
                draw(2),
                draw(2),
            ],
            false,
        ),
        (
            "bad-id",
            vec![bg, begin(4, 0), red(0), end, draw(4), red(40)],
            false,
        ),
    ]
}

#[test]
fn malformed_layers_are_ignored_like_soft_raster() {
    for (batch, path) in MODES {
        with_platform(&path_config(batch, path), test_assets(), |p| {
            set_path(p, path);
            for (name, cmds, broken) in malformed() {
                let list = DrawList { cmds };
                let r = p.draw_only(&list);
                assert!(r.bad_layer, "{name}");
                assert_eq!(r.broken_layer, broken, "{name}");
                let got = sdl_frame(p, &list);
                assert_same(
                    &got,
                    &soft_frame(test_assets(), &list),
                    path.rgb_only(),
                    name,
                );
            }
        });
    }
}

#[test]
fn ignored_layer_commands_are_logged_once() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        for (_, cmds, broken) in malformed() {
            if !broken {
                p.present(&DrawList { cmds }).unwrap();
            }
        }
        assert_eq!(
            count_logs(&take_logs(), "対応のないキャッシュ層のコマンド"),
            1
        );
    });
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "入れ子"))]
fn nested_layer_asserts_in_debug_builds() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        let (_, cmds, _) = malformed().swap_remove(0);
        p.present(&DrawList { cmds }).unwrap();
    });
}

#[test]
fn failed_layer_texture_is_not_retried_until_the_size_changes() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        p.renderer_mut().inject.layer_tex = true;
        let list = layer(1, 16);
        p.present(&list).unwrap();
        p.renderer_mut().inject.layer_tex = false;
        for _ in 0..3 {
            p.present(&list).unwrap();
            assert_eq!(p.renderer_mut().layer_size(0), None);
            let got = p.capture_frame().unwrap();
            assert_same(&got, &soft_frame(test_assets(), &list), false, "backoff");
        }
        assert_eq!(p.stats().layer_redraws, 0);
        assert_eq!(count_logs(&take_logs(), "その場描き"), 1);
        p.present(&layer(1, 17)).unwrap();
        assert_eq!(p.renderer_mut().layer_size(0), Some((51, 24)));
        p.renderer_mut().inject.layer_tex = true;
        p.present(&layer(1, 16)).unwrap();
        p.renderer_mut().inject.layer_tex = false;
        p.handle_event(Event::RenderDeviceReset { timestamp: 0 }, &mut Vec::new());
        p.present(&layer(1, 16)).unwrap();
        assert_eq!(
            p.renderer_mut().layer_size(0),
            Some((48, 24)),
            "リセットで試し直す"
        );
    });
}

#[test]
fn layer_without_an_opaque_clear_is_reported() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        let mut list = layer(1, 16);
        assert!(!p.draw_only(&list).layer_not_opaque);
        list.cmds[2] = DrawCmd::Clear(rgba(100, 100, 100, 254));
        assert!(p.draw_only(&list).layer_not_opaque);
        list.cmds.remove(2);
        assert!(p.draw_only(&list).layer_not_opaque);
        p.present(&list).unwrap();
        p.present(&list).unwrap();
        let n = count_logs(&take_logs(), "不透明な Clear でない");
        assert_eq!(n, usize::from(cfg!(debug_assertions)));
    });
}
