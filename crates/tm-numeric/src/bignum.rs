use core::cmp::Ordering;

/// お金などの巨大数。内部は `f64`(非公開)で、NaN・無限大・負の値・-0.0 を持たない [02 D5]。
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct BigNum(pub(crate) f64);

impl BigNum {
    /// 0。
    pub const ZERO: BigNum = BigNum(0.0);
    /// 1。
    pub const ONE: BigNum = BigNum(1.0);
    /// 表現できる最大の有限値。演算の飽和先。
    pub const MAX: BigNum = BigNum(f64::MAX);

    fn saturate(x: f64) -> Self {
        debug_assert!(!x.is_nan() && x >= 0.0);
        if x.is_finite() { BigNum(x) } else { Self::MAX }
    }

    /// `v as f64`(最近接偶数への丸め。10 VT-3)。
    pub fn from_u64(v: u64) -> Self {
        BigNum(v as f64)
    }

    /// 和。上限を超えたら `MAX`。
    pub fn saturating_add(self, rhs: Self) -> Self {
        Self::saturate(self.0 + rhs.0)
    }

    /// 差。`rhs > self` なら `None`。結果は +0.0 以上。
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        if rhs.0 > self.0 {
            None
        } else {
            Some(BigNum(self.0 - rhs.0))
        }
    }

    /// `self × (k as f64)`。上限を超えたら `MAX`。
    pub fn mul_small(self, k: u64) -> Self {
        Self::saturate(self.0 * k as f64)
    }

    /// 積。上限を超えたら `MAX`。
    pub fn saturating_mul(self, rhs: Self) -> Self {
        Self::saturate(self.0 * rhs.0)
    }

    /// `self ÷ (k as f64)`。`k = 0` なら panic する。
    pub fn div_small(self, k: u64) -> Self {
        assert!(k != 0, "BigNum::div_small by zero");
        BigNum(self.0 / k as f64)
    }

    /// 商。`rhs` が 0 なら `None`。上限を超えたら `MAX`。
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        if rhs.is_zero() {
            None
        } else {
            Some(Self::saturate(self.0 / rhs.0))
        }
    }

    /// `base^exp` を乗算だけで求める。手順は 02 §4.1 で固定(LSB から二乗の繰り返し)。上限を超えたら `MAX`。
    pub fn pow_u32(base: Self, exp: u32) -> Self {
        let mut result = 1.0_f64;
        let mut b = base.0;
        let mut e = exp;
        while e > 0 {
            if e & 1 == 1 {
                result *= b;
            }
            e >>= 1;
            if e > 0 {
                b *= b;
            }
        }
        Self::saturate(result)
    }

    /// 全順序の比較(ソートのキーに使う)。
    pub fn total_cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }

    /// 0 かどうか。
    pub fn is_zero(self) -> bool {
        self.0 == 0.0
    }

    /// ハッシュ用のビット列。-0.0 は 0.0 に正規化する [02 §3.6]。
    pub fn to_bits(self) -> u64 {
        if self.is_zero() { 0 } else { self.0.to_bits() }
    }

    /// ビット列から戻す。NaN・無限大・負なら `None`。-0.0 は `ZERO` にする。
    pub fn from_bits_checked(bits: u64) -> Option<Self> {
        let x = f64::from_bits(bits);
        if x == 0.0 {
            Some(Self::ZERO)
        } else if x.is_finite() && x > 0.0 {
            Some(BigNum(x))
        } else {
            None
        }
    }
}
