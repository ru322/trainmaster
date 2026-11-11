#![allow(dead_code)]

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use tm_numeric::BigNum;

/// 既定でシード固定(1)。`PROPTEST_RNG_SEED` があればそちらを使う。
pub fn config() -> ProptestConfig {
    let mut c = ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    };
    if matches!(c.rng_seed, RngSeed::Random) {
        c.rng_seed = RngSeed::Fixed(1);
    }
    c
}

pub fn raw(x: BigNum) -> f64 {
    f64::from_bits(x.to_bits())
}

pub fn big(x: f64) -> BigNum {
    BigNum::from_bits_checked(x.to_bits()).expect("non-negative finite")
}

pub fn is_valid(x: BigNum) -> bool {
    BigNum::from_bits_checked(x.to_bits()) == Some(x) && x.to_bits() >> 63 == 0
}

/// 非負の有限値の全域(非正規化数を含む)。
pub fn any_bignum() -> impl Strategy<Value = BigNum> {
    prop_oneof![
        (0u64..=0x7FEF_FFFF_FFFF_FFFF).prop_map(|b| BigNum::from_bits_checked(b).unwrap()),
        any::<u64>().prop_map(BigNum::from_u64),
        Just(BigNum::ZERO),
        Just(BigNum::MAX),
    ]
}

/// FNV-1a 64(`to_bits` の LE バイト列)。
pub fn fnv1a(values: impl IntoIterator<Item = BigNum>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in values {
        for byte in v.to_bits().to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// 2^e(正規化数の範囲)をビット列から作る。
pub fn pow2(e: i32) -> f64 {
    assert!((-1022..=1023).contains(&e));
    f64::from_bits(((e + 1023) as u64) << 52)
}

fn bit(limbs: &[u32], i: usize) -> bool {
    limbs.get(i / 32).is_some_and(|l| (l >> (i % 32)) & 1 == 1)
}

fn bit_len(limbs: &[u32]) -> usize {
    (0..limbs.len() * 32)
        .rev()
        .find(|&i| bit(limbs, i))
        .map_or(0, |i| i + 1)
}

/// 整数 B × 2^exp2(B は 32 ビットの桁の LE 列)を、最近接偶数で f64 に丸めた参照値。
pub fn reference_round(limbs: &[u32], exp2: i32) -> f64 {
    let len = bit_len(limbs);
    if len == 0 {
        return 0.0;
    }
    let shift = len.saturating_sub(53);
    let mut m: u64 = 0;
    for i in (shift..len).rev() {
        m = (m << 1) | u64::from(bit(limbs, i));
    }
    if shift > 0 && bit(limbs, shift - 1) {
        let sticky = (0..shift - 1).any(|i| bit(limbs, i));
        if sticky || m & 1 == 1 {
            m += 1;
        }
    }
    let mut e = exp2 + shift as i32;
    if m == 1 << 53 {
        m >>= 1;
        e += 1;
    }
    while m < 1 << 52 {
        m <<= 1;
        e -= 1;
    }
    let biased = e + 52 + 1023;
    assert!((1..=2046).contains(&biased), "out of normal range");
    f64::from_bits(((biased as u64) << 52) | (m & ((1 << 52) - 1)))
}

pub fn limbs_u64(v: u64) -> [u32; 2] {
    [v as u32, (v >> 32) as u32]
}

pub fn five_pow(n: u32) -> Vec<u32> {
    let mut limbs = vec![1u32];
    for _ in 0..n {
        let mut carry = 0u64;
        for l in &mut limbs {
            let t = u64::from(*l) * 5 + carry;
            *l = t as u32;
            carry = t >> 32;
        }
        if carry > 0 {
            limbs.push(carry as u32);
        }
    }
    limbs
}
