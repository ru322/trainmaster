//! SDL のプラットフォームの設定 [14 §3.1]。

/// 表示の方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    /// 画面のない `dummy`・`offscreen` のドライバなら `Windowed { scale: 2 }`、それ以外(KMSDRM、開発機の X11・Wayland)はフルスクリーン。
    Auto,
    /// デスクトップの解像度のフルスクリーン。出力が 1920×1080 以上なら描画の画素をそのまま(整数倍)、小さければ縮めて写す。
    Fullscreen,
    /// 640×360 × `scale` のウィンドウ(3 で 1920×1080)。
    Windowed {
        /// 拡大の倍率。
        scale: u8,
    },
}

/// フレームの上限。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameCap {
    /// 60fps。
    Fps60,
    /// 30fps(vsync の 2 回に 1 回)。
    Fps30,
}

/// 四角形の描き方 [14 §3.3.2]。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchMode {
    /// `render_geometry` でまとめて描く(既定)。
    Geometry,
    /// 1 つずつ `copy` と `fill_rect` で描く(予備)。
    Copy,
}

/// 音のデバイスの設定。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioConfig {
    /// false なら音のデバイスを開かない。
    pub enabled: bool,
    /// バッファのサンプル数(既定 1024)。
    pub buffer_samples: u16,
    /// デバイスの名前。`None` は既定のデバイス。
    pub device: Option<String>,
}

/// `SdlCore::new` と `SdlPlatform::new` に渡す設定。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SdlConfig {
    /// 表示の方式。
    pub display: DisplayMode,
    /// フルスクリーンから F11 で戻るウィンドウの倍率(640×360 の何倍か。既定 2)。
    pub window_scale: u8,
    /// `SDL_RENDER_DRIVER` のヒント。`None` なら SDL に任せる。
    pub render_driver: Option<String>,
    /// vsync を使うか。
    pub vsync: bool,
    /// フレームの上限。
    pub frame_cap: FrameCap,
    /// 四角形の描き方。
    pub batch: BatchMode,
    /// 音。
    pub audio: AudioConfig,
    /// 開発用のキー(F12。F11 は KMSDRM でなければ常に使える)を有効にするか [14 §3.6.5]。
    pub dev_hotkeys: bool,
}

impl SdlConfig {
    /// 実機(ラズパイ)の既定。
    pub fn target_default() -> Self {
        SdlConfig {
            display: DisplayMode::Auto,
            render_driver: Some("opengles2".to_owned()),
            ..Self::dev_default()
        }
    }

    /// 開発機の既定(ウィンドウ。F11 でフルスクリーンと切り替え)。
    pub fn dev_default() -> Self {
        SdlConfig {
            display: DisplayMode::Windowed { scale: 2 },
            window_scale: 2,
            render_driver: None,
            vsync: true,
            frame_cap: FrameCap::Fps60,
            batch: BatchMode::Geometry,
            audio: AudioConfig {
                enabled: true,
                buffer_samples: 1024,
                device: None,
            },
            dev_hotkeys: cfg!(debug_assertions),
        }
    }
}
