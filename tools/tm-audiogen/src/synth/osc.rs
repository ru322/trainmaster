//! 発振器(14 §5.4)。振幅の基準は ±8192。

use crate::schema::{Duty, NoiseMode};
use crate::tables::{CENT_RATIO_Q30, DUTY_THRESHOLD, INC_C8, PITCH_MAX, SINE256, TRI32};

/// C0 からのセントを位相の増分にする。範囲外は C0〜B8 の端で止める。
pub(crate) fn pitch_inc(pitch: i32) -> u32 {
    let p = pitch.clamp(0, PITCH_MAX) as u32;
    let oct = p / 1200;
    let r = (p % 1200) as usize;
    let inc = (u64::from(INC_C8) * u64::from(CENT_RATIO_Q30[r])) >> 30;
    (inc >> (8 - oct)) as u32
}

pub(crate) fn pulse(phase: u32, duty: Duty) -> i32 {
    if phase < DUTY_THRESHOLD[duty_index(duty)] {
        8192
    } else {
        -8192
    }
}

pub(crate) fn duty_index(duty: Duty) -> usize {
    match duty {
        Duty::D12 => 0,
        Duty::D25 => 1,
        Duty::D50 => 2,
        Duty::D75 => 3,
    }
}

pub(crate) fn triangle(phase: u32) -> i32 {
    i32::from(TRI32[(phase >> 27) as usize])
}

pub(crate) fn saw(phase: u32) -> i32 {
    (phase >> 18) as i32 - 8192
}

pub(crate) fn sine(phase: u32) -> i32 {
    i32::from(SINE256[(phase >> 24) as usize])
}

/// 15 ビットの線形帰還シフトレジスタ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lfsr(pub u16);

impl Lfsr {
    pub(crate) const INIT: Lfsr = Lfsr(1);

    pub(crate) fn shift(&mut self, mode: NoiseMode) {
        let r = self.0;
        let tap = match mode {
            NoiseMode::Long => 1,
            NoiseMode::Short => 6,
        };
        let fb = (r ^ (r >> tap)) & 1;
        self.0 = (r >> 1) | (fb << 14);
    }

    pub(crate) fn output(self) -> i32 {
        if self.0 & 1 == 1 { -8192 } else { 8192 }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_arithmetic)]
    use super::*;
    use crate::tables::SAMPLE_RATE;

    const A4: i32 = 4 * 1200 + 900;

    #[test]
    fn a02_square_period_a4() {
        let inc = pitch_inc(A4);
        let mut phase = 0u32;
        let mut prev = pulse(phase, Duty::D50);
        let mut changes = 0u32;
        for _ in 1..SAMPLE_RATE {
            phase = phase.wrapping_add(inc);
            let v = pulse(phase, Duty::D50);
            if v != prev {
                changes += 1;
            }
            prev = v;
        }
        let hz = f64::from(changes) / 2.0;
        assert!((hz - 440.0).abs() <= 0.5, "{hz}");
        let exact = f64::from(inc) * f64::from(SAMPLE_RATE) / 4_294_967_296.0;
        assert!((exact - 440.0).abs() < 0.01, "{exact}");
    }

    #[test]
    fn a03_duty_ratio() {
        let inc = pitch_inc(A4);
        let period = 4_294_967_296.0 / f64::from(inc);
        let n = period.round() as u32;
        for (duty, ratio) in [
            (Duty::D12, 0.125),
            (Duty::D25, 0.25),
            (Duty::D50, 0.5),
            (Duty::D75, 0.75),
        ] {
            let mut phase = 0u32;
            let mut high = 0u32;
            for _ in 0..n {
                if pulse(phase, duty) > 0 {
                    high += 1;
                }
                phase = phase.wrapping_add(inc);
            }
            let want = period * ratio;
            assert!(
                (f64::from(high) - want).abs() <= 1.0,
                "{duty:?} {high} {want}"
            );
        }
    }

    #[test]
    fn pitch_is_clamped_and_octaves_halve() {
        assert_eq!(pitch_inc(-500), pitch_inc(0));
        assert_eq!(pitch_inc(20_000), pitch_inc(PITCH_MAX));
        assert_eq!(pitch_inc(8 * 1200), INC_C8);
        assert_eq!(pitch_inc(7 * 1200), INC_C8 >> 1);
    }

    #[test]
    fn waves_cover_full_scale() {
        assert_eq!(saw(0), -8192);
        assert_eq!(saw(u32::MAX), 8191);
        assert_eq!(sine(0), 0);
        assert_eq!(sine(0x4000_0000), 8192);
        assert_eq!(triangle(0), -8192);
        assert_eq!(triangle(0x7FFF_FFFF), 8192);
    }

    fn first_bits(mode: NoiseMode) -> u32 {
        let mut r = Lfsr::INIT;
        let mut bits = 0u32;
        for i in 0..32 {
            r.shift(mode);
            bits |= u32::from(r.0 & 1) << i;
        }
        bits
    }

    #[test]
    fn a05_noise_golden_and_period() {
        assert_eq!(first_bits(NoiseMode::Long), 0x3000_4000);
        assert_eq!(first_bits(NoiseMode::Short), 0x2080_4000);
        for (mode, period) in [(NoiseMode::Short, 93), (NoiseMode::Long, 32767)] {
            let mut r = Lfsr::INIT;
            let mut n = 0;
            loop {
                r.shift(mode);
                n += 1;
                if r == Lfsr::INIT {
                    break;
                }
            }
            assert_eq!(n, period, "{mode:?}");
        }
    }
}
