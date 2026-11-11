//! SDL の dummy ドライバでのスモーク(14 §9.4 の R-01 相当と、入力・時計・音・終了)。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use sdl2::event::Event;
use sdl2::keyboard::{Mod, Scancode};
use tm_platform_api::{
    AssetBundle, AtlasImage, AudioCmd, Color, DrawCmd, DrawList, Key, LayerId, PhysicalInput,
    Platform, PlatformError, Rect, SoundData, SoundId, SoundKind, SpriteId, SpriteRect,
};
use tm_platform_sdl::{BatchMode, SdlConfig, SdlCore, SdlPlatform};

static SDL_LOCK: Mutex<()> = Mutex::new(());

const W: u8 = 255;
#[rustfmt::skip]
static PIXELS: [u8; 8 * 2 * 4] = [
    W, W, W, W,  W, W, W, W,  W, 0, 0, W,  0, W, 0, W,  0, 0, 0, 0,  0, 0, 0, 0,  0, 0, 0, 0,  0, 0, 0, 0,
    W, W, W, W,  W, W, W, W,  0, 0, W, W,  W, W, 0, W,  0, 0, 0, 0,  0, 0, 0, 0,  0, 0, 0, 0,  0, 0, 0, 0,
];
static ATLASES: [AtlasImage; 1] = [AtlasImage {
    width: 8,
    height: 2,
    rgba: &PIXELS,
}];
static SPRITES: [SpriteRect; 2] = [
    SpriteRect {
        atlas: 0,
        x: 0,
        y: 0,
        w: 2,
        h: 2,
    },
    SpriteRect {
        atlas: 0,
        x: 2,
        y: 0,
        w: 2,
        h: 2,
    },
];
static TONE: [i16; 2205] = [0; 2205];
static SOUNDS: [SoundData; 2] = [
    SoundData {
        name: "bgm_test",
        kind: SoundKind::Bgm,
        samples: &TONE,
        loop_start: Some(0),
    },
    SoundData {
        name: "se_test",
        kind: SoundKind::Se,
        samples: &TONE,
        loop_start: None,
    },
];
static BUNDLE: AssetBundle = AssetBundle {
    atlases: &ATLASES,
    sprites: &SPRITES,
    sounds: &SOUNDS,
    white: SpriteId(0),
};

const BG: Color = rgba(0, 0, 64, 255);
const RED: Color = rgba(255, 0, 0, 255);
const GREEN: Color = rgba(0, 255, 0, 255);
const BLUE: Color = rgba(0, 0, 255, 255);
const YELLOW: Color = rgba(255, 255, 0, 255);
const GRAY: Color = rgba(100, 100, 100, 255);
const WHITE: Color = rgba(255, 255, 255, 255);

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

fn config(batch: BatchMode, vsync: bool, audio: bool) -> SdlConfig {
    let mut cfg = SdlConfig::dev_default();
    cfg.batch = batch;
    cfg.vsync = vsync;
    cfg.audio.enabled = audio;
    cfg.dev_hotkeys = false;
    cfg
}

