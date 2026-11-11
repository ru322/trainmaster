//! 固定タイムステップの累積器 [02 D38、16 §1.7、APP-D-02]。

use tm_core::TICKS_PER_SEC;
use tm_platform_api::MonoTime;

/// 1 tick の累積器の単位(µs × `TICKS_PER_SEC`)。
pub const TICK_UNITS: u64 = 1_000_000;
/// 1 フレームで数える時間の上限(µs)。
pub const MAX_FRAME_DT_US: u64 = 100_000;
/// 1 フレームの step の上限(倍率 1 のとき)。
pub const MAX_STEPS_PER_FRAME: u32 = 4;
/// フレーム間隔のスナップの幅(±1000µs × `TICKS_PER_SEC`)。
pub const SNAP_TOLERANCE_UNITS: u64 = 60_000;

/// フレームの時刻から、そのフレームで進める tick 数を決める。
#[derive(Clone, Debug)]
pub struct FrameClock {
    prev: MonoTime,
    acc: u64,
    scale: u32,
    dropped_ticks: u64,
}

impl FrameClock {
    /// `now` を起点に作る。`scale` は早送りの倍率(1 = 実時間)。
    pub fn new(now: MonoTime, scale: u32) -> Self {
        FrameClock {
            prev: now,
            acc: 0,
            scale: scale.max(1),
            dropped_ticks: 0,
        }
    }

    /// 前の呼び出しからの時間を数え、進める tick 数を返す。止まっていれば時間をためずに 0。
    pub fn advance(&mut self, now: MonoTime, running: bool) -> u32 {
        let dt_us = now.saturating_since(self.prev).min(MAX_FRAME_DT_US);
        self.prev = now;
        if !running {
            self.acc = 0;
            return 0;
        }
        let units = snap(dt_us * u64::from(TICKS_PER_SEC));
        self.acc += units * u64::from(self.scale);
        let cap = u64::from(MAX_STEPS_PER_FRAME * self.scale);
        let n = (self.acc / TICK_UNITS).min(cap);
        self.acc -= n * TICK_UNITS;
        if n == cap && self.acc >= TICK_UNITS {
            self.dropped_ticks += self.acc / TICK_UNITS;
            self.acc = 0;
        }
        n as u32
    }

    /// 追いつけずに捨てた tick の累計。
    pub fn dropped_ticks(&self) -> u64 {
        self.dropped_ticks
    }
}

/// 間隔が k tick(k = 1..=4)の ±1000µs 以内なら、ちょうど k tick にする。
fn snap(units: u64) -> u64 {
    let k = (units + TICK_UNITS / 2) / TICK_UNITS;
    if (1..=u64::from(MAX_STEPS_PER_FRAME)).contains(&k)
        && units.abs_diff(k * TICK_UNITS) <= SNAP_TOLERANCE_UNITS
    {
        k * TICK_UNITS
    } else {
        units
    }
}

/// スナップなしの D38 の累積器で進む tick 数(APP-V-02 の比較用)。`acc` は呼び出し側が持つ。
pub fn unsnapped_steps(acc: &mut u64, dt_us: u64) -> u32 {
    *acc += dt_us.min(MAX_FRAME_DT_US) * u64::from(TICKS_PER_SEC);
    let n = (*acc / TICK_UNITS).min(u64::from(MAX_STEPS_PER_FRAME));
    *acc -= n * TICK_UNITS;
    if n == u64::from(MAX_STEPS_PER_FRAME) && *acc >= TICK_UNITS {
        *acc = 0;
    }
    n as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(clock: &mut FrameClock, start: u64, dts: &[u64]) -> Vec<u32> {
        let mut t = start;
        dts.iter()
            .map(|dt| {
                t += dt;
                clock.advance(MonoTime(t), true)
            })
            .collect()
    }

    #[test]
    fn exact60_is_one_tick_per_frame_from_any_phase() {
        for phase in 0..3u64 {
            let mut c = FrameClock::new(MonoTime(phase * 1_000_000 / 60), 1);
            let mut steps = Vec::new();
            for f in phase + 1..phase + 601 {
                steps.push(c.advance(MonoTime(f * 1_000_000 / 60), true));
            }
            assert!(steps.iter().all(|&s| s == 1), "位相 {phase}: {steps:?}");
        }
    }

    #[test]
    fn without_snap_exact60_alternates() {
        let mut acc = 0;
        let mut seen = [0u32; 3];
        let mut prev = 0;
        for f in 1..601u64 {
            let now = f * 1_000_000 / 60;
            let n = unsnapped_steps(&mut acc, now - prev);
            prev = now;
            seen[n as usize] += 1;
        }
        assert!(seen[0] > 0 && seen[2] > 0, "{seen:?}");
    }

    #[test]
    fn snap_window_is_plus_minus_1000us() {
        let base = 16_667;
        let mut c = FrameClock::new(MonoTime(0), 1);
        assert_eq!(run(&mut c, 0, &[base + 999]), [1]);
        let mut c = FrameClock::new(MonoTime(0), 1);
        assert_eq!(run(&mut c, 0, &[base - 999]), [1]);
        let mut c = FrameClock::new(MonoTime(0), 1);
        assert_eq!(run(&mut c, 0, &[base - 1_500]), [0]);
        let mut c = FrameClock::new(MonoTime(0), 1);
        assert_eq!(run(&mut c, 0, &[33_333, 50_000, 66_666]), [2, 3, 4]);
    }

    fn total(dt: u64, frames: usize) -> u32 {
        let mut c = FrameClock::new(MonoTime(0), 1);
        run(&mut c, 0, &vec![dt; frames]).iter().sum()
    }

    #[test]
    fn snap_window_edges() {
        assert_eq!(total(17_666, 60), 60, "+999.33µs はスナップする");
        assert_eq!(total(15_667, 60), 60, "−999.67µs はスナップする");
        assert_eq!(total(17_668, 60), 63, "+1001µs はスナップしない");
        assert_eq!(total(15_666, 60), 56, "−1001µs はスナップしない");
        assert_eq!(total(34_333, 60), 120, "2 tick の +999.67µs");
        assert_eq!(total(34_335, 60), 123, "2 tick の +1001µs");
    }

    #[test]
    fn cap_and_dropped_ticks() {
        let mut c = FrameClock::new(MonoTime(0), 1);
        assert_eq!(run(&mut c, 0, &[5_000_000]), [4]);
        assert_eq!(c.dropped_ticks(), 2);
        assert_eq!(run(&mut c, 5_000_000, &[16_667]), [1]);
    }

    #[test]
    fn paused_clears_the_remainder() {
        let mut c = FrameClock::new(MonoTime(0), 1);
        c.advance(MonoTime(25_000), true);
        assert_eq!(c.advance(MonoTime(1_000_000), false), 0);
        assert_eq!(c.advance(MonoTime(1_016_667), true), 1);
    }

    #[test]
    fn time_scale_multiplies_steps_and_cap() {
        let mut c = FrameClock::new(MonoTime(0), 4);
        assert_eq!(run(&mut c, 0, &[16_667]), [4]);
        assert_eq!(run(&mut c, 16_667, &[100_000]), [16]);
        assert_eq!(c.dropped_ticks(), 8);
    }

    #[test]
    fn time_going_backwards_counts_as_zero() {
        let mut c = FrameClock::new(MonoTime(1_000), 1);
        assert_eq!(c.advance(MonoTime(500), true), 0);
    }
}
