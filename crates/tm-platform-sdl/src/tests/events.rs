//! イベントの処理、時計と計測(14 §3.5.6、§3.7、§3.8)。

use std::time::Duration;

use sdl2::event::Event;
use tm_platform_api::{AudioCmd, DrawCmd, DrawList, LayerId, Platform, SoundId};

use super::{config, count_logs, fill, rgba, take_logs, test_assets, with_platform};
use crate::config::BatchMode;

fn removed(which: u32, iscapture: bool) -> Event {
    Event::AudioDeviceRemoved {
        timestamp: 0,
        which,
        iscapture,
    }
}

#[test]
fn playback_device_removal_turns_audio_off() {
    let mut cfg = config(BatchMode::Geometry);
    cfg.audio.enabled = true;
    with_platform(&cfg, test_assets(), |p| {
        assert!(p.stats().audio.enabled);
        let mut out = Vec::new();
        p.handle_event(removed(2, true), &mut out);
        p.handle_event(removed(0, false), &mut out);
        assert!(p.stats().audio.enabled, "which = 0 は開いていないデバイス");
        assert!(p.audio_callback_count().is_some());
        p.handle_event(removed(2, false), &mut out);
        assert!(!p.stats().audio.enabled);
        assert_eq!(p.audio_callback_count(), None);
        p.handle_event(removed(2, false), &mut out);
        p.audio(&[AudioCmd::PlaySe(SoundId(0))]);
        assert!(out.is_empty());
        assert_eq!(count_logs(&take_logs(), "外された"), 1);
    });
}

#[test]
fn present_reports_interval_present_and_render_times() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        let list = DrawList {
            cmds: vec![DrawCmd::Clear(rgba(1, 2, 3, 255))],
        };
        p.present(&list).unwrap();
        assert_eq!(p.stats().frame_interval_us, 0);
        std::thread::sleep(Duration::from_millis(30));
        p.present(&list).unwrap();
        let s = p.stats();
        assert!(s.frame_interval_us >= 30_000, "{s:?}");
        assert!(s.frame_interval_us > s.present_us, "{s:?}");
        assert!(p.render_us() <= s.present_us);
    });
}

#[test]
fn layer_size_changes_are_counted() {
    with_platform(&config(BatchMode::Geometry), test_assets(), |p| {
        for i in 0..10u32 {
            let list = DrawList {
                cmds: vec![
                    DrawCmd::BeginCachedLayer {
                        layer: LayerId(0),
                        revision: 1,
                        x: 0,
                        y: 0,
                        w: 16 + i % 2,
                        h: 8,
                    },
                    DrawCmd::Clear(rgba(100, 100, 100, 255)),
                    fill(0, 0, 2, 2, rgba(255, 0, 0, 255)),
                    DrawCmd::EndCachedLayer,
                    DrawCmd::DrawCachedLayer { layer: LayerId(0) },
                ],
            };
            assert_eq!(p.draw_only(&list).error, None);
        }
        assert_eq!(p.renderer_mut().layer_recreates(), 9);
        assert_eq!(p.renderer_mut().layer_redraws, 10);
    });
}

#[test]
fn mapping_lines_that_pass_the_check_are_accepted_by_sdl() {
    let _guard = super::sdl_lock();
    let sdl = sdl2::init().unwrap();
    let gc = sdl.game_controller().unwrap();
    let sample = crate::pad_mappings::tests::SAMPLE;
    assert_eq!(crate::pad_mappings::check(sample), Ok(()));
    for m in crate::pad_mappings::PAD_MAPPINGS.iter().chain([&sample]) {
        assert!(gc.add_mapping(m).is_ok(), "{m}");
    }
    assert!(gc.add_mapping("not a mapping").is_err());
}

