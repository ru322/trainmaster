//! PoC: スプライト 2000 個・矩形・線・動くキャッシュ層を描き、BGM と効果音を鳴らし、`stats()` を 1 秒ごとに出す(14 §7.3)。Esc か Q で終わる。
//! 環境変数は 14 §3.1 の `TM_*`(`TM_DEV_HOTKEYS` を除く)と、`AV_POC_SPRITES`(数)・`AV_POC_FRAMES`(終わるフレーム数)・`AV_POC_DUMP`(最後のフレームの RGBA の出力先)・`AV_POC_MODE`(`stress` / `latency`)。
//! パッドの軸は、半分を超えた・戻ったときだけログに出す。
//! 実機での確認の手順は `scripts/av-poc-device.md`(音の詳しい手順は `scripts/av-poc-audio.md`)。

use std::process::ExitCode;
use std::time::Instant;

use tm_assets::font::{self, FontId};
use tm_assets::palette::{PAL_COIN, PAL_PANEL, PAL_WHITE};
use tm_assets::sound::{
    BGM_GAME_1, BGM_TITLE, JINGLE_COMPANY_UP, SE_BUY, SE_CANCEL, SE_COIN, SE_CONFIRM, SE_CURSOR,
    SE_ERROR, SE_POPUP, SE_UNIT_UP,
};
use tm_assets::sprite::SPRITE_COUNT;
use tm_platform_api::{
    AudioCmd, Color, DrawCmd, DrawList, Key, LayerId, PhysicalInput, Platform, PlatformError, Rect,
    SpriteId,
};
use tm_platform_sdl::{BatchMode, DisplayMode, FrameCap, SdlConfig, SdlCore, SdlPlatform};

const SE_CYCLE: [tm_assets::SoundId; 8] = [
    SE_CURSOR, SE_CONFIRM, SE_COIN, SE_BUY, SE_POPUP, SE_UNIT_UP, SE_CANCEL, SE_ERROR,
];
const BGM_CYCLE: [tm_assets::SoundId; 2] = [BGM_GAME_1, BGM_TITLE];
const MAP_W: u32 = 640;
const MAP_H: u32 = 288;

struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            eprintln!("[{}] {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn env_num<T: std::str::FromStr>(name: &str) -> Option<T> {
    env(name).and_then(|v| v.parse().ok())
}

fn config_from_env() -> SdlConfig {
    let kmsdrm = env("SDL_VIDEODRIVER").is_some_and(|v| v.eq_ignore_ascii_case("kmsdrm"));
    let mut cfg = if kmsdrm {
        SdlConfig::target_default()
    } else {
        SdlConfig::dev_default()
    };
    let scale = env_num("TM_WINDOW_SCALE").unwrap_or(2u8).clamp(1, 6);
    match env("TM_DISPLAY").as_deref() {
        Some("fullscreen") => cfg.display = DisplayMode::Fullscreen,
        Some("window") => cfg.display = DisplayMode::Windowed { scale },
        Some("auto") => cfg.display = DisplayMode::Auto,
        _ if !kmsdrm => cfg.display = DisplayMode::Windowed { scale },
        _ => {}
    }
    if env("TM_RENDER_BATCH").as_deref() == Some("copy") {
        cfg.batch = BatchMode::Copy;
    }
    if env("TM_FPS").as_deref() == Some("30") {
        cfg.frame_cap = FrameCap::Fps30;
    }
    if env("TM_VSYNC").as_deref() == Some("off") {
        cfg.vsync = false;
    }
    if env("TM_AUDIO").as_deref() == Some("off") {
        cfg.audio.enabled = false;
    }
    if let Some(n) = env_num("TM_AUDIO_BUFFER") {
        cfg.audio.buffer_samples = n;
    }
    match env("TM_RENDER_DRIVER").as_deref() {
        Some("auto") => cfg.render_driver = None,
        Some(driver) => cfg.render_driver = Some(driver.to_owned()),
        None => {}
    }
    cfg
}

struct Mover {
    x: i32,
    y: i32,
    vx: i32,
    vy: i32,
    id: SpriteId,
    w: u32,
    h: u32,
}

struct Rng(u32);

impl Rng {
    fn next(&mut self, n: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 % n
    }
}

