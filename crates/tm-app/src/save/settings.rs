//! `settings.ron`: 音量と「せつめいを 出す」。セーブとは別で、リセットでも消さない [16 §1.20、17 GW-12]。

use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use tm_ui::Settings;

#[derive(Serialize, Deserialize)]
#[serde(rename = "Settings", default)]
struct SettingsFile {
    music: u8,
    sound: u8,
    show_explanations: bool,
}

impl Default for SettingsFile {
    fn default() -> Self {
        let s = Settings::default();
        SettingsFile {
            music: s.music,
            sound: s.sound,
            show_explanations: s.show_explanations,
        }
    }
}

/// `Settings(music: 6, sound: 8, show_explanations: true)` の 1 行。
pub fn encode_settings(s: Settings) -> String {
    let f = SettingsFile {
        music: s.music,
        sound: s.sound,
        show_explanations: s.show_explanations,
    };
    let mut out = ron::ser::to_string_pretty(
        &f,
        PrettyConfig::new().struct_names(true).compact_structs(true),
    )
    .expect("設定は RON に書ける");
    out.push('\n');
    out
}

/// 読む。ない欄は初期値、音量は `Ui::set_settings` が 0..=10 に収める。
pub fn decode_settings(bytes: &[u8]) -> Result<Settings, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let f: SettingsFile = ron::from_str(text).map_err(|e| e.to_string())?;
    Ok(Settings {
        music: f.music,
        sound: f.sound,
        show_explanations: f.show_explanations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let s = Settings {
            music: 3,
            sound: 10,
            show_explanations: false,
        };
        let text = encode_settings(s);
        assert_eq!(
            text,
            "Settings(music: 3, sound: 10, show_explanations: false)\n"
        );
        assert_eq!(decode_settings(text.as_bytes()), Ok(s));
        assert_eq!(
            decode_settings(b"Settings(music: 2)"),
            Ok(Settings {
                music: 2,
                ..Settings::default()
            }),
            "ない欄は初期値"
        );
        assert!(decode_settings(b"Settings(music: 300)").is_err());
        assert!(decode_settings(b"(music: ").is_err());
        assert!(decode_settings(&[0xff, 0xfe]).is_err());
    }
}
