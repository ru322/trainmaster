//! 版の情報 [16 §1.14、APP-D-13]。

use std::hint::black_box;

// 実行ファイルの中で make-dist.sh がこの印を、check-rust-version.sh が RUSTC_VERSION を探す(03 §6.2、C-3)。
// 短い文字列は即値の書き込みに畳まれて実行ファイルに残らないことがあるので、black_box を通して読む
const GAME_VERSION_MARK: &str = concat!("TM_GAME_VERSION=", env!("TM_GAME_VERSION"));
const RUSTC_VERSION: &str = env!("TM_RUSTC_VERSION");

/// ゲームと rustc の版。リプレイのヘッダと起動時のログに使う。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionInfo {
    /// `TM_BUILD_VERSION`(ビルド時)があればそれ、なければ `CARGO_PKG_VERSION`。
    pub game_version: String,
    /// `rustc --version` の 1 行全体。
    pub rustc_version: String,
}

impl VersionInfo {
    /// このビルドの版。
    pub fn build() -> Self {
        VersionInfo {
            game_version: black_box(GAME_VERSION_MARK)["TM_GAME_VERSION=".len()..].to_owned(),
            rustc_version: black_box(RUSTC_VERSION).to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustc_version_is_the_whole_line() {
        let v = VersionInfo::build();
        assert!(!v.game_version.is_empty());
        assert_eq!(v.game_version, env!("TM_GAME_VERSION"));
        assert!(
            v.rustc_version.starts_with("rustc ") || v.rustc_version == "unknown",
            "{}",
            v.rustc_version
        );
    }
}
