//! SDL のオーディオデバイスとコールバック、命令の受け渡し [14 §4.1、§4.5、§4.7]。

use std::time::Instant;

use sdl2::AudioSubsystem;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use tm_platform_api::{AudioCmd, AudioStats, SoundData};

use super::mixer::{Mixer, MixerReport, SAMPLE_RATE};
use crate::config::AudioConfig;

const REPORT_INTERVAL_US: u64 = 1_000_000;
#[cfg(test)]
const CAPTURE_LEN: usize = 8192;

/// コールバックの間隔の計測。
#[derive(Default)]
pub(crate) struct CallbackTiming {
    expected_us: u64,
    last: Option<Instant>,
    calls: u64,
    late: u32,
    max_gap_us: u32,
}

impl CallbackTiming {
    fn new(expected_us: u64) -> Self {
        CallbackTiming {
            expected_us,
            ..Default::default()
        }
    }

    fn tick(&mut self, now: Instant) {
        self.calls += 1;
        if let Some(last) = self.last {
            let gap =
                u64::try_from(now.saturating_duration_since(last).as_micros()).unwrap_or(u64::MAX);
            if gap.saturating_mul(2) > self.expected_us * 3 {
                self.late = self.late.saturating_add(1);
            }
            self.max_gap_us = self.max_gap_us.max(u32::try_from(gap).unwrap_or(u32::MAX));
        }
        self.last = Some(now);
    }
}

/// SDL のオーディオスレッドで動くコールバック。
pub(crate) struct MixerCallback {
    mixer: Mixer,
    timing: CallbackTiming,
    #[cfg(test)]
    capture: Vec<i16>,
}

impl AudioCallback for MixerCallback {
    type Channel = i16;

    fn callback(&mut self, out: &mut [i16]) {
        self.timing.tick(Instant::now());
        self.mixer.render(out);
        #[cfg(test)]
        {
            let n = (self.capture.capacity() - self.capture.len()).min(out.len());
            self.capture.extend_from_slice(&out[..n]);
        }
    }
}

/// 1 秒ごとに読み出す、コールバックとミキサーの数。
struct Report {
    mix: MixerReport,
    late: u32,
    max_gap_us: u32,
}

impl MixerCallback {
    fn take_report(&mut self) -> Report {
        Report {
            mix: self.mixer.take_report(),
            late: std::mem::take(&mut self.timing.late),
            max_gap_us: std::mem::take(&mut self.timing.max_gap_us),
        }
    }
}

/// 開いたデバイスと、メインスレッドの側の集計。
pub(crate) struct AudioOut {
    dev: AudioDevice<MixerCallback>,
    last_report_us: u64,
}

impl AudioOut {
    /// 22050Hz・モノラル・i16 で、一時停止の状態で開く。
    pub(crate) fn open(audio: &AudioSubsystem, cfg: &AudioConfig) -> Result<AudioOut, String> {
        let desired = AudioSpecDesired {
            freq: Some(SAMPLE_RATE as i32),
            channels: Some(1),
            samples: Some(cfg.buffer_samples),
        };
        let dev = audio.open_playback(cfg.device.as_deref(), &desired, |spec| {
            let expected_us = u64::from(spec.samples) * 1_000_000 / u64::from(SAMPLE_RATE);
            MixerCallback {
                mixer: Mixer::new(),
                timing: CallbackTiming::new(expected_us),
                #[cfg(test)]
                capture: Vec::with_capacity(CAPTURE_LEN),
            }
        })?;
        let spec = dev.spec();
        log::info!(
            "音のデバイス({}): {}Hz、{}ch、{:?}、{} サンプル",
            audio.current_audio_driver(),
            spec.freq,
            spec.channels,
            spec.format,
            spec.samples
        );
        Ok(AudioOut {
            dev,
            last_report_us: 0,
        })
    }

