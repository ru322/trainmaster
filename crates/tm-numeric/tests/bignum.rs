mod common;

use common::{any_bignum, big, config, five_pow, is_valid, limbs_u64, raw, reference_round};
use core::cmp::Ordering;
use proptest::prelude::*;
use tm_numeric::{BigNum, JA_UNIT_THRESHOLDS};

#[test]
fn constants() {
    assert_eq!(raw(BigNum::ZERO), 0.0);
    assert_eq!(raw(BigNum::ONE), 1.0);
    assert_eq!(raw(BigNum::MAX), f64::MAX);
    assert!(BigNum::ZERO.is_zero());
    assert!(!BigNum::ONE.is_zero());
    assert_eq!(BigNum::ZERO.to_bits(), 0);
}

#[test]
fn saturates_at_max() {
    let two = BigNum::from_u64(2);
    assert_eq!(BigNum::MAX.saturating_add(BigNum::MAX), BigNum::MAX);
    assert_eq!(BigNum::MAX.saturating_add(BigNum::ONE), BigNum::MAX);
    assert_eq!(BigNum::MAX.mul_small(2), BigNum::MAX);
    assert_eq!(BigNum::MAX.mul_small(u64::MAX), BigNum::MAX);
    assert_eq!(BigNum::MAX.saturating_mul(two), BigNum::MAX);
    assert_eq!(BigNum::MAX.saturating_mul(BigNum::MAX), BigNum::MAX);
    assert_eq!(
        BigNum::MAX.checked_div(BigNum::ONE.div_small(2)),
        Some(BigNum::MAX)
    );
    let tiny = BigNum::from_bits_checked(1).unwrap();
    assert_eq!(BigNum::ONE.checked_div(tiny), Some(BigNum::MAX));
    assert_eq!(BigNum::pow_u32(BigNum::from_u64(10), 309), BigNum::MAX);
    assert_eq!(BigNum::pow_u32(two, u32::MAX), BigNum::MAX);
    assert_eq!(BigNum::pow_u32(BigNum::MAX, 2), BigNum::MAX);
    assert!(is_valid(BigNum::MAX.mul_small(0)));
    assert_eq!(BigNum::MAX.mul_small(0), BigNum::ZERO);
}

#[test]
fn pow_u32_edges() {
    let half = BigNum::ONE.div_small(2);
    assert_eq!(BigNum::pow_u32(BigNum::ZERO, 0), BigNum::ONE);
    assert_eq!(BigNum::pow_u32(BigNum::ZERO, 7), BigNum::ZERO);
    assert_eq!(BigNum::pow_u32(BigNum::MAX, 0), BigNum::ONE);
    assert_eq!(BigNum::pow_u32(BigNum::ONE, u32::MAX), BigNum::ONE);
    assert_eq!(BigNum::pow_u32(half, u32::MAX), BigNum::ZERO);
    assert_eq!(
        BigNum::pow_u32(BigNum::from_u64(3), 33),
        BigNum::from_u64(5_559_060_566_555_523)
    );
    assert_eq!(
        BigNum::pow_u32(BigNum::from_u64(2), 1023),
        big(common::pow2(1023))
    );
}

#[test]
fn division() {
    assert_eq!(BigNum::ONE.checked_div(BigNum::ZERO), None);
    assert_eq!(BigNum::ZERO.checked_div(BigNum::ZERO), None);
    assert_eq!(BigNum::ZERO.checked_div(BigNum::ONE), Some(BigNum::ZERO));
    assert_eq!(
        BigNum::from_u64(10).div_small(4),
        BigNum::from_u64(5).div_small(2)
    );
    assert_eq!(BigNum::from_u64(7).div_small(7), BigNum::ONE);
}

#[test]
#[should_panic]
fn div_small_by_zero_panics() {
    let _ = BigNum::ONE.div_small(0);
}

#[test]
fn checked_sub_edges() {
    let five = BigNum::from_u64(5);
    assert_eq!(five.checked_sub(five), Some(BigNum::ZERO));
    assert_eq!(five.checked_sub(five).unwrap().to_bits(), 0);
    assert_eq!(BigNum::ZERO.checked_sub(BigNum::ONE), None);
    assert_eq!(
        five.checked_sub(BigNum::from_u64(3)),
        Some(BigNum::from_u64(2))
    );
}

#[test]
fn from_bits_checked_rejects_invalid() {
    for bad in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.0,
        -f64::MIN_POSITIVE,
    ] {
        assert_eq!(BigNum::from_bits_checked(bad.to_bits()), None, "{bad}");
    }
    assert_eq!(BigNum::from_bits_checked(0x7FF8_0000_0000_0001), None);
    assert_eq!(
        BigNum::from_bits_checked((-0.0f64).to_bits()),
        Some(BigNum::ZERO)
    );
    assert_eq!(
        BigNum::from_bits_checked(f64::MAX.to_bits()),
        Some(BigNum::MAX)
    );
    assert_eq!(BigNum::from_bits_checked(1).map(BigNum::to_bits), Some(1));
}