fn movers(count: usize, sprites: &[tm_platform_api::SpriteRect]) -> Vec<Mover> {
    let mut rng = Rng(0x1234_5678);
    (0..count)
        .map(|i| {
            let id = (i % SPRITE_COUNT) as u16;
            let r = sprites[usize::from(id)];
            Mover {
                x: rng.next(600 * 16) as i32,
                y: rng.next(340 * 16) as i32,
                vx: rng.next(64) as i32 - 32,
                vy: rng.next(64) as i32 - 32,
                id: SpriteId(id),
                w: u32::from(r.w),
                h: u32::from(r.h),
            }
        })
        .collect()
}

fn step(m: &mut Mover) {
    let (max_x, max_y) = ((640 - m.w as i32) * 16, (360 - m.h as i32) * 16);
    m.x += m.vx;
    m.y += m.vy;
    if m.x < 0 || m.x > max_x {
        m.vx = -m.vx;
        m.x = m.x.clamp(0, max_x.max(0));
    }
    if m.y < 0 || m.y > max_y {
        m.vy = -m.vy;
        m.y = m.y.clamp(0, max_y.max(0));
    }
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

fn text(cmds: &mut Vec<DrawCmd>, s: &str, mut x: i32, y: i32, tint: Color) {
    for ch in s.chars() {
        let g =
            font::glyph(FontId::Body12, ch).unwrap_or_else(|| font::fallback_glyph(FontId::Body12));
        if let Some(b) = g.bitmap {
            cmds.push(DrawCmd::Sprite {
                id: b.sprite,
                dst: Rect {
                    x: x + i32::from(b.x_off),
                    y: y + i32::from(b.y_off),
                    w: u32::from(b.w),
                    h: u32::from(b.h),
                },
                tint,
            });
        }
        x += i32::from(g.advance);
    }
}

fn map_layer(cmds: &mut Vec<DrawCmd>, revision: u64, cam_x: i32) {
    cmds.push(DrawCmd::BeginCachedLayer {
        layer: LayerId(0),
        revision,
        x: -cam_x,
        y: 32,
        w: MAP_W,
        h: MAP_H,
    });
    cmds.push(DrawCmd::Clear(rgba(40, 92, 52, 255)));
    let grid = rgba(60, 120, 72, 255);
    for gx in (0..MAP_W as i32).step_by(16) {
        cmds.push(DrawCmd::Line {
            x0: gx,
            y0: 0,
            x1: gx,
            y1: MAP_H as i32 - 1,
            color: grid,
        });
    }
    for gy in (0..MAP_H as i32).step_by(16) {
        cmds.push(DrawCmd::Line {
            x0: 0,
            y0: gy,
            x1: MAP_W as i32 - 1,
            y1: gy,
            color: grid,
        });
    }
    let shade = (revision % 4) as u8 * 50;
    cmds.push(DrawCmd::FillRect {
        rect: Rect {
            x: 32,
            y: 32,
            w: 576,
            h: 8,
        },
        color: rgba(90, 90, 90 + shade, 255),
    });
    cmds.push(DrawCmd::FillRect {
        rect: Rect {
            x: 32,
            y: 32,
            w: 8,
            h: 224,
        },
        color: rgba(90, 90, 90 + shade, 255),
    });
    text(cmds, &format!("layer rev {revision}"), 48, 64, PAL_WHITE);
    cmds.push(DrawCmd::EndCachedLayer);
    cmds.push(DrawCmd::DrawCachedLayer { layer: LayerId(0) });
}

fn run(p: &mut SdlPlatform<'_>) -> Result<(), PlatformError> {
    let bundle = tm_assets::bundle().map_err(|e| PlatformError::Assets(format!("{e:?}")))?;
    p.load_assets(bundle)?;
    let count = env_num("AV_POC_SPRITES").unwrap_or(2000usize);
    let max_frames = env_num::<u64>("AV_POC_FRAMES");
    let mut sprites = movers(count, bundle.sprites);
    let mut list = DrawList::default();
    let mut inputs = Vec::new();
    let mut axes = std::collections::HashMap::new();
    let latency = env("AV_POC_MODE").as_deref() == Some("latency");
    let mut audio = Vec::new();
    if !latency {
        audio.push(AudioCmd::PlayBgm {
            id: BGM_TITLE,
            crossfade_ms: 0,
        });
    }
    let start = p.now();
    let (mut next_se, mut next_stats, mut next_jingle, mut next_bgm) = (
        start.0,
        start.0 + 1_000_000,
        start.0 + 10_000_000,
        start.0 + 15_000_000,
    );
    let (mut se_i, mut bgm_i, mut frames, mut frames_at_stats) = (0usize, 0usize, 0u64, 0u64);
    let (mut flash, mut audio_max_us) = (false, 0u128);
    let target_us: u32 = if env("TM_FPS").as_deref() == Some("30") {
        33_333
    } else {
        16_667
    };
    let (mut interval_max, mut render_max, mut slow) = (0u32, 0u32, 0u32);
    log::info!(
        "av_poc: pid {}、スプライト {count} 個、{}。Esc か Q で終わる",
        std::process::id(),
        if latency {
            "遅れの計測(1 秒ごとと Space・Enter・パッドのボタンで、画面の白いフラッシュと se_cursor)"
        } else {
            "BGM・効果音 100ms ごと・ジングル 10 秒ごと・BGM のクロスフェード 15 秒目から 20 秒ごと"
        }
    );
    loop {
        inputs.clear();
        p.poll_input(&mut inputs);
        for i in &inputs {
            let changed = match *i {
                PhysicalInput::PadAxis { pad, axis, value } => {
                    let side = (value / 16384).signum();
                    axes.insert((pad, axis), side) != Some(side)
                }
                _ => true,
            };
            if changed {
                log::info!("入力: {i:?}");
            }
            if matches!(
                i,
                PhysicalInput::QuitRequested
                    | PhysicalInput::KeyDown(Key::Escape)
                    | PhysicalInput::KeyDown(Key::Q)
            ) {
                return finish(p, frames);
            }
            flash |= latency
                && matches!(
                    i,
                    PhysicalInput::KeyDown(Key::Space | Key::Enter)
                        | PhysicalInput::PadButtonDown { .. }
                );
        }
        let now = p.now().0;
        let t_ms = (now - start.0) / 1000;
        if latency {
            if now >= next_se {
                flash = true;
                next_se = now + 1_000_000;
            }
            if flash {
                audio.push(AudioCmd::PlaySe(SE_CURSOR));
            }
        } else {
            while now >= next_se {
                audio.push(AudioCmd::PlaySe(SE_CYCLE[se_i % SE_CYCLE.len()]));
                se_i += 1;
                next_se += 100_000;
            }
            if now >= next_jingle {
                audio.push(AudioCmd::PlayJingle(JINGLE_COMPANY_UP));
                next_jingle += 10_000_000;
            }
            if now >= next_bgm {
                audio.push(AudioCmd::PlayBgm {
                    id: BGM_CYCLE[bgm_i % BGM_CYCLE.len()],
                    crossfade_ms: 1000,
                });
                bgm_i += 1;
                next_bgm += 20_000_000;
            }
        }

        list.cmds.clear();
        list.cmds.push(DrawCmd::Clear(PAL_PANEL));
        let cam = ((t_ms / 20) % 448) as i32;
        let cam_x = if cam < 224 { cam } else { 447 - cam };
        map_layer(&mut list.cmds, t_ms / 5000, cam_x);
        list.cmds.push(DrawCmd::FillRect {
            rect: Rect {
                x: 0,
                y: 0,
                w: 640,
                h: 32,
            },
            color: rgba(20, 28, 52, 255),
        });
        list.cmds.push(DrawCmd::FillRect {
            rect: Rect {
                x: 416,
                y: 32,
                w: 224,
                h: 288,
            },
            color: rgba(255, 255, 255, 96),
        });
        let (cx, cy) = (575, 200);
        for (dx, dy) in [
            (50, 0),
            (50, 25),
            (35, 35),
            (25, 50),
            (0, 50),
            (-25, 50),
            (-35, 35),
            (-50, 25),
        ] {
            list.cmds.push(DrawCmd::Line {
                x0: cx,
                y0: cy,
                x1: cx + dx,
                y1: cy + dy,
                color: PAL_COIN,
            });
            list.cmds.push(DrawCmd::Line {
                x0: cx,
                y0: cy,
                x1: cx - dx,
                y1: cy - dy,
                color: PAL_COIN,
            });
        }
        for m in &mut sprites {
            step(m);
            list.cmds.push(DrawCmd::Sprite {
                id: m.id,
                dst: Rect {
                    x: m.x / 16,
                    y: m.y / 16,
                    w: m.w,
                    h: m.h,
                },
                tint: PAL_WHITE,
            });
        }
        let s = p.stats();
        text(
            &mut list.cmds,
            &format!(
                "Train Master av_poc  {}us  quads {}  Esc/Q",
                s.frame_interval_us, s.draw_quads
            ),
            8,
            28,
            PAL_COIN,
        );
        if std::mem::take(&mut flash) {
            list.cmds.push(DrawCmd::FillRect {
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 640,
                    h: 360,
                },
                color: PAL_WHITE,
            });
        }

        let t = Instant::now();
        p.audio(&audio);
        audio_max_us = audio_max_us.max(t.elapsed().as_micros());
        audio.clear();
        p.present(&list)?;
        frames += 1;
        let s = p.stats();
        if frames > 1 {
            interval_max = interval_max.max(s.frame_interval_us);
            slow += u32::from(s.frame_interval_us > target_us * 3 / 2);
        }
        render_max = render_max.max(p.render_us());

        let now = p.now().0;
        if now >= next_stats {
            let s = p.stats();
            log::info!(
                "{} fps、間隔 {}us、present {}us(描画 {}us)、1 秒の最大 間隔 {}us・描画 {}us、遅れたフレーム {}、四角形 {}、層の描き直し {}、音 {:?}、コールバック {:?}、audio() 最大 {}us",
                frames - frames_at_stats,
                s.frame_interval_us,
                s.present_us,
                p.render_us(),
                std::mem::take(&mut interval_max),
                std::mem::take(&mut render_max),
                std::mem::take(&mut slow),
                s.draw_quads,
                s.layer_redraws,
                s.audio,
                p.audio_callback_count(),
                std::mem::take(&mut audio_max_us)
            );
            frames_at_stats = frames;
            next_stats += 1_000_000;
        }
        if max_frames.is_some_and(|n| frames >= n) {
            return finish(p, frames);
        }
    }
}