    /// 音の表をミキサーに渡し、一時停止を解く。
    pub(crate) fn start(&mut self, sounds: &'static [SoundData]) {
        self.dev.lock().mixer.set_sounds(sounds);
        self.dev.resume();
    }

    /// 命令を渡す。1 秒ごとに数を読み出して `stats` に足す。命令も読み出しもなければロックを取らない。
    pub(crate) fn send(&mut self, cmds: &[AudioCmd], now_us: u64, stats: &mut AudioStats) {
        let report_due = now_us.saturating_sub(self.last_report_us) >= REPORT_INTERVAL_US;
        if cmds.is_empty() && !report_due {
            return;
        }
        let report = {
            let mut g = self.dev.lock();
            for c in cmds {
                g.mixer.enqueue(*c);
            }
            report_due.then(|| g.take_report())
        };
        if let Some(r) = report {
            self.last_report_us = now_us;
            stats.dropped_cmds = stats.dropped_cmds.saturating_add(r.mix.dropped_cmds);
            stats.dropped_jingles = stats.dropped_jingles.saturating_add(r.mix.dropped_jingles);
            stats.invalid_cmds = stats.invalid_cmds.saturating_add(r.mix.invalid_cmds);
            stats.stolen_se = stats.stolen_se.saturating_add(r.mix.stolen_se);
            stats.late_callbacks = stats.late_callbacks.saturating_add(r.late);
            stats.max_callback_gap_us = r.max_gap_us;
            if r.mix != MixerReport::default() {
                log::warn!("音の命令: {:?}", r.mix);
            }
        }
    }