#[test]
fn from_u64_rounds_ties_to_even() {
    const P53: u64 = 1 << 53;
    let cases = [
        (0, 0.0),
        (1, 1.0),
        (P53 - 1, 9_007_199_254_740_991.0),
        (P53, 9_007_199_254_740_992.0),
        (P53 + 1, 9_007_199_254_740_992.0),
        (P53 + 2, 9_007_199_254_740_994.0),
        (P53 + 3, 9_007_199_254_740_996.0),
        (P53 + 5, 9_007_199_254_740_996.0),
        ((1 << 54) + 2, 18_014_398_509_481_984.0),
        ((1 << 54) + 6, 18_014_398_509_481_992.0),
        (0x8000_0000_0000_0400, 9_223_372_036_854_775_808.0),
        (0x8000_0000_0000_0C00, 9_223_372_036_854_779_904.0),
        (u64::MAX, 18_446_744_073_709_551_616.0),
    ];
    for (v, expected) in cases {
        assert_eq!(raw(BigNum::from_u64(v)), expected, "{v}");
        assert_eq!(reference_round(&limbs_u64(v), 0), expected, "{v}");
    }
}

#[test]
fn pow10_is_correctly_rounded() {
    for n in 0..=308u32 {
        let expected = reference_round(&five_pow(n), n as i32);
        assert_eq!(raw(BigNum::pow10(n)), expected, "10^{n}");
    }
    assert_eq!(BigNum::pow10(309), BigNum::MAX);
    assert_eq!(BigNum::pow10(u32::MAX), BigNum::MAX);
}

#[test]
fn thresholds_match_pow10() {
    for (k, t) in JA_UNIT_THRESHOLDS.iter().enumerate() {
        assert_eq!(*t, BigNum::pow10(4 * k as u32), "k = {k}");
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn add_is_commutative(a in any_bignum(), b in any_bignum()) {
        prop_assert_eq!(a.saturating_add(b).to_bits(), b.saturating_add(a).to_bits());
    }

    #[test]
    fn mul_is_commutative(a in any_bignum(), b in any_bignum()) {
        prop_assert_eq!(a.saturating_mul(b).to_bits(), b.saturating_mul(a).to_bits());
    }

    #[test]
    fn add_is_monotone(a in any_bignum(), b in any_bignum(), c in any_bignum()) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(lo.saturating_add(c) <= hi.saturating_add(c));
    }

    #[test]
    fn mul_is_monotone(a in any_bignum(), b in any_bignum(), c in any_bignum()) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(lo.saturating_mul(c) <= hi.saturating_mul(c));
    }

    #[test]
    fn checked_sub_never_negative(a in any_bignum(), b in any_bignum()) {
        match a.checked_sub(b) {
            None => prop_assert!(b > a),
            Some(d) => {
                prop_assert!(b <= a);
                prop_assert!(is_valid(d));
                prop_assert!(d >= BigNum::ZERO);
                prop_assert!(d <= a);
            }
        }
    }

    #[test]
    fn results_stay_finite_and_non_negative(
        a in any_bignum(),
        b in any_bignum(),
        k in any::<u64>(),
        e in 0u32..2048,
    ) {
        prop_assert!(is_valid(a.saturating_add(b)));
        prop_assert!(is_valid(a.saturating_mul(b)));
        prop_assert!(is_valid(a.mul_small(k)));
        prop_assert!(is_valid(a.div_small(k.max(1))));
        if let Some(q) = a.checked_div(b) {
            prop_assert!(is_valid(q));
        }
        prop_assert!(is_valid(BigNum::pow_u32(a, e)));
    }

    #[test]
    fn total_cmp_agrees_with_partial_ord(a in any_bignum(), b in any_bignum()) {
        prop_assert_eq!(Some(a.total_cmp(&b)), a.partial_cmp(&b));
        prop_assert_eq!(a.total_cmp(&b) == Ordering::Equal, a == b);
    }

    #[test]
    fn bits_round_trip(a in any_bignum()) {
        prop_assert_eq!(BigNum::from_bits_checked(a.to_bits()), Some(a));
    }

    #[test]
    fn from_u64_matches_reference(v in any::<u64>()) {
        prop_assert_eq!(raw(BigNum::from_u64(v)), reference_round(&limbs_u64(v), 0));
    }

    #[test]
    fn pow_u32_close_to_repeated_mul(
        base in prop_oneof![
            (0u64..=1000).prop_map(BigNum::from_u64),
            (1u64..=100_000).prop_map(|n| BigNum::from_u64(n).div_small(100)),
        ],
        exp in 0u32..=64,
    ) {
        let mut repeated = BigNum::ONE;
        for _ in 0..exp {
            repeated = repeated.saturating_mul(base);
        }
        prop_assume!(repeated < BigNum::MAX);
        let fast = BigNum::pow_u32(base, exp);
        let (lo, hi) = if fast <= repeated { (fast, repeated) } else { (repeated, fast) };
        let diff = hi.checked_sub(lo).unwrap();
        let tolerance = hi.saturating_mul(BigNum::ONE.div_small(1_000_000_000_000));
        prop_assert!(diff <= tolerance, "{:?} vs {:?}", fast, repeated);
    }
}
