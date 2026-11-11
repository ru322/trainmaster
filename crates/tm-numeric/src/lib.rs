//! 巨大数の型と、日本語の数詞単位での表示。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod bignum;
mod format;
mod pow10;

pub use bignum::BigNum;
pub use format::{
    FormatStyle, JA_UNITS, OUTPUT_CHARS, UnitScript, format_ja, format_ja_into, ja_unit_index,
};
pub use pow10::JA_UNIT_THRESHOLDS;
