//! 単調時計とフレームの間隔の制御 [14 §3.7]。

use std::time::Instant;

use crate::config::FrameCap;

const PROBE_FRAMES: u64 = 120;
/// 診断の間に、間隔が閾値未満のフレームがこの数以上あれば vsync が効いていない。
const PROBE_SHORT_FRAMES: u64 = 60;
const FPS60_MIN_US: u64 = 12_000;
const FPS30_EARLY_US: u64 = 4_000;
/// vsync が効けば間隔は 33.3ms、効かなければ 33.3 − 4ms ≒ 29.4ms になるので、その間で分ける。
const FPS30_MIN_US: u64 = 31_500;

/// 起点からのマイクロ秒を返す時計(`CLOCK_MONOTONIC`)。
pub(crate) struct Clock {
    origin: Instant,
}

impl Clock {
    pub(crate) fn new() -> Self {
        Clock {
            origin: Instant::now(),
        }
    }

    pub(crate) fn now_us(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// vsync の自己診断とソフトウェアのフレーム制限。時刻は外から渡す。
pub(crate) struct FramePacer {
    cap: FrameCap,
    vsync_trusted: bool,
    probe_count: u64,
    probe_short: u64,
    last_present: Option<u64>,
    next_target: Option<u64>,
}

impl FramePacer {
    pub(crate) fn new(vsync: bool, cap: FrameCap) -> Self {
        FramePacer {
            cap,
            vsync_trusted: vsync,
            probe_count: 0,
            probe_short: 0,
            last_present: None,
            next_target: None,
        }
    }

    fn period_us(&self) -> u64 {
        match self.cap {
            FrameCap::Fps60 => 16_667,
            FrameCap::Fps30 => 33_333,
        }
    }

    #[cfg(test)]
    fn vsync_trusted(&self) -> bool {
        self.vsync_trusted
    }

    /// present の前に sleep するマイクロ秒。
    pub(crate) fn wait_before_present(&mut self, now: u64) -> u64 {
        if self.vsync_trusted {
            return match (self.cap, self.last_present) {
                (FrameCap::Fps30, Some(prev)) => {
                    (prev + self.period_us() - FPS30_EARLY_US).saturating_sub(now)
                }
                _ => 0,
            };
        }
        let period = self.period_us();
        let target = match self.next_target {
            Some(t) if now <= t + period => t,
            _ => now,
        };
        self.next_target = Some(target + period);
        target.saturating_sub(now)
    }

    /// present の後に呼ぶ。最初の 120 フレームのうち、間隔が 60fps で 12ms、30fps で 31.5ms 未満のものが 60 以上なら
    /// vsync が効いていないとみなし、ソフトウェアの制限に切り替えて true を返す(14 §3.7.2)。
    pub(crate) fn presented(&mut self, now: u64) -> bool {
        let prev = self.last_present.replace(now);
        if !self.vsync_trusted || self.probe_count >= PROBE_FRAMES {
            return false;
        }
        let Some(prev) = prev else {
            return false;
        };
        let min = match self.cap {
            FrameCap::Fps60 => FPS60_MIN_US,
            FrameCap::Fps30 => FPS30_MIN_US,
        };
        self.probe_count += 1;
        if now - prev < min {
            self.probe_short += 1;
        }
        if self.probe_count == PROBE_FRAMES && self.probe_short >= PROBE_SHORT_FRAMES {
            self.vsync_trusted = false;
            return true;
        }
        false
    }
}

/// `present` の中の時刻: 始め、描き終わり(sleep の前)、SDL の present の直後。
#[derive(Clone, Copy, Debug)]
pub(crate) struct PresentTimes {
    pub(crate) start: u64,
    pub(crate) drawn: u64,
    pub(crate) end: u64,
}

/// フレームの制限と、present ごとの計測値。
pub(crate) struct FrameTimer {
    pacer: FramePacer,
    last_end: Option<u64>,
    pub(crate) interval_us: u32,
    pub(crate) present_us: u32,
    pub(crate) render_us: u32,
}

fn to_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

impl FrameTimer {
    pub(crate) fn new(vsync: bool, cap: FrameCap) -> Self {
        FrameTimer {
            pacer: FramePacer::new(vsync, cap),
            last_end: None,
            interval_us: 0,
            present_us: 0,
            render_us: 0,
        }
    }

    pub(crate) fn wait_before_present(&mut self, now: u64) -> u64 {
        self.pacer.wait_before_present(now)
    }

    /// 計測値を更新する。vsync が効いていないと判断して切り替えたら true。
    pub(crate) fn record(&mut self, t: PresentTimes) -> bool {
        self.interval_us = self.last_end.map_or(0, |p| to_u32(t.end - p));
        self.present_us = to_u32(t.end - t.start);
        self.render_us = to_u32(t.drawn - t.start);
        self.last_end = Some(t.end);
        self.pacer.presented(t.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_monotonic() {
        let c = Clock::new();
        let a = c.now_us();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = c.now_us();
        assert!(b >= a + 2_000, "{a} {b}");
        assert!(a < 1_000_000);
    }

    #[test]
    fn software_limiter_paces_to_period() {
        let mut p = FramePacer::new(false, FrameCap::Fps60);
        assert_eq!(p.wait_before_present(1_000), 0);
        p.presented(1_000);
        assert_eq!(p.wait_before_present(5_000), 12_667);
        p.presented(17_667);
        assert_eq!(p.wait_before_present(20_000), 14_334);
    }

    #[test]
    fn software_limiter_resyncs_when_late() {
        let mut p = FramePacer::new(false, FrameCap::Fps60);
        p.wait_before_present(0);
        assert_eq!(p.wait_before_present(16_667 + 16_668), 0);
        assert_eq!(p.wait_before_present(16_667 + 16_668), 16_667);
    }

    #[test]
    fn software_limiter_keeps_target_when_exactly_one_frame_late() {
        let mut p = FramePacer::new(false, FrameCap::Fps60);
        p.wait_before_present(0);
        assert_eq!(p.wait_before_present(33_334), 0);
        assert_eq!(p.wait_before_present(33_334), 0);
        assert_eq!(p.wait_before_present(33_334), 16_667);
    }

    #[test]
    fn software_limiter_within_one_frame_keeps_target() {
        let mut p = FramePacer::new(false, FrameCap::Fps60);
        p.wait_before_present(0);
        assert_eq!(p.wait_before_present(20_000), 0);
        assert_eq!(p.wait_before_present(20_000), 13_334);
    }

    #[test]
    fn fps30_software_period() {
        let mut p = FramePacer::new(false, FrameCap::Fps30);
        p.wait_before_present(0);
        assert_eq!(p.wait_before_present(10_000), 23_333);
    }

    #[test]
    fn fps30_with_vsync_sleeps_until_before_next() {
        let mut p = FramePacer::new(true, FrameCap::Fps30);
        assert_eq!(p.wait_before_present(0), 0);
        p.presented(100_000);
        assert_eq!(p.wait_before_present(110_000), 19_333);
        assert_eq!(p.wait_before_present(140_000), 0);
    }

    #[test]
    fn vsync_probe_switches_when_too_fast() {
        let mut p = FramePacer::new(true, FrameCap::Fps60);
        let mut switched_at = None;
        for i in 0..=PROBE_FRAMES {
            assert_eq!(p.wait_before_present(i * 1_000), 0);
            if p.presented(i * 1_000) {
                switched_at = Some(i);
            }
        }
        assert_eq!(switched_at, Some(PROBE_FRAMES));
        assert!(!p.vsync_trusted());
        assert!(p.wait_before_present(PROBE_FRAMES * 1_000 + 1) == 0);
        assert!(p.wait_before_present(PROBE_FRAMES * 1_000 + 2) > 16_000);
    }

    #[test]
    fn vsync_probe_keeps_vsync_at_60hz() {
        let mut p = FramePacer::new(true, FrameCap::Fps60);
        for i in 0..=PROBE_FRAMES * 2 {
            assert!(!p.presented(i * 16_667));
        }
        assert!(p.vsync_trusted());
        assert_eq!(p.wait_before_present(10_000_000), 0);
    }

    /// 最初の present の後、`short` 回を `short_us`、残りを `long_us` の間隔で 120 回 present する。
    fn probe(cap: FrameCap, short: u64, short_us: u64, long_us: u64) -> FramePacer {
        let mut p = FramePacer::new(true, cap);
        let mut t = 0;
        p.presented(t);
        for i in 0..PROBE_FRAMES {
            t += if i < short { short_us } else { long_us };
            p.presented(t);
        }
        p
    }

    #[test]
    fn vsync_probe_counts_short_intervals() {
        let fps30 = |short| probe(FrameCap::Fps30, short, 29_400, 33_333).vsync_trusted();
        assert!(fps30(59));
        assert!(!fps30(60));
        assert!(!fps30(120));
        let fps60 = |short| probe(FrameCap::Fps60, short, 1_000, 16_667).vsync_trusted();
        assert!(fps60(59));
        assert!(!fps60(60));
        assert!(probe(FrameCap::Fps30, 120, FPS30_MIN_US, 0).vsync_trusted());
        assert!(!probe(FrameCap::Fps30, 120, FPS30_MIN_US - 1, 0).vsync_trusted());
        assert!(probe(FrameCap::Fps60, 120, FPS60_MIN_US, 0).vsync_trusted());
        assert!(!probe(FrameCap::Fps60, 120, FPS60_MIN_US - 1, 0).vsync_trusted());
    }

    #[test]
    fn one_long_stall_does_not_hide_a_missing_vsync() {
        let mut p = FramePacer::new(true, FrameCap::Fps30);
        let mut t = 0;
        p.presented(t);
        for i in 0..PROBE_FRAMES {
            t += if i == 3 { 500_000 } else { 29_400 };
            p.presented(t);
        }
        assert!(!p.vsync_trusted());
    }

    #[test]
    fn probe_runs_only_once() {
        let mut p = probe(FrameCap::Fps60, 0, 0, 16_667);
        for i in 0..PROBE_FRAMES * 2 {
            assert!(!p.presented(10_000_000 + i * 1_000));
        }
        assert!(p.vsync_trusted());
    }

    #[test]
    fn fps30_without_vsync_paces_by_software_after_probe() {
        let mut p = probe(FrameCap::Fps30, 120, 29_400, 0);
        let t = 10_000_000;
        p.wait_before_present(t);
        assert_eq!(p.wait_before_present(t + 1_000), 33_333 - 1_000);
    }

    fn times(start: u64, drawn: u64, end: u64) -> PresentTimes {
        PresentTimes { start, drawn, end }
    }

    #[test]
    fn timer_measures_interval_present_and_render() {
        let mut t = FrameTimer::new(false, FrameCap::Fps60);
        t.record(times(1_000, 3_000, 16_000));
        assert_eq!(
            (t.interval_us, t.present_us, t.render_us),
            (0, 15_000, 2_000)
        );
        t.record(times(46_000, 47_000, 48_000));
        assert_eq!(
            (t.interval_us, t.present_us, t.render_us),
            (32_000, 2_000, 1_000)
        );
    }

    #[test]
    fn timer_paces_fps30_from_the_end_of_present() {
        let mut t = FrameTimer::new(true, FrameCap::Fps30);
        t.record(times(0, 1_000, 10_000));
        assert_eq!(t.wait_before_present(10_000), 33_333 - FPS30_EARLY_US);
    }
}