    /// これまでにコールバックが呼ばれた回数。
    pub(crate) fn callback_count(&mut self) -> u64 {
        self.dev.lock().timing.calls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tm_assets::sound::{BGM_TITLE, JINGLE_GOAL, SE_COIN};

    #[test]
    fn timing_counts_late_and_max_gap() {
        let t0 = Instant::now();
        let mut t = CallbackTiming::new(46_439);
        t.tick(t0);
        t.tick(t0 + Duration::from_micros(46_000));
        t.tick(t0 + Duration::from_micros(46_000 + 69_658));
        t.tick(t0 + Duration::from_micros(46_000 + 69_658 + 69_659));
        assert_eq!(t.calls, 4);
        assert_eq!(t.late, 1);
        assert_eq!(t.max_gap_us, 69_659);
    }

    fn offline(sounds: &'static [SoundData], cmds: &[AudioCmd]) -> Vec<i16> {
        let mut m = Mixer::new();
        m.set_sounds(sounds);
        for c in cmds {
            m.enqueue(*c);
        }
        let mut out = vec![0; CAPTURE_LEN];
        m.render(&mut out);
        out
    }

    /// dummy ドライバでデバイスを一時停止の状態で開く。
    fn open_dummy(buffer_samples: u16) -> AudioOut {
        sdl2::hint::set("SDL_AUDIODRIVER", "dummy");
        let audio = sdl2::init().unwrap().audio().unwrap();
        let cfg = AudioConfig {
            enabled: true,
            buffer_samples,
            device: None,
        };
        AudioOut::open(&audio, &cfg).unwrap()
    }

    fn wait_callbacks(out: &mut AudioOut, n: u64) {
        let start = Instant::now();
        while out.callback_count() < n {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "コールバックが進まない"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// dummy ドライバのコールバックの出力が、同じ命令を与えたミキサーの出力とサンプル単位で一致する。
    #[test]
    fn dummy_device_outputs_bgm_and_se() {
        let _guard = crate::tests::sdl_lock();
        let mut out = open_dummy(1024);
        let sounds = tm_assets::bundle().unwrap().sounds;
        let cmds = [
            AudioCmd::PlayBgm {
                id: BGM_TITLE,
                crossfade_ms: 0,
            },
            AudioCmd::PlaySe(SE_COIN),
            AudioCmd::PlayJingle(BGM_TITLE),
        ];
        let mut stats = AudioStats::default();
        out.send(&cmds, 0, &mut stats);
        out.start(sounds);
        let start = Instant::now();
        while out.dev.lock().capture.len() < CAPTURE_LEN {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "コールバックが進まない"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let got = out.dev.lock().capture.clone();
        assert_eq!(got, offline(sounds, &cmds));
        let bgm_only = offline(sounds, &cmds[..1]);
        let coin = sounds[usize::from(SE_COIN.0)].samples.len();
        assert_ne!(got[..coin], bgm_only[..coin]);
        assert_eq!(got[coin..], bgm_only[coin..]);
        assert!(got[coin..].iter().any(|s| *s != 0));
        out.send(&[AudioCmd::PlayJingle(JINGLE_GOAL)], 1_000_000, &mut stats);
        assert_eq!(stats.invalid_cmds, 1, "{stats:?}");
        assert_eq!((stats.dropped_cmds, stats.dropped_jingles), (0, 0));
        assert!(stats.max_callback_gap_us > 0, "{stats:?}");
        assert!(out.callback_count() >= 2);
    }

    #[test]
    fn dropped_jingles_are_accumulated() {
        let _guard = crate::tests::sdl_lock();
        let mut out = open_dummy(512);
        let mut stats = AudioStats::default();
        out.send(&[AudioCmd::PlayJingle(JINGLE_GOAL); 36], 0, &mut stats);
        out.start(tm_assets::bundle().unwrap().sounds);
        wait_callbacks(&mut out, 1);
        out.send(&[], 1_000_000, &mut stats);
        assert_eq!(stats.dropped_jingles, 3, "{stats:?}");
        out.send(&[], 1_500_000, &mut stats);
        assert_eq!(stats.dropped_jingles, 3);
        out.send(&[], 2_000_000, &mut stats);
        assert_eq!(stats.dropped_jingles, 3);
    }

    /// 読み出しは 1 秒に 1 回だけで、`max_callback_gap_us` は直近の読み出しの値で置き換える。
    #[test]
    fn report_is_read_once_a_second() {
        let _guard = crate::tests::sdl_lock();
        let mut out = open_dummy(512);
        out.start(tm_assets::bundle().unwrap().sounds);
        wait_callbacks(&mut out, 3);
        let mut stats = AudioStats {
            max_callback_gap_us: u32::MAX,
            ..AudioStats::default()
        };
        out.send(&[AudioCmd::PlaySe(SE_COIN)], 999_999, &mut stats);
        assert_eq!(stats.max_callback_gap_us, u32::MAX);
        out.send(&[], 1_000_000, &mut stats);
        assert!(
            (1..u32::MAX).contains(&stats.max_callback_gap_us),
            "{stats:?}"
        );
        stats.max_callback_gap_us = u32::MAX;
        out.send(&[AudioCmd::PlaySe(SE_COIN)], 1_999_999, &mut stats);
        out.send(&[], 1_999_999, &mut stats);
        assert_eq!(stats.max_callback_gap_us, u32::MAX);
        out.send(&[], 2_000_000, &mut stats);
        assert!(stats.max_callback_gap_us < u32::MAX);
    }

    #[test]
    fn take_report_resets_late_and_gap() {
        let mut cb = MixerCallback {
            mixer: Mixer::new(),
            timing: CallbackTiming::new(46_439),
            capture: Vec::new(),
        };
        let t0 = Instant::now();
        cb.timing.tick(t0);
        cb.timing.tick(t0 + Duration::from_micros(80_000));
        cb.callback(&mut [0; 16]);
        let r = cb.take_report();
        assert_eq!(r.late, 1);
        assert!(r.max_gap_us >= 80_000);
        let r = cb.take_report();
        assert_eq!((r.late, r.max_gap_us), (0, 0));
        assert_eq!(cb.timing.calls, 3);
    }
}