/// SDL を使うテストを 1 つずつ動かし、終わったら SDL が後片付けまで済んだことを確かめる。
fn with_platform(cfg: &SdlConfig, f: impl FnOnce(&mut SdlPlatform<'_>)) {
    let _guard = SDL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    sdl2::hint::set("SDL_VIDEODRIVER", "dummy");
    sdl2::hint::set("SDL_AUDIODRIVER", "dummy");
    {
        let core = SdlCore::new(cfg).expect("SdlCore::new");
        let tc = core.texture_creator();
        let mut platform = SdlPlatform::new(core, &tc, cfg).expect("SdlPlatform::new");
        f(&mut platform);
    }
    let reinit = std::thread::spawn(|| sdl2::init().map(drop))
        .join()
        .unwrap();
    assert_eq!(reinit, Ok(()), "SDL_Quit まで後片付けされていない");
}

fn layer_cmds(revision: u64, x: i32, w: u32) -> [DrawCmd; 5] {
    [
        DrawCmd::BeginCachedLayer {
            layer: LayerId(0),
            revision,
            x,
            y: 200,
            w,
            h: 8,
        },
        DrawCmd::Clear(GRAY),
        DrawCmd::FillRect {
            rect: rect(-4, 2, 8, 2),
            color: RED,
        },
        DrawCmd::EndCachedLayer,
        DrawCmd::DrawCachedLayer { layer: LayerId(0) },
    ]
}

fn scene(layer: [DrawCmd; 5]) -> DrawList {
    let mut cmds = vec![
        DrawCmd::Clear(BG),
        DrawCmd::FillRect {
            rect: rect(10, 10, 20, 10),
            color: RED,
        },
        DrawCmd::FillRect {
            rect: rect(40, 10, 20, 10),
            color: rgba(255, 255, 255, 128),
        },
        DrawCmd::FillRect {
            rect: rect(70, 10, 0, 10),
            color: RED,
        },
        DrawCmd::Line {
            x0: 100,
            y0: 5,
            x1: 110,
            y1: 5,
            color: GREEN,
        },
        DrawCmd::Line {
            x0: 100,
            y0: 20,
            x1: 105,
            y1: 25,
            color: GREEN,
        },
        DrawCmd::Sprite {
            id: SpriteId(1),
            dst: rect(200, 100, 4, 4),
            tint: WHITE,
        },
        DrawCmd::Sprite {
            id: SpriteId(1),
            dst: rect(210, 100, 4, 4),
            tint: rgba(0, 255, 255, 255),
        },
        DrawCmd::Sprite {
            id: SpriteId(1),
            dst: rect(220, 100, 4, 4),
            tint: rgba(255, 255, 255, 0),
        },
    ];
    cmds.extend(layer);
    DrawList { cmds }
}

fn px(frame: &[u8], x: usize, y: usize) -> Color {
    let i = (y * 640 + x) * 4;
    rgba(frame[i], frame[i + 1], frame[i + 2], frame[i + 3])
}

fn assert_px(frame: &[u8], x: usize, y: usize, want: Color) {
    let got = px(frame, x, y);
    let close = |a: u8, b: u8| a.abs_diff(b) <= 2;
    assert!(
        close(got.r, want.r) && close(got.g, want.g) && close(got.b, want.b),
        "({x}, {y}): {got:?} != {want:?}"
    );
}

fn check_scene(frame: &[u8]) {
    assert_eq!(frame.len(), 640 * 360 * 4);
    assert_px(frame, 0, 0, BG);
    assert_px(frame, 639, 359, BG);
    assert_px(frame, 10, 10, RED);
    assert_px(frame, 29, 19, RED);
    assert_px(frame, 30, 19, BG);
    assert_px(frame, 9, 10, BG);
    assert_px(frame, 45, 15, rgba(128, 128, 160, 255));
    assert_px(frame, 70, 10, BG);
    for x in 100..=110 {
        assert_px(frame, x, 5, GREEN);
    }
    assert_px(frame, 111, 5, BG);
    assert_px(frame, 99, 5, BG);
    for i in 0..=5 {
        assert_px(frame, 100 + i, 20 + i, GREEN);
    }
    assert_px(frame, 101, 20, BG);
    for (dx, dy, c) in [(0, 0, RED), (2, 0, GREEN), (0, 2, BLUE), (2, 2, YELLOW)] {
        assert_px(frame, 200 + dx, 100 + dy, c);
        assert_px(frame, 201 + dx, 101 + dy, c);
    }
    assert_px(frame, 204, 100, BG);
    assert_px(frame, 210, 100, rgba(0, 0, 0, 255));
    assert_px(frame, 212, 100, GREEN);
    assert_px(frame, 220, 100, BG);
    assert_px(frame, 300, 200, GRAY);
    assert_px(frame, 315, 207, GRAY);
    assert_px(frame, 300, 202, RED);
    assert_px(frame, 303, 203, RED);
    assert_px(frame, 304, 202, GRAY);
    assert_px(frame, 299, 202, BG);
    assert_px(frame, 316, 200, BG);
    assert_px(frame, 300, 208, BG);
}

#[test]
fn r01_draw_with_geometry_and_layer_cache() {
    with_platform(&config(BatchMode::Geometry, false, false), |p| {
        assert_eq!(p.logical_size(), (640, 360));
        p.present(&DrawList::default())
            .expect("load_assets の前の present");
        p.load_assets(&BUNDLE).expect("load_assets");
        assert!(matches!(
            p.load_assets(&BUNDLE),
            Err(PlatformError::Assets(_))
        ));

        p.present(&scene(layer_cmds(1, 300, 16))).expect("present");
        check_scene(&p.capture_frame().expect("capture"));
        let s = p.stats();
        assert_eq!(s.layer_redraws, 1);
        assert_eq!(s.draw_quads, 2 + 1 + 6 + 3 + 1);

        p.present(&scene(layer_cmds(1, 300, 16))).unwrap();
        assert_eq!(p.stats().layer_redraws, 1);
        p.present(&scene(layer_cmds(1, 310, 16))).unwrap();
        assert_eq!(p.stats().layer_redraws, 1);
        assert_px(&p.capture_frame().unwrap(), 310, 200, GRAY);
        p.present(&scene(layer_cmds(2, 300, 16))).unwrap();
        assert_eq!(p.stats().layer_redraws, 2);
        p.present(&scene(layer_cmds(2, 300, 17))).unwrap();
        assert_eq!(p.stats().layer_redraws, 3);
        p.present(&scene(layer_cmds(2, 300, 16))).unwrap();
        check_scene(&p.capture_frame().unwrap());
    });
}

#[test]
fn r01_copy_mode_draws_the_same() {
    let mut list = scene(layer_cmds(1, 300, 16));
    for (i, (dx, dy)) in [
        (9, 0),
        (9, 4),
        (6, 6),
        (4, 9),
        (0, 9),
        (-4, 9),
        (-6, 6),
        (-9, 4),
    ]
    .into_iter()
    .enumerate()
    {
        let (x0, y0) = (40 + 25 * i as i32, 300);
        list.cmds.push(DrawCmd::Line {
            x0,
            y0,
            x1: x0 + dx,
            y1: y0 + dy,
            color: YELLOW,
        });
    }
    list.cmds.push(DrawCmd::Sprite {
        id: SpriteId(1),
        dst: rect(400, 300, 6, 10),
        tint: rgba(255, 255, 255, 160),
    });
    let mut frames = Vec::new();
    for batch in [BatchMode::Geometry, BatchMode::Copy] {
        with_platform(&config(batch, false, false), |p| {
            p.load_assets(&BUNDLE).unwrap();
            p.present(&list).unwrap();
            frames.push(p.capture_frame().unwrap());
        });
    }
    check_scene(&frames[1]);
    for (i, (a, b)) in frames[0].iter().zip(&frames[1]).enumerate() {
        assert!(a.abs_diff(*b) <= 2 || i % 4 == 3, "画素 {}: {a} {b}", i / 4);
    }
}

#[test]
fn draw_cached_layer_without_begin_draws_nothing() {
    with_platform(&config(BatchMode::Geometry, false, false), |p| {
        p.load_assets(&BUNDLE).unwrap();
        p.present(&scene(layer_cmds(1, 300, 16))).unwrap();
        let mut list = scene(layer_cmds(1, 300, 16));
        list.cmds.truncate(list.cmds.len() - 5);
        list.cmds
            .push(DrawCmd::DrawCachedLayer { layer: LayerId(0) });
        p.present(&list).unwrap();
        let f = p.capture_frame().unwrap();
        assert_px(&f, 300, 200, BG);
        assert_px(&f, 10, 10, RED);
    });
}

#[test]
fn r01_oversized_layer_is_drawn_inline() {
    with_platform(&config(BatchMode::Geometry, false, false), |p| {
        p.load_assets(&BUNDLE).unwrap();
        p.present(&scene(layer_cmds(1, 300, 1025))).unwrap();
        let f = p.capture_frame().unwrap();
        assert_eq!(p.stats().layer_redraws, 0);
        assert_px(&f, 300, 200, GRAY);
        assert_px(&f, 639, 207, GRAY);
        assert_px(&f, 300, 202, RED);
        assert_px(&f, 299, 202, BG);
        assert_px(&f, 300, 208, BG);
        assert_px(&f, 10, 300, BG);
    });
}

#[test]
fn input_from_sdl_queue() {
    with_platform(&config(BatchMode::Geometry, false, false), |p| {
        let sdl = sdl2::init().unwrap();
        let events = sdl.event().unwrap();
        let key = |down: bool, repeat: bool| {
            let (timestamp, window_id, keycode, scancode, keymod) =
                (0, 0, None, Some(Scancode::A), Mod::NOMOD);
            if down {
                Event::KeyDown {
                    timestamp,
                    window_id,
                    keycode,
                    scancode,
                    keymod,
                    repeat,
                }
            } else {
                Event::KeyUp {
                    timestamp,
                    window_id,
                    keycode,
                    scancode,
                    keymod,
                    repeat,
                }
            }
        };
        let mut out = vec![PhysicalInput::KeyDown(Key::Z)];
        p.poll_input(&mut out);
        out.truncate(1);
        for e in [
            key(true, false),
            key(true, true),
            key(false, false),
            Event::Quit { timestamp: 0 },
        ] {
            events.push_event(e).unwrap();
        }
        p.poll_input(&mut out);
        assert_eq!(
            out,
            [
                PhysicalInput::KeyDown(Key::Z),
                PhysicalInput::KeyDown(Key::A),
                PhysicalInput::KeyUp(Key::A),
                PhysicalInput::QuitRequested,
            ]
        );
        out.clear();
        p.poll_input(&mut out);
        assert!(out.is_empty());
    });
}

#[test]
fn clock_and_software_frame_limit() {
    with_platform(&config(BatchMode::Geometry, false, false), |p| {
        p.load_assets(&BUNDLE).unwrap();
        let t0 = p.now();
        let start = Instant::now();
        for _ in 0..10 {
            p.present(&scene(layer_cmds(1, 300, 16))).unwrap();
        }
        let t1 = p.now();
        assert!(t1 > t0);
        assert!(
            start.elapsed() >= Duration::from_micros(9 * 16_667),
            "{:?}",
            start.elapsed()
        );
        let s = p.stats();
        assert!(s.frame_interval_us > 0, "{s:?}");
        assert!(s.present_us <= s.frame_interval_us, "{s:?}");
    });
}

#[test]
fn clock_falls_back_when_vsync_does_not_wait() {
    with_platform(&config(BatchMode::Geometry, true, false), |p| {
        p.load_assets(&BUNDLE).unwrap();
        let list = scene(layer_cmds(1, 300, 16));
        for _ in 0..121 {
            p.present(&list).unwrap();
        }
        let start = Instant::now();
        for _ in 0..10 {
            p.present(&list).unwrap();
        }
        assert!(
            start.elapsed() >= Duration::from_micros(9 * 16_667),
            "{:?}",
            start.elapsed()
        );
    });
}

#[test]
fn audio_callback_runs_and_reports() {
    with_platform(&config(BatchMode::Geometry, false, true), |p| {
        assert!(!p.stats().audio.enabled);
        p.load_assets(&BUNDLE).unwrap();
        assert!(p.stats().audio.enabled);
        p.audio(&[
            AudioCmd::PlayBgm {
                id: SoundId(0),
                crossfade_ms: 0,
            },
            AudioCmd::PlaySe(SoundId(1)),
            AudioCmd::PlaySe(SoundId(0)),
        ]);
        let start = Instant::now();
        while p.audio_callback_count().unwrap() < 3 {
            assert!(
                start.elapsed() < Duration::from_secs(3),
                "コールバックが呼ばれない"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(1100).saturating_sub(start.elapsed()));
        p.audio(&[]);
        let a = p.stats().audio;
        assert_eq!(a.invalid_cmds, 1, "{a:?}");
        assert_eq!(a.dropped_cmds, 0);
        assert!(a.max_callback_gap_us > 0, "{a:?}");
    });
}

#[test]
fn audio_disabled_opens_no_device() {
    with_platform(&config(BatchMode::Geometry, false, false), |p| {
        p.load_assets(&BUNDLE).unwrap();
        assert_eq!(p.audio_callback_count(), None);
        p.audio(&[AudioCmd::PlaySe(SoundId(1))]);
        assert!(!p.stats().audio.enabled);
    });
}

#[test]
fn invalid_bundle_is_rejected() {
    static BAD: AssetBundle = AssetBundle {
        atlases: &ATLASES,
        sprites: &SPRITES,
        sounds: &SOUNDS,
        white: SpriteId(2),
    };
    with_platform(&config(BatchMode::Geometry, false, false), |p| {
        assert!(matches!(p.load_assets(&BAD), Err(PlatformError::Assets(_))));
    });
}
