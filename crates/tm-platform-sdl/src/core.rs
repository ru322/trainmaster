//! SDL のコンテキスト、サブシステム、ウィンドウのキャンバス、イベントポンプ [14 §3.2]。

use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::{EventPump, GameControllerSubsystem, JoystickSubsystem, Sdl, VideoSubsystem};
use tm_platform_api::{LOGICAL_HEIGHT, LOGICAL_WIDTH, PlatformError};

use crate::config::{DisplayMode, SdlConfig};
use crate::pad_mappings::PAD_MAPPINGS;

const WINDOW_TITLE: &str = "Train Master";

/// SDL の初期化の前半(14 §3.2 の手順 1〜4 と 6)。`TextureCreator` を作る。
pub struct SdlCore {
    pub(crate) canvas: Canvas<Window>,
    pub(crate) event_pump: EventPump,
    pub(crate) pads: Option<(GameControllerSubsystem, JoystickSubsystem)>,
    pub(crate) sdl: Sdl,
}

impl SdlCore {
    /// ヒント、SDL、ウィンドウ、キャンバス、ゲームパッドのサブシステムを用意する。
    pub fn new(cfg: &SdlConfig) -> Result<SdlCore, PlatformError> {
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
        sdl2::hint::set("SDL_RENDER_BATCHING", "1");
        if let Some(driver) = &cfg.render_driver {
            sdl2::hint::set("SDL_RENDER_DRIVER", driver);
        }
        let sdl = sdl2::init().map_err(PlatformError::Init)?;
        let video = sdl.video().map_err(PlatformError::Init)?;
        let event_pump = sdl.event_pump().map_err(PlatformError::Init)?;
        let driver = video.current_video_driver();
        let display = resolve_display(cfg.display, driver);
        let canvas = match build_canvas(&video, display, cfg.vsync, false) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("アクセラレーションのレンダラを作れないのでソフトウェアにする: {e}");
                build_canvas(&video, display, cfg.vsync, true).map_err(PlatformError::Init)?
            }
        };
        sdl.mouse().show_cursor(false);
        let pads = open_pad_subsystems(&sdl);
        let info = canvas.info();
        let (win_w, win_h) = canvas.window().size();
        let output = canvas
            .output_size()
            .map_or_else(|e| format!("不明({e})"), |(w, h)| format!("{w}×{h}"));
        log::info!(
            "SDL {}: ビデオ {driver}、レンダラ {}、表示 {display:?}、ウィンドウ {win_w}×{win_h}、出力 {output}、最大テクスチャ {}×{}",
            sdl2::version::version(),
            info.name,
            info.max_texture_width,
            info.max_texture_height
        );
        Ok(SdlCore {
            canvas,
            event_pump,
            pads,
            sdl,
        })
    }

    /// キャンバスの `TextureCreator`。`main` のスタックに置き、`SdlPlatform::new` に貸す。
    pub fn texture_creator(&self) -> TextureCreator<WindowContext> {
        self.canvas.texture_creator()
    }
}

pub(crate) fn resolve_display(mode: DisplayMode, video_driver: &str) -> DisplayMode {
    match mode {
        DisplayMode::Auto if video_driver.eq_ignore_ascii_case("kmsdrm") => DisplayMode::Fullscreen,
        DisplayMode::Auto => DisplayMode::Windowed { scale: 2 },
        other => other,
    }
}

fn build_canvas(
    video: &VideoSubsystem,
    display: DisplayMode,
    vsync: bool,
    software: bool,
) -> Result<Canvas<Window>, String> {
    let window = match display {
        DisplayMode::Fullscreen => {
            let mode = video.desktop_display_mode(0)?;
            let (w, h) = (mode.w.max(1) as u32, mode.h.max(1) as u32);
            video
                .window(WINDOW_TITLE, w, h)
                .fullscreen_desktop()
                .build()
        }
        DisplayMode::Windowed { .. } | DisplayMode::Auto => {
            let s = match display {
                DisplayMode::Windowed { scale } => u32::from(scale.max(1)),
                _ => 2,
            };
            video
                .window(WINDOW_TITLE, LOGICAL_WIDTH * s, LOGICAL_HEIGHT * s)
                .position_centered()
                .build()
        }
    }
    .map_err(|e| e.to_string())?;
    let builder = window.into_canvas();
    let builder = if software {
        builder.software()
    } else {
        builder.accelerated()
    };
    let builder = builder.target_texture();
    let builder = if vsync {
        builder.present_vsync()
    } else {
        builder
    };
    builder.build().map_err(|e| e.to_string())
}

fn open_pad_subsystems(sdl: &Sdl) -> Option<(GameControllerSubsystem, JoystickSubsystem)> {
    let pads = sdl
        .game_controller()
        .and_then(|gc| Ok((gc, sdl.joystick()?)));
    match pads {
        Ok((gc, joy)) => {
            for mapping in PAD_MAPPINGS {
                if let Err(e) = gc.add_mapping(mapping) {
                    log::warn!("ゲームパッドのマッピングを登録できない: {e}");
                }
            }
            Some((gc, joy))
        }
        Err(e) => {
            log::warn!("ゲームパッドのサブシステムを開けない(キーボードだけで動く): {e}");
            None
        }
    }
}
