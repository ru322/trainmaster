//! 音量と「説明を出す」の設定。リセットしても保つ [02 D30]。

/// 利用者の設定。初期値は 01 §11.3、§12.4。tm-app が `settings.ron` に保存する。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// 音楽の音量(0..=10)。
    pub music: u8,
    /// 効果音の音量(0..=10)。
    pub sound: u8,
    /// 説明を出す。
    pub show_explanations: bool,
}

/// 音量の上限。
pub(crate) const VOLUME_MAX: u8 = 10;

impl Settings {
    /// 音量を 0..=10 に収めたもの。
    pub(crate) fn clamped(self) -> Settings {
        Settings {
            music: self.music.min(VOLUME_MAX),
            sound: self.sound.min(VOLUME_MAX),
            ..self
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            music: 6,
            sound: 8,
            show_explanations: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_01() {
        let s = Settings::default();
        assert_eq!((s.music, s.sound, s.show_explanations), (6, 8, true));
    }

    #[test]
    fn clamped_keeps_volumes_in_range() {
        let s = Settings {
            music: 11,
            sound: 255,
            show_explanations: false,
        }
        .clamped();
        assert_eq!((s.music, s.sound, s.show_explanations), (10, 10, false));
        assert_eq!(Settings::default().clamped(), Settings::default());
    }
}
