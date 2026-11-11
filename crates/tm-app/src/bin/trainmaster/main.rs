//! ゲーム本体の実行ファイル。環境変数を読み、SDL のプラットフォームを作って `tm_app::run` を呼ぶ。

#![forbid(unsafe_code)]

mod sdl_env;

use std::process::ExitCode;
use std::time::Instant;

use log::{error, info, warn};
use tm_app::{AppError, AppExit, EXIT_POWER_OFF};
use tm_platform_sdl::{SdlCore, SdlPlatform};

fn main() -> ExitCode {
    tm_app::logging::init(tm_app::logging::level_from_env());
    let v = tm_app::VersionInfo::build();
    tm_app::logging::always(format_args!(
        "trainmaster {} ({})",
        v.game_version, v.rustc_version
    ));
    let vars = std::env::vars_os().filter_map(|(k, v)| match (k.into_string(), v.into_string()) {
        (Ok(k), Ok(v)) => Some((k, v)),
        (k, _) => {
            let name = k.unwrap_or_else(|k| k.to_string_lossy().into_owned());
            warn!("環境変数 {name} は UTF-8 でないので無視した");
            None
        }
    });
    let (launch, warnings) = match sdl_env::LaunchConfig::from_env(vars) {
        Ok(c) => c,
        Err(e) => {
            error!("設定: {e}");
            return ExitCode::FAILURE;
        }
    };
    for w in &warnings {
        warn!("{w}");
    }
    info!(
        "設定: {:?} シード {:?} リプレイ {:?} 保存 {:?} 終わり方 {:?} 倍率 {} デバッグ表示 {} 自動終了 {:?}",
        launch.sdl,
        launch.app.seed,
        launch.app.replay,
        launch.app.save,
        launch.app.exit_action,
        launch.app.time_scale,
        launch.app.overlay,
        launch.app.quit_after_frames
    );
    let t = Instant::now();
    let core = match SdlCore::new(&launch.sdl) {
        Ok(c) => c,
        Err(e) => {
            error!("SDL の初期化: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    info!("SdlCore::new: {}ms", t.elapsed().as_millis());
    let tc = core.texture_creator();
    let t = Instant::now();
    let mut platform = match SdlPlatform::new(core, &tc, &launch.sdl) {
        Ok(p) => p,
        Err(e) => {
            error!("SdlPlatform::new: {e:?}");
            return ExitCode::FAILURE;
        }
    };
    info!("SdlPlatform::new: {}ms", t.elapsed().as_millis());
    exit_code(&tm_app::run(&mut platform, launch.app))
}

/// `run` の結果 → 終了コード(0 / 10 / 1)。drop の後に `main` から返す(14 §3.8)。
fn exit_code(r: &Result<AppExit, AppError>) -> ExitCode {
    let code = match r {
        Ok(AppExit::Normal) => 0,
        Ok(AppExit::PowerOff) => EXIT_POWER_OFF,
        Err(e) => {
            error!("{e}");
            1
        }
    };
    info!("終了コード {code}");
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_t_30_exit_codes() {
        assert_eq!(exit_code(&Ok(AppExit::Normal)), ExitCode::SUCCESS);
        assert_eq!(exit_code(&Ok(AppExit::PowerOff)), ExitCode::from(10));
        assert_ne!(exit_code(&Ok(AppExit::PowerOff)), ExitCode::FAILURE);
        assert_eq!(
            exit_code(&Err(AppError::Config("x".to_owned()))),
            ExitCode::FAILURE
        );
    }
}
