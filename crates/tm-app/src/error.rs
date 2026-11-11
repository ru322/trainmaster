//! アプリのエラー [16 §1.15]。

use std::fmt;

use tm_core::BalanceError;
use tm_platform_api::{AssetError, PlatformError};

/// 起動や実行を止めるエラー。配布用の設定の誤りはエラーにしない(警告して既定値)。
#[derive(Debug)]
pub enum AppError {
    /// `tm_assets::bundle()` の検査の失敗。
    Assets(AssetError),
    /// `load_assets` か `present` の失敗。
    Platform(PlatformError),
    /// 開発用の `TM_BALANCE_FILE` の表が検証を通らない。
    Balance(BalanceError),
    /// 開発用のファイルが読めない。
    Config(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Assets(e) => write!(f, "アセットの検査に失敗: {e:?}"),
            AppError::Platform(e) => write!(f, "プラットフォームのエラー: {e:?}"),
            AppError::Balance(e) => write!(f, "{e}"),
            AppError::Config(m) => write!(f, "設定: {m}"),
        }
    }
}

impl std::error::Error for AppError {}
