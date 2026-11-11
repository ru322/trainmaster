//! 決定論のゴールデン(02 §10.4、10 VT-3・VT-4、02 V5)。x86_64 で生成し、aarch64 でも同じ値になること。

mod common;

use common::{big, fnv1a, raw};
use core::hint::black_box;
use tm_numeric::BigNum;

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 11
}

#[test]
fn from_u64_bits() {
    let cases: [(u64, u64); 6] = [
        (1, 0x3FF0_0000_0000_0000),
        ((1 << 53) + 1, 0x4340_0000_0000_0000),
        ((1 << 53) + 3, 0x4340_0000_0000_0002),
        (0x8000_0000_0000_0C00, 0x43E0_0000_0000_0002),
        (12_345_678_901_234_567_891, 0x43E5_6A95_319D_63E1),
        (u64::MAX, 0x43F0_0000_0000_0000),
    ];
    for (v, bits) in cases {
        assert_eq!(
            BigNum::from_u64(v).to_bits(),
            bits,
            "{v}: {:#018X}",
            BigNum::from_u64(v).to_bits()
        );
    }
    let mut s = 1;
    let h = fnv1a((0..4096).map(|_| BigNum::from_u64(lcg(&mut s) << (lcg(&mut s) % 12))));
    assert_eq!(h, 0xCC70_2761_9BA2_7678, "{h:#018X}");
}

#[test]
fn pow10_bits() {
    let cases: [(u32, u64); 7] = [
        (0, 0x3FF0_0000_0000_0000),
        (22, 0x4480_F0CF_064D_D592),
        (23, 0x44B5_2D02_C7E1_4AF6),
        (24, 0x44EA_7843_79D9_9DB4),
        (68, 0x4E0D_AC74_463A_989F),
        (80, 0x508A_FCEF_51F0_FB5F),
        (308, 0x7FE1_CCF3_85EB_C8A0),
    ];
    for (n, bits) in cases {
        assert_eq!(
            BigNum::pow10(n).to_bits(),
            bits,
            "10^{n}: {:#018X}",
            BigNum::pow10(n).to_bits()
        );
    }
    let h = fnv1a((0..=309).map(BigNum::pow10));
    assert_eq!(h, 0xD1F5_CAD0_64B2_DECB, "{h:#018X}");
}

#[test]
fn pow_u32_bits() {
    let r115 = BigNum::from_u64(115).div_small(100);
    let cases: [(BigNum, u32, u64); 6] = [
        (BigNum::from_u64(3), 40, 0x43E5_1716_8A45_23FD),
        (BigNum::from_u64(7), 52, 0x490F_9CFB_C9AF_13FD),
        (BigNum::from_u64(10), 52, 0x4ABA_BA47_1495_7D30),
        (r115, 52, 0x4096_648C_40F1_DEDC),
        (BigNum::from_u64(13), 100, 0x5710_7EC3_8400_F04E),
        (BigNum::ONE.div_small(3), 200, 0x2C20_1559_3836_645F),
    ];
    for (base, exp, bits) in cases {
        let got = BigNum::pow_u32(base, exp).to_bits();
        assert_eq!(got, bits, "{base:?}^{exp}: {got:#018X}");
    }
    let bases = (2..=20)
        .map(BigNum::from_u64)
        .chain((101..=130).map(|n| BigNum::from_u64(n).div_small(100)));
    let h = fnv1a(bases.flat_map(|b| (0..=64).map(move |e| BigNum::pow_u32(b, e))));
    assert_eq!(h, 0x3F89_98B1_AA12_1E3E, "{h:#018X}");
}

#[test]
fn arithmetic_chain_bits() {
    let mut s = 42;
    let mut acc = BigNum::ONE;
    let mut out = Vec::with_capacity(20_000);
    for i in 0..20_000u32 {
        let x = BigNum::from_u64(lcg(&mut s)).div_small(lcg(&mut s) % 1000 + 1);
        acc = match i % 6 {
            0 => acc.saturating_add(x),
            1 => acc.mul_small(lcg(&mut s) % 100 + 1),
            2 => acc.checked_sub(x).unwrap_or(x),
            3 => acc.div_small(lcg(&mut s) % 97 + 1),
            4 => acc.saturating_mul(x.div_small(1 << 40)),
            _ => acc
                .checked_div(x)
                .unwrap_or(acc)
                .saturating_add(BigNum::pow10(i % 80)),
        };
        out.push(acc);
    }
    let h = fnv1a(out);
    assert_eq!(h, 0x5CFC_50D5_2C34_DFE3, "{h:#018X}");
}

/// 02 V5: 積と和(差)が FMA に縮約されないこと。縮約されると 2^-60 が残る。
#[test]
#[allow(clippy::disallowed_methods)]
fn no_fma_contraction() {
    let a = black_box(1.0 + common::pow2(-30));
    let c = black_box(1.0 + common::pow2(-29));
    assert_eq!(a * a - c, 0.0);
    assert_eq!(a.mul_add(a, -c), common::pow2(-60));

    let ab = big(a).saturating_mul(big(a));
    assert_eq!(ab.checked_sub(big(c)), Some(BigNum::ZERO));
    assert_eq!(raw(ab), c);
}