#[test]
fn auto_display_is_fullscreen_except_on_headless_drivers() {
    use crate::config::DisplayMode;
    use crate::core::resolve_display;
    for driver in ["kmsdrm", "KMSDRM", "x11", "wayland", "windows"] {
        assert_eq!(
            resolve_display(DisplayMode::Auto, driver),
            DisplayMode::Fullscreen,
            "{driver}"
        );
    }
    for driver in ["dummy", "offscreen"] {
        assert_eq!(
            resolve_display(DisplayMode::Auto, driver),
            DisplayMode::Windowed { scale: 2 },
            "{driver}"
        );
    }
    let w = DisplayMode::Windowed { scale: 3 };
    assert_eq!(
        resolve_display(w, "x11"),
        w,
        "環境変数で選んだウィンドウはそのまま"
    );
    assert_eq!(
        resolve_display(DisplayMode::Fullscreen, "dummy"),
        DisplayMode::Fullscreen
    );
    assert_eq!(
        crate::config::SdlConfig::dev_default().display,
        DisplayMode::Auto
    );
}

/// F11 はウィンドウのある環境(KMSDRM でない)なら開発用のキーが無効でもフルスクリーンを切り替える。ゲームには渡さない。
#[test]
fn f11_toggles_fullscreen_without_dev_hotkeys() {
    use sdl2::keyboard::Scancode;
    use sdl2::video::FullscreenType;
    let cfg = config(BatchMode::Geometry);
    assert!(!cfg.dev_hotkeys);
    with_platform(&cfg, test_assets(), |p| {
        let key = |sc| Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: None,
            scancode: Some(sc),
            keymod: sdl2::keyboard::Mod::NOMOD,
            repeat: false,
        };
        let mut out = Vec::new();
        assert_eq!(
            p.canvas_mut().window().fullscreen_state(),
            FullscreenType::Off
        );
        p.handle_event(key(Scancode::F11), &mut out);
        assert_ne!(
            p.canvas_mut().window().fullscreen_state(),
            FullscreenType::Off
        );
        p.handle_event(key(Scancode::F11), &mut out);
        assert_eq!(
            p.canvas_mut().window().fullscreen_state(),
            FullscreenType::Off
        );
        p.handle_event(key(Scancode::F12), &mut out);
        assert!(out.is_empty(), "F11・F12 はゲームの入力にならない: {out:?}");
    });
}

/// 最初からフルスクリーンでも、F11 で戻るウィンドウは `window_scale` の大きさ(既定 1280×720)。
#[test]
fn f11_from_fullscreen_returns_to_the_window_scale() {
    use sdl2::keyboard::Scancode;
    use sdl2::video::FullscreenType;
    let mut cfg = config(BatchMode::Geometry);
    cfg.display = crate::config::DisplayMode::Fullscreen;
    for (scale, size) in [(2, (1280, 720)), (3, (1920, 1080))] {
        cfg.window_scale = scale;
        with_platform(&cfg, test_assets(), |p| {
            assert_ne!(
                p.canvas_mut().window().fullscreen_state(),
                FullscreenType::Off
            );
            p.handle_event(f11(Scancode::F11), &mut Vec::new());
            assert_eq!(
                p.canvas_mut().window().fullscreen_state(),
                FullscreenType::Off
            );
            assert_eq!(p.canvas_mut().window().size(), size);
        });
    }
}

/// KMSDRM(実機)では開発用のキーが有効でも F11 を受け付けない。
#[test]
fn f11_is_ignored_on_kmsdrm() {
    use sdl2::keyboard::Scancode;
    use sdl2::video::FullscreenType;
    let mut cfg = config(BatchMode::Geometry);
    cfg.dev_hotkeys = true;
    with_platform(&cfg, test_assets(), |p| {
        p.act_as_kmsdrm();
        let mut out = Vec::new();
        p.handle_event(f11(Scancode::F11), &mut out);
        assert_eq!(
            p.canvas_mut().window().fullscreen_state(),
            FullscreenType::Off
        );
        assert!(out.is_empty());
    });
}

fn f11(sc: sdl2::keyboard::Scancode) -> Event {
    Event::KeyDown {
        timestamp: 0,
        window_id: 0,
        keycode: None,
        scancode: Some(sc),
        keymod: sdl2::keyboard::Mod::NOMOD,
        repeat: false,
    }
}