fn finish(p: &mut SdlPlatform<'_>, frames: u64) -> Result<(), PlatformError> {
    if let Some(path) = env("AV_POC_DUMP") {
        let px = p.capture_frame()?;
        std::fs::write(&path, px).map_err(|e| PlatformError::Render(e.to_string()))?;
        log::info!("{path} に 640×360 の RGBA を書いた");
    }
    log::info!("{frames} フレームで終わる");
    Ok(())
}

/// PL-V-15・BV7: 表示モードと、640×360 を何倍で出すか。
fn log_display() {
    let video = match sdl2::init().and_then(|s| s.video()) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("表示モードを調べられない: {e}");
            return;
        }
    };
    let n = video.num_video_displays().unwrap_or(0);
    for i in 0..n {
        let name = video.display_name(i).unwrap_or_default();
        match (video.desktop_display_mode(i), video.current_display_mode(i)) {
            (Ok(d), Ok(c)) => {
                let scale = (d.w / 640).min(d.h / 360);
                log::info!(
                    "表示 {i} {name}: デスクトップ {}×{}@{}Hz、いま {}×{}@{}Hz、{scale} 倍({}×{}、余白 左右 {}・上下 {})",
                    d.w,
                    d.h,
                    d.refresh_rate,
                    c.w,
                    c.h,
                    c.refresh_rate,
                    640 * scale,
                    360 * scale,
                    (d.w - 640 * scale) / 2,
                    (d.h - 360 * scale) / 2
                );
            }
            (Err(e), _) | (_, Err(e)) => log::warn!("表示 {i} {name}: {e}"),
        }
    }
}

fn main() -> ExitCode {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let cfg = config_from_env();
    let core = match SdlCore::new(&cfg) {
        Ok(c) => c,
        Err(e) => {
            log::error!("{e:?}");
            return ExitCode::FAILURE;
        }
    };
    log_display();
    let tc = core.texture_creator();
    let mut platform = match SdlPlatform::new(core, &tc, &cfg) {
        Ok(p) => p,
        Err(e) => {
            log::error!("{e:?}");
            return ExitCode::FAILURE;
        }
    };
    let code = match run(&mut platform) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e:?}");
            ExitCode::FAILURE
        }
    };
    // PL-V-06 (2): 14 §3.8 の順(platform → tc)で後片付けされることをログで確かめる
    log::info!("drop(platform) の前");
    drop(platform);
    log::info!("drop(platform) の後、drop(tc) の前");
    drop(tc);
    log::info!("drop(tc) の後(SDL_Quit まで済んだ)");
    code
}
