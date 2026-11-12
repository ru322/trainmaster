//! `PlatformEnv` → `SdlConfig` と、起動の設定 `LaunchConfig` [16 §1.4、§1.12]。

use tm_app::{AppConfig, AppError, ConfigWarning, DisplayChoice, PlatformEnv};
use tm_platform_sdl::{BatchMode, DisplayMode, FrameCap, SdlConfig};

/// lib の `run` に渡す設定と、SDL のプラットフォームに渡す設定。
pub struct LaunchConfig {
    pub app: AppConfig,
    pub sdl: SdlConfig,
}

impl LaunchConfig {
    /// 環境変数から作る。`Err` は開発用の表の差し替えの失敗だけ。
    pub fn from_env(
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Result<(LaunchConfig, Vec<ConfigWarning>), AppError> {
        let (app, penv, warnings) = AppConfig::from_env(vars)?;
        Ok((
            LaunchConfig {
                app,
                sdl: to_sdl_config(&penv),
            },
            warnings,
        ))
    }
}

/// 実機(`kmsdrm`)か開発機の既定に、環境変数の値を重ねる。
pub fn to_sdl_config(p: &PlatformEnv) -> SdlConfig {
    let mut c = if p.target {
        SdlConfig::target_default()
    } else {
        SdlConfig::dev_default()
    };
    c.display = match p.display {
        DisplayChoice::Auto => DisplayMode::Auto,
        DisplayChoice::Fullscreen => DisplayMode::Fullscreen,
        DisplayChoice::Windowed { scale } => DisplayMode::Windowed { scale },
    };
    c.window_scale = p.window_scale;
    c.frame_cap = if p.fps30 {
        FrameCap::Fps30
    } else {
        FrameCap::Fps60
    };
    c.vsync = p.vsync;
    c.batch = if p.copy_batch {
        BatchMode::Copy
    } else {
        BatchMode::Geometry
    };
    c.render_driver = p.render_driver.clone();
    c.audio.enabled = p.audio;
    c.audio.buffer_samples = p.audio_buffer;
    if let Some(on) = p.dev_hotkeys {
        c.dev_hotkeys = on;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(pairs: &[(&str, &str)]) -> SdlConfig {
        let vars = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string()));
        LaunchConfig::from_env(vars).expect("from_env").0.sdl
    }

    #[test]
    fn kmsdrm_uses_the_target_defaults() {
        assert_eq!(
            launch(&[("SDL_VIDEODRIVER", "kmsdrm")]),
            SdlConfig::target_default()
        );
    }

    #[test]
    fn dev_defaults_and_overrides() {
        assert_eq!(launch(&[]), SdlConfig::dev_default());
        let c = launch(&[
            ("TM_DISPLAY", "fullscreen"),
            ("TM_FPS", "30"),
            ("TM_VSYNC", "off"),
            ("TM_RENDER_BATCH", "copy"),
            ("TM_RENDER_DRIVER", "software"),
            ("TM_AUDIO", "off"),
            ("TM_AUDIO_BUFFER", "4096"),
        ]);
        assert_eq!(c.display, DisplayMode::Fullscreen);
        assert_eq!(c.frame_cap, FrameCap::Fps30);
        assert!(!c.vsync);
        assert_eq!(c.batch, BatchMode::Copy);
        assert_eq!(c.render_driver.as_deref(), Some("software"));
        assert!(!c.audio.enabled);
        assert_eq!(c.audio.buffer_samples, 4096);
        assert_eq!(
            launch(&[("TM_DISPLAY", "window"), ("TM_WINDOW_SCALE", "4")]).display,
            DisplayMode::Windowed { scale: 4 }
        );
        assert_eq!(
            launch(&[("TM_WINDOW_SCALE", "4")]).display,
            DisplayMode::Auto,
            "開発機の既定はフルスクリーン(ウィンドウは TM_DISPLAY=window)"
        );
        assert_eq!(
            launch(&[("TM_WINDOW_SCALE", "4")]).window_scale,
            4,
            "F11 で戻るウィンドウ"
        );
    }

    #[test]
    fn dev_hotkeys_only_with_devtools() {
        let c = launch(&[("TM_DEV_HOTKEYS", "off")]);
        if cfg!(feature = "devtools") {
            assert!(!c.dev_hotkeys);
        } else {
            assert_eq!(c.dev_hotkeys, SdlConfig::dev_default().dev_hotkeys);
        }
    }
}
