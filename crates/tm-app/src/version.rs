//! 版の情報 [16 §1.14、APP-D-13]。

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
            game_version: env!("TM_GAME_VERSION").to_owned(),
            rustc_version: env!("TM_RUSTC_VERSION").to_owned(),
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
        assert!(
            v.rustc_version.starts_with("rustc ") || v.rustc_version == "unknown",
            "{}",
            v.rustc_version
        );
    }
}
