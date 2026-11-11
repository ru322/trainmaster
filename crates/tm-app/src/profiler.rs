//! 1 フレームの区間ごとの時間と、step の回数の集計 [02 §9.4、16 §1.14、APP-D-12]。

use log::{Level, log, warn};
use tm_platform_api::{AudioStats, MonoTime, PlatformStats};

use crate::clock::{FrameClock, unsnapped_steps};

/// 集計してログに出す間隔(フレーム)。
pub const SUMMARY_FRAMES: u32 = 600;
/// 区間の数(入力、update、シミュレーション、render、audio + present)。
pub const SECTIONS: usize = 5;
/// 区間の名前。
pub const SECTION_NAMES: [&str; SECTIONS] = ["input", "update", "sim", "render", "out"];

/// 1 フレームの区間の境目の時刻。
#[derive(Clone, Copy, Debug)]
pub struct FrameTimes {
    /// フレームの始め。
    pub input: MonoTime,
    /// `ui.update` の前。
    pub update: MonoTime,
    /// シミュレーションの前。
    pub sim: MonoTime,
    /// `ui.render` の前。
    pub draw: MonoTime,
    /// `audio` と `present` の前。
    pub out: MonoTime,
    /// `present` の後。
    pub end: MonoTime,
}

impl FrameTimes {
    fn sections(&self) -> [u64; SECTIONS] {
        let t = [
            self.input,
            self.update,
            self.sim,
            self.draw,
            self.out,
            self.end,
        ];
        std::array::from_fn(|i| t[i + 1].saturating_since(t[i]))
    }
}

/// step の回数ごとのフレーム数(0、1、2、3、4 以上)。
pub type StepHistogram = [u64; 5];

/// 集計。`window` は直近の `SUMMARY_FRAMES` の分、`total` は起動からの累計。
#[derive(Clone, Debug, Default)]
pub struct FrameProfiler {
    verbose: bool,
    frames: u32,
    sum_us: [u64; SECTIONS],
    max_us: [u64; SECTIONS],
    last_us: [u64; SECTIONS],
    window_steps: StepHistogram,
    total_steps: StepHistogram,
    shadow_snapped: StepHistogram,
    shadow_unsnapped: StepHistogram,
    shadow_clock: Option<FrameClock>,
    unsnapped_acc: u64,
    prev_start: Option<MonoTime>,
    window_interval_sum: u64,
    interval_dev: [u64; 4],
    last_interval_us: u64,
    last_platform: PlatformStats,
    last_audio: AudioStats,
    dropped_ticks: u64,
}

fn bucket(n: u32) -> usize {
    (n as usize).min(4)
}

impl FrameProfiler {
    /// `verbose` なら集計を `info` で出す(devtools)。そうでなければ `debug`。
    pub fn new(verbose: bool) -> Self {
        FrameProfiler {
            verbose,
            ..Default::default()
        }
    }

    /// 1 フレームを記録する。`running` はシミュレーションを進めるフレームか。
    pub fn record(
        &mut self,
        t: FrameTimes,
        steps: u32,
        running: bool,
        dropped_ticks: u64,
        stats: PlatformStats,
    ) {
        let sec = t.sections();
        for (i, us) in sec.iter().enumerate() {
            self.sum_us[i] += us;
            self.max_us[i] = self.max_us[i].max(*us);
        }
        self.last_us = sec;
        if let Some(prev) = self.prev_start {
            let dt = t.input.saturating_since(prev);
            self.last_interval_us = dt;
            self.window_interval_sum += dt;
            let dev = dt.abs_diff(16_667);
            let i = match dev {
                0..=100 => 0,
                101..=500 => 1,
                501..=1_000 => 2,
                _ => 3,
            };
            self.interval_dev[i] += 1;
            let clock = self
                .shadow_clock
                .get_or_insert_with(|| FrameClock::new(prev, 1));
            self.shadow_snapped[bucket(clock.advance(t.input, true))] += 1;
            self.shadow_unsnapped[bucket(unsnapped_steps(&mut self.unsnapped_acc, dt))] += 1;
        }
        if running {
            self.total_steps[bucket(steps)] += 1;
        }
        self.prev_start = Some(t.input);
        self.window_steps[bucket(steps)] += 1;
        self.dropped_ticks = dropped_ticks;
        self.last_platform = stats;
        self.frames += 1;
        if self.frames == SUMMARY_FRAMES {
            self.summarize();
        }
    }

    fn summarize(&mut self) {
        let n = u64::from(self.frames);
        let level = if self.verbose {
            Level::Info
        } else {
            Level::Debug
        };
        let avg: [u64; SECTIONS] = std::array::from_fn(|i| self.sum_us[i] / n);
        log!(
            level,
            "計測 {n} フレーム: 平均/最大 µs input {}/{} update {}/{} sim {}/{} render {}/{} out {}/{}、\
             step 0:{} 1:{} 2:{} 3:{} 4+:{}、間隔の平均 {}µs、捨てた tick {}、present {}µs、四角形 {}",
            avg[0],
            self.max_us[0],
            avg[1],
            self.max_us[1],
            avg[2],
            self.max_us[2],
            avg[3],
            self.max_us[3],
            avg[4],
            self.max_us[4],
            self.window_steps[0],
            self.window_steps[1],
            self.window_steps[2],
            self.window_steps[3],
            self.window_steps[4],
            self.window_interval_sum / n,
            self.dropped_ticks,
            self.last_platform.present_us,
            self.last_platform.draw_quads,
        );
        let a = self.last_platform.audio;
        if a.dropped_cmds > self.last_audio.dropped_cmds
            || a.dropped_jingles > self.last_audio.dropped_jingles
            || a.late_callbacks > self.last_audio.late_callbacks
        {
            warn!(
                "音: 捨てた命令 {}、捨てたジングル {}、遅れたコールバック {}",
                a.dropped_cmds, a.dropped_jingles, a.late_callbacks
            );
        }
        self.last_audio = a;
        self.frames = 0;
        self.sum_us = [0; SECTIONS];
        self.max_us = [0; SECTIONS];
        self.window_steps = [0; 5];
        self.window_interval_sum = 0;
    }

    /// 直前のフレームの区間ごとの時間(µs)。
    pub fn last_sections(&self) -> [u64; SECTIONS] {
        self.last_us
    }

    /// 直前のフレームの間隔(µs)。
    pub fn last_interval_us(&self) -> u64 {
        self.last_interval_us
    }

    /// 直前の `PlatformStats`。
    pub fn last_platform(&self) -> PlatformStats {
        self.last_platform
    }

    /// 進めたフレームの step の回数の分布(起動からの累計。説明で打ち切った分は数えない)。
    pub fn total_steps(&self) -> StepHistogram {
        self.total_steps
    }

    /// 全フレームを進めたと仮定した step の回数の分布。スナップあり・なし(APP-V-02)。
    pub fn shadow_steps(&self) -> (StepHistogram, StepHistogram) {
        (self.shadow_snapped, self.shadow_unsnapped)
    }

    /// フレームの間隔の 16,667µs からのずれの分布(≤100、≤500、≤1000、それ以上 µs)。
    pub fn interval_deviation(&self) -> [u64; 4] {
        self.interval_dev
    }
}
