//! 数の表示のキャッシュ `NumText`(`format_ja_into` の結果)[13 §4.1]。

use tm_assets::font::FontId;
use tm_input::InputDevice;
use tm_numeric::{BigNum, FormatStyle, format_ja_into};

use super::{TextBlock, layout};

/// 円なしの書式(人数)。
pub(crate) const PEOPLE: FormatStyle = FormatStyle {
    append_yen: false,
    ..FormatStyle::DEFAULT
};

/// 1 つの数の表示。`BigNum::to_bits()` と書式が前回と同じなら作り直さない。
#[derive(Default)]
pub(crate) struct NumText {
    key: Option<(u64, bool)>,
    /// `format_ja` の文字列。
    pub text: String,
    /// 文字列を折り返さずに並べたもの。
    pub block: TextBlock,
    size: Option<FontId>,
    #[cfg(test)]
    rebuilds: u32,
}

impl NumText {
    /// 値を入れる。表示が変わったら true。
    pub(crate) fn set(&mut self, v: BigNum, style: &FormatStyle, size: FontId) -> bool {
        let key = (v.to_bits(), style.append_yen);
        if self.key == Some(key) && self.size == Some(size) {
            return false;
        }
        self.key = Some(key);
        self.size = Some(size);
        format_ja_into(v, style, &mut self.text);
        layout(
            &self.text,
            size,
            u16::MAX,
            &[],
            InputDevice::Keyboard,
            &mut self.block,
        );
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
        true
    }

    /// 人数を入れる(円なし)。
    pub(crate) fn set_people(&mut self, n: u64, size: FontId) -> bool {
        self.set(BigNum::from_u64(n), &PEOPLE, size)
    }

    /// 作り直した回数(テスト用)。
    #[cfg(test)]
    pub(crate) fn rebuilds(&self) -> u32 {
        self.rebuilds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_bits_do_not_rebuild() {
        let mut n = NumText::default();
        assert!(n.set(
            BigNum::from_u64(12_345),
            &FormatStyle::DEFAULT,
            FontId::Body12
        ));
        assert_eq!(n.text, "1万2345円");
        let cap = n.text.capacity();
        assert!(!n.set(
            BigNum::from_u64(12_345),
            &FormatStyle::DEFAULT,
            FontId::Body12
        ));
        assert_eq!(n.rebuilds(), 1);
        assert!(n.set(
            BigNum::from_u64(12_346),
            &FormatStyle::DEFAULT,
            FontId::Body12
        ));
        assert_eq!(n.rebuilds(), 2);
        assert_eq!(n.text.capacity(), cap, "文字列のバッファは使い回す");
        assert!(n.set(BigNum::from_u64(12_346), &PEOPLE, FontId::Body12));
        assert_eq!(n.text, "1万2346");
        assert!(n.set(
            BigNum::from_u64(123_456),
            &FormatStyle::DEFAULT,
            FontId::Body12
        ));
        assert_eq!(
            n.text, "12万円",
            "01 D-10: 最上位が 2 桁以上なら下の単位を添えない"
        );
    }

    #[test]
    fn people_have_no_yen() {
        let mut n = NumText::default();
        n.set_people(120_000, FontId::Body12);
        assert_eq!(n.text, "12万");
        n.set_people(12, FontId::Body12);
        assert_eq!(n.text, "12");
        assert_eq!(n.block.width, 12);
    }

    #[test]
    fn longest_number_is_138px_at_12px() {
        let mut v = BigNum::ONE;
        for _ in 0..68 {
            v = v.mul_small(10);
        }
        let v = v.mul_small(12_345).div_small(10_000);
        let mut n = NumText::default();
        n.set(v, &FormatStyle::DEFAULT, FontId::Body12);
        assert_eq!(n.text, "1無量大数2345不可思議円");
        assert_eq!(n.block.width, 138, "13 §3.5 の見積もり");
        assert_eq!(n.block.lines, 1);
    }
}
