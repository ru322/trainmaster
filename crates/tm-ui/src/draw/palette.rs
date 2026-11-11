//! 画面の配色。正は 13 §3.11 の表(UI-D-21)。

#![allow(
    dead_code,
    reason = "13 §3.11 の表をそのまま持つ。線路の色などは IMP-402 などが使う"
)]

use tm_core::LineId;
use tm_platform_api::Color;

const fn rgb(hex: u32) -> Color {
    Color {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
        a: 255,
    }
}

/// 色の不透明度を変える。
pub(crate) const fn with_alpha(c: Color, a: u8) -> Color {
    Color { a, ..c }
}

/// 白(スプライトをそのままの色で描く tint)。
pub(crate) const WHITE: Color = rgb(0xFFFFFF);
/// 黒。
pub(crate) const BLACK: Color = rgb(0x000000);
/// 地面の市松の 1 色目。
pub(crate) const GROUND_A: Color = rgb(0xE8E2CC);
/// 地面の市松の 2 色目。
pub(crate) const GROUND_B: Color = rgb(0xDED7BE);
/// 線路。
pub(crate) const RAIL: Color = rgb(0x5A5A5A);
/// 路線 1〜6(Okabe–Ito)。
pub(crate) const LINES: [Color; 6] = [
    rgb(0xE69F00),
    rgb(0x56B4E9),
    rgb(0x009E73),
    rgb(0xF0E442),
    rgb(0x0072B2),
    rgb(0xD55E00),
];
/// パネル・HUD の地。
pub(crate) const PANEL: Color = rgb(0x1E2A4A);
/// 情報の段の地(帯の 2 段目と区別する)。
pub(crate) const INFO_BG: Color = rgb(0x26345A);
/// マップの表示域のうち、マップの外(今のマップが表示域より小さいとき)。
pub(crate) const OFF_MAP: Color = BLACK;
/// ふつうの文字。
pub(crate) const TEXT: Color = rgb(0xFFFFFF);
/// 使えない文字。
pub(crate) const TEXT_DISABLED: Color = rgb(0xA6AEC0);
/// お金。
pub(crate) const MONEY: Color = rgb(0xFFE066);
/// 収入。
pub(crate) const INCOME: Color = rgb(0xA5F2B0);
/// エラーの案内。
pub(crate) const ERROR: Color = rgb(0xFFB4A8);
/// highlight の枠。
pub(crate) const HIGHLIGHT: Color = rgb(0xFFD400);
/// 会社ゲージの空き(13 は決めていない)。地に対して 3:1 以上(図形の基準。13 §8)。
pub(crate) const GAUGE_EMPTY: Color = rgb(0x6B7AA6);
/// 会社ゲージのたまった分。
pub(crate) const GAUGE_FILL: Color = INCOME;
/// 会社レベルが最大のときのゲージ(金色。01 §5.1)。
pub(crate) const GAUGE_GOLD: Color = HIGHLIGHT;
/// マップ上の数の札の地。
pub(crate) const TAG_BG: Color = with_alpha(PANEL, 208);

/// 路線の色。7 本目以降は色を繰り返す(番号の札で区別する)。
pub(crate) fn line_color(id: LineId) -> Color {
    LINES[id.index() % LINES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_values_follow_13() {
        assert_eq!(
            GROUND_A,
            Color {
                r: 0xE8,
                g: 0xE2,
                b: 0xCC,
                a: 255
            }
        );
        assert_eq!(PANEL, tm_assets::palette::PAL_PANEL);
        assert_eq!(RAIL, tm_assets::palette::PAL_RAIL);
        assert_eq!(HIGHLIGHT, tm_assets::palette::PAL_HIGHLIGHT);
        assert_eq!(line_color(LineId(0)), rgb(0xE69F00));
        assert_eq!(line_color(LineId(6)), line_color(LineId(0)));
    }

    fn luminance(c: Color) -> f64 {
        let lin = |v: u8| {
            let s = f64::from(v) / 255.0;
            if s <= 0.039_28 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn gauge_segments_meet_3_to_1() {
        assert!(
            contrast(GAUGE_EMPTY, PANEL) >= 3.0,
            "{}",
            contrast(GAUGE_EMPTY, PANEL)
        );
        assert!(contrast(GAUGE_EMPTY, GAUGE_FILL) >= 3.0);
        assert!(contrast(GAUGE_FILL, PANEL) >= 3.0);
        assert!(contrast(GAUGE_GOLD, PANEL) >= 3.0);
    }

    #[test]
    fn text_on_the_info_row_meets_4_5_to_1() {
        for c in [TEXT, TEXT_DISABLED, MONEY, INCOME] {
            assert!(contrast(c, INFO_BG) >= 4.5, "{c:?}");
        }
        assert_ne!(INFO_BG, PANEL);
    }
}
