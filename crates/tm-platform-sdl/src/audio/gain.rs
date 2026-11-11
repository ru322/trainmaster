//! 音量の表とゲインのランプ [14 §4.4、PL-D-03、PL-D-05]。

use super::mixer::SAMPLE_RATE;

/// Q16 の 1.0。
pub(crate) const UNITY: u32 = 1 << 16;
/// 止める・切り替えるときのフェード(5ms)。
pub(crate) const STOP_FADE_SAMPLES: u32 = 110;
/// 音量とダッキングのランプ(10ms)。
pub(crate) const LEVEL_RAMP_SAMPLES: u32 = 220;
/// 音量の段階の上限。
pub(crate) const MAX_LEVEL: u8 = 10;
/// 音量の段階の倍率 `(L/10)^2` の Q16。`⌊(2·L²·65536 + 100) / 200⌋`。
pub(crate) const VOLUME_Q16: [u32; 11] = [
    0, 655, 2621, 5898, 10486, 16384, 23593, 32113, 41943, 53084, 65536,
];

/// 段階 `level`(10 で頭打ち)の Q16 の倍率。ダッキング中は半分。
pub(crate) fn level_gain(level: u8, duck: bool) -> u32 {
    VOLUME_Q16[usize::from(level.min(MAX_LEVEL))] >> u32::from(duck)
}

/// ミリ秒をサンプル数にする(切り捨て)。
pub(crate) fn ms_to_samples(ms: u16) -> u32 {
    u32::from(ms) * SAMPLE_RATE / 1000
}

/// `total` サンプルで `start` から `target` へ直線的に変わる Q16 のゲイン。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ramp {
    cur: u32,
    start: u32,
    target: u32,
    elapsed: u32,
    total: u32,
}

impl Ramp {
    /// 変化しないゲイン。
    pub(crate) const fn fixed(gain: u32) -> Ramp {
        Ramp {
            cur: gain,
            start: gain,
            target: gain,
            elapsed: 0,
            total: 0,
        }
    }

    /// いまの値から `target` へ `samples` で変える。0 ならすぐに変える。
    pub(crate) fn retarget(&mut self, target: u32, samples: u32) {
        *self = Ramp {
            cur: if samples == 0 { target } else { self.cur },
            start: self.cur,
            target,
            elapsed: 0,
            total: samples,
        };
    }

    /// 1 サンプル進める。
    pub(crate) fn step(&mut self) {
        if self.elapsed >= self.total {
            return;
        }
        self.elapsed += 1;
        let (s, t) = (i64::from(self.start), i64::from(self.target));
        let v = s + (t - s) * i64::from(self.elapsed) / i64::from(self.total);
        self.cur = u32::try_from(v).unwrap_or(self.target);
    }

    /// いまの値。
    pub(crate) fn cur(&self) -> u32 {
        self.cur
    }

    /// 目標の値。
    #[cfg(test)]
    pub(crate) fn target(&self) -> u32 {
        self.target
    }

    /// 目標に着くまでのサンプル数。
    pub(crate) fn remaining(&self) -> u32 {
        self.total - self.elapsed.min(self.total)
    }

    /// 0 に着いた。
    pub(crate) fn is_silent(&self) -> bool {
        self.cur == 0 && self.remaining() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_table_matches_formula() {
        for (l, v) in VOLUME_Q16.iter().enumerate() {
            let l = l as u64;
            assert_eq!(u64::from(*v), (2 * l * l * 65536 + 100) / 200, "{l}");
        }
        assert_eq!(level_gain(11, false), UNITY);
        assert_eq!(level_gain(255, true), UNITY / 2);
        assert_eq!(level_gain(6, true), 23593 >> 1);
    }

    #[test]
    fn ms_to_samples_floors() {
        assert_eq!(ms_to_samples(5), STOP_FADE_SAMPLES);
        assert_eq!(ms_to_samples(10), LEVEL_RAMP_SAMPLES);
        assert_eq!(ms_to_samples(1000), 22050);
        assert_eq!(ms_to_samples(u16::MAX), 1_445_046);
    }

    #[test]
    fn ramp_is_linear_and_lands_on_target() {
        let mut r = Ramp::fixed(UNITY);
        r.retarget(0, 22050);
        for k in 1..=22050u64 {
            r.step();
            assert_eq!(u64::from(r.cur()), 65536 - 65536 * k / 22050, "{k}");
        }
        assert!(r.is_silent());
        r.step();
        assert_eq!(r.cur(), 0);
        r.retarget(1000, 0);
        assert_eq!((r.cur(), r.remaining()), (1000, 0));
    }

    #[test]
    fn retarget_mid_ramp_starts_from_current() {
        let mut r = Ramp::fixed(0);
        r.retarget(1000, 10);
        for _ in 0..5 {
            r.step();
        }
        assert_eq!(r.cur(), 500);
        r.retarget(0, 5);
        r.step();
        assert_eq!(r.cur(), 400);
        assert_eq!(r.remaining(), 4);
        assert!(!r.is_silent());
    }
}
