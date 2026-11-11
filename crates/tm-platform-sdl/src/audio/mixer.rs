//! SDL に依存しないミキサー。整数だけで合成し、確保・ログ・ブロックをしない [14 §4.2〜§4.4]。

use tm_platform_api::{AudioBus, AudioCmd, SoundData, SoundId, SoundKind};

use super::gain::{
    LEVEL_RAMP_SAMPLES, MAX_LEVEL, Ramp, STOP_FADE_SAMPLES, UNITY, level_gain, ms_to_samples,
};

/// 効果音の枠の数。
pub const SE_SLOTS: usize = 8;
/// 命令キューの長さ。
pub const CMD_QUEUE_LEN: usize = 128;
/// ジングルの待ち行列の長さ。
pub const JINGLE_QUEUE_LEN: usize = 32;
/// サンプリング周波数(Hz)。
pub const SAMPLE_RATE: u32 = 22050;
/// 音楽の音量の初期値(01 §12.4)。
pub const INITIAL_BGM_LEVEL: u8 = 6;
/// 効果音の音量の初期値(01 §12.4)。
pub const INITIAL_SE_LEVEL: u8 = 8;

/// 捨てた・無視した命令の数。`take_report` で読み出す。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MixerReport {
    /// 命令キューがあふれて捨てた数。
    pub dropped_cmds: u32,
    /// ジングルの待ち行列があふれて捨てた数。
    pub dropped_jingles: u32,
    /// 範囲外や種類の違う ID で無視した数。
    pub invalid_cmds: u32,
    /// 効果音の枠が埋まっていて古い音を止めた数。
    pub stolen_se: u32,
}

#[derive(Clone, Copy)]
struct FixedQueue<T: Copy, const N: usize> {
    buf: [Option<T>; N],
    head: usize,
    len: usize,
}

impl<T: Copy, const N: usize> FixedQueue<T, N> {
    const fn new() -> Self {
        FixedQueue {
            buf: [None; N],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, v: T) -> bool {
        if self.len == N {
            return false;
        }
        self.buf[(self.head + self.len) % N] = Some(v);
        self.len += 1;
        true
    }

    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        let v = self.buf[self.head].take();
        self.head = (self.head + 1) % N;
        self.len -= 1;
        v
    }

    fn clear(&mut self) {
        while self.pop().is_some() {}
    }
}

#[derive(Clone, Copy)]
struct Voice {
    id: SoundId,
    data: &'static [i16],
    loop_start: Option<u32>,
    pos: u32,
    gain: Ramp,
    stop_when_silent: bool,
}

impl Voice {
    fn new(id: SoundId, sound: &'static SoundData, pos: u32, gain: Ramp) -> Voice {
        Voice {
            id,
            data: sound.samples,
            loop_start: sound.loop_start,
            pos,
            gain,
            stop_when_silent: false,
        }
    }

    fn fade_out(mut self, samples: u32) -> Voice {
        self.gain.retarget(0, samples);
        self.stop_when_silent = true;
        self
    }

    /// 1 サンプル進め、ゲインを掛けた値と、次のサンプルも鳴るかを返す。
    fn tick(&mut self) -> (i32, bool) {
        let Some(&s) = self.data.get(self.pos as usize) else {
            return (0, false);
        };
        self.gain.step();
        let out = ((i64::from(s) * i64::from(self.gain.cur())) >> 16) as i32;
        self.pos += 1;
        if self.pos as usize >= self.data.len() {
            match self.loop_start {
                Some(ls) => self.pos = ls,
                None => return (out, false),
            }
        }
        (out, !(self.stop_when_silent && self.gain.is_silent()))
    }
}

fn tick_slot(slot: &mut Option<Voice>) -> i32 {
    let Some(v) = slot else { return 0 };
    let (s, alive) = v.tick();
    if !alive {
        *slot = None;
    }
    s
}

fn fade_in(samples: u32) -> Ramp {
    let mut g = Ramp::fixed(0);
    g.retarget(UNITY, samples);
    g
}

#[derive(Clone, Copy)]
struct SeVoice {
    v: Voice,
    seq: u64,
    stopping: bool,
}

#[derive(Clone, Copy)]
struct Held {
    id: SoundId,
    pos: u32,
}

/// Bgm バス(BGM 2 + ジングル 1)と Se バス(8 枠)のミキサー。
pub struct Mixer {
    sounds: &'static [SoundData],
    cmds: FixedQueue<AudioCmd, CMD_QUEUE_LEN>,
    bgm_cur: Option<Voice>,
    bgm_out: Option<Voice>,
    bgm_held: Option<Held>,
    bgm_next: Option<SoundId>,
    jingle: Option<Voice>,
    jingle_q: FixedQueue<SoundId, JINGLE_QUEUE_LEN>,
    se: [Option<SeVoice>; SE_SLOTS],
    se_seq: u64,
    bgm_level: u8,
    se_level: u8,
    duck: bool,
    bgm_bus: Ramp,
    se_bus: Ramp,
    report: MixerReport,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    /// 音の表は空(無音)、音量は初期値。
    pub fn new() -> Self {
        Mixer {
            sounds: &[],
            cmds: FixedQueue::new(),
            bgm_cur: None,
            bgm_out: None,
            bgm_held: None,
            bgm_next: None,
            jingle: None,
            jingle_q: FixedQueue::new(),
            se: [None; SE_SLOTS],
            se_seq: 0,
            bgm_level: INITIAL_BGM_LEVEL,
            se_level: INITIAL_SE_LEVEL,
            duck: false,
            bgm_bus: Ramp::fixed(level_gain(INITIAL_BGM_LEVEL, false)),
            se_bus: Ramp::fixed(level_gain(INITIAL_SE_LEVEL, false)),
            report: MixerReport::default(),
        }
    }

    /// 音の表を渡す。`SoundId` の番号順。
    pub fn set_sounds(&mut self, sounds: &'static [SoundData]) {
        self.sounds = sounds;
    }

    /// 命令を積む。キューが満杯なら捨てて数え、false を返す。
    pub fn enqueue(&mut self, cmd: AudioCmd) -> bool {
        let ok = self.cmds.push(cmd);
        if !ok {
            self.report.dropped_cmds = self.report.dropped_cmds.saturating_add(1);
        }
        ok
    }

    /// 積まれた命令を順に適用してから `out` を埋める。
    pub fn render(&mut self, out: &mut [i16]) {
        while let Some(cmd) = self.cmds.pop() {
            self.apply(cmd);
        }
        for s in out {
            *s = self.next_sample();
        }
    }

    /// 数を返して 0 に戻す。
    pub fn take_report(&mut self) -> MixerReport {
        std::mem::take(&mut self.report)
    }

    fn sound(&self, id: SoundId, kind: SoundKind) -> Option<&'static SoundData> {
        self.sounds
            .get(usize::from(id.0))
            .filter(|s| s.kind == kind)
    }

    fn invalid(&mut self) {
        self.report.invalid_cmds = self.report.invalid_cmds.saturating_add(1);
    }

    fn apply(&mut self, cmd: AudioCmd) {
        match cmd {
            AudioCmd::PlayBgm { id, crossfade_ms } => match self.sound(id, SoundKind::Bgm) {
                Some(s) => self.play_bgm(id, s, crossfade_ms),
                None => self.invalid(),
            },
            AudioCmd::StopBgm { fade_ms } => self.stop_bgm(fade_ms),
            AudioCmd::PlayJingle(id) => match self.sound(id, SoundKind::Jingle) {
                Some(s) => self.play_jingle(id, s),
                None => self.invalid(),
            },
            AudioCmd::PlaySe(id) => match self.sound(id, SoundKind::Se) {
                Some(s) => self.play_se(id, s),
                None => self.invalid(),
            },
            AudioCmd::StopSe(id) => match self.sound(id, SoundKind::Se) {
                Some(_) => self.stop_se(id),
                None => self.invalid(),
            },
            AudioCmd::SetBgmDuck(duck) => {
                self.duck = duck;
                self.retarget_bus(AudioBus::Bgm);
            }
            AudioCmd::SetVolume { bus, level } => {
                let level = level.min(MAX_LEVEL);
                match bus {
                    AudioBus::Bgm => self.bgm_level = level,
                    AudioBus::Se => self.se_level = level,
                }
                self.retarget_bus(bus);
            }
        }
    }

    fn retarget_bus(&mut self, bus: AudioBus) {
        match bus {
            AudioBus::Bgm => self
                .bgm_bus
                .retarget(level_gain(self.bgm_level, self.duck), LEVEL_RAMP_SAMPLES),
            AudioBus::Se => self
                .se_bus
                .retarget(level_gain(self.se_level, false), LEVEL_RAMP_SAMPLES),
        }
    }

    fn play_bgm(&mut self, id: SoundId, sound: &'static SoundData, crossfade_ms: u16) {
        if self.jingle.is_some() {
            self.bgm_next = Some(id);
            return;
        }
        if self.bgm_cur.is_some_and(|v| v.id == id) {
            return;
        }
        let (out_fade, gain) = if crossfade_ms == 0 {
            (STOP_FADE_SAMPLES, Ramp::fixed(UNITY))
        } else {
            let n = ms_to_samples(crossfade_ms);
            (n, fade_in(n))
        };
        if let Some(cur) = self.bgm_cur.take() {
            self.bgm_out = Some(cur.fade_out(out_fade));
        }
        self.bgm_cur = Some(Voice::new(id, sound, 0, gain));
    }

    fn stop_bgm(&mut self, fade_ms: u16) {
        self.bgm_held = None;
        self.bgm_next = None;
        if let Some(j) = self.jingle.take() {
            self.jingle_q.clear();
            self.bgm_out = Some(j.fade_out(STOP_FADE_SAMPLES));
        } else if let Some(cur) = self.bgm_cur.take() {
            self.bgm_out = Some(cur.fade_out(ms_to_samples(fade_ms.max(5))));
        }
    }

    fn play_jingle(&mut self, id: SoundId, sound: &'static SoundData) {
        if self.jingle.is_some() {
            if !self.jingle_q.push(id) {
                self.report.dropped_jingles = self.report.dropped_jingles.saturating_add(1);
            }
            return;
        }
        if let Some(cur) = self.bgm_cur.take() {
            self.bgm_held = Some(Held {
                id: cur.id,
                pos: cur.pos,
            });
            self.bgm_out = Some(cur.fade_out(STOP_FADE_SAMPLES));
        }
        self.bgm_next = None;
        self.jingle = Some(Voice::new(id, sound, 0, Ramp::fixed(UNITY)));
    }

    fn on_jingle_end(&mut self) {
        while let Some(id) = self.jingle_q.pop() {
            if let Some(s) = self.sound(id, SoundKind::Jingle) {
                self.jingle = Some(Voice::new(id, s, 0, Ramp::fixed(UNITY)));
                return;
            }
        }
        let held = self.bgm_held.take();
        let Some(id) = self.bgm_next.take().or(held.map(|h| h.id)) else {
            return;
        };
        let Some(s) = self.sound(id, SoundKind::Bgm) else {
            return;
        };
        self.bgm_cur = Some(match held {
            Some(h) if h.id == id => Voice::new(id, s, h.pos, fade_in(STOP_FADE_SAMPLES)),
            _ => Voice::new(id, s, 0, Ramp::fixed(UNITY)),
        });
    }

    fn play_se(&mut self, id: SoundId, sound: &'static SoundData) {
        let seq = self.se_seq;
        self.se_seq = self.se_seq.wrapping_add(1);
        let oldest = |stopping_only: bool| {
            self.se
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    s.filter(|s| s.stopping || !stopping_only)
                        .map(|s| (s.seq, i))
                })
                .min()
                .map(|(_, i)| i)
        };
        let idx = match self.se.iter().position(Option::is_none).or(oldest(true)) {
            Some(i) => i,
            None => {
                self.report.stolen_se = self.report.stolen_se.saturating_add(1);
                oldest(false).unwrap_or(0)
            }
        };
        self.se[idx] = Some(SeVoice {
            v: Voice::new(id, sound, 0, Ramp::fixed(UNITY)),
            seq,
            stopping: false,
        });
    }

    fn stop_se(&mut self, id: SoundId) {
        if let Some(s) = self
            .se
            .iter_mut()
            .flatten()
            .filter(|s| s.v.id == id && !s.stopping)
            .min_by_key(|s| s.seq)
        {
            s.stopping = true;
            s.v = s.v.fade_out(STOP_FADE_SAMPLES);
        }
    }

    fn next_sample(&mut self) -> i16 {
        let mut bgm = tick_slot(&mut self.bgm_cur) + tick_slot(&mut self.bgm_out);
        let jingle_ended = match &mut self.jingle {
            Some(j) => {
                let (s, alive) = j.tick();
                bgm += s;
                !alive
            }
            None => false,
        };
        if jingle_ended {
            self.jingle = None;
            self.on_jingle_end();
        }
        let mut se = 0i32;
        for slot in &mut self.se {
            if let Some(v) = slot {
                let (s, alive) = v.v.tick();
                se += s;
                if !alive {
                    *slot = None;
                }
            }
        }
        self.bgm_bus.step();
        self.se_bus.step();
        let mix = ((i64::from(bgm) * i64::from(self.bgm_bus.cur())) >> 16)
            + ((i64::from(se) * i64::from(self.se_bus.cur())) >> 16);
        mix.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
    }
}

/// テスト用: 1 つの声の状態。
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceSnap {
    /// 音の ID。
    pub id: SoundId,
    /// 次に読むサンプルの位置。
    pub pos: u32,
    /// いまのゲイン(Q16)。
    pub gain: u32,
    /// ゲインの目標。
    pub target: u32,
    /// 目標に着くまでのサンプル数。
    pub remaining: u32,
}

/// テスト用: ミキサーの状態。
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixerSnapshot {
    /// 鳴っている BGM。
    pub bgm_cur: Option<VoiceSnap>,
    /// 消えていく BGM・ジングル。
    pub bgm_out: Option<VoiceSnap>,
    /// ジングルのために止めた BGM(ID と位置)。
    pub bgm_held: Option<(SoundId, u32)>,
    /// ジングルの間に来た BGM。
    pub bgm_next: Option<SoundId>,
    /// 鳴っているジングル。
    pub jingle: Option<VoiceSnap>,
    /// ジングルの待ち行列の長さ。
    pub jingle_q: usize,
    /// 効果音の枠(声、通し番号、止めている途中か)。
    pub se: [Option<(VoiceSnap, u64, bool)>; SE_SLOTS],
    /// Bgm バスのゲイン。
    pub bgm_bus: u32,
    /// Bgm バスのゲインの目標。
    pub bgm_bus_target: u32,
    /// Se バスのゲイン。
    pub se_bus: u32,
    /// Se バスのゲインの目標。
    pub se_bus_target: u32,
    /// 命令キューの長さ。
    pub cmds: usize,
}

#[cfg(test)]
impl Voice {
    fn snap(&self) -> VoiceSnap {
        VoiceSnap {
            id: self.id,
            pos: self.pos,
            gain: self.gain.cur(),
            target: self.gain.target(),
            remaining: self.gain.remaining(),
        }
    }
}

#[cfg(test)]
impl Mixer {
    /// テスト用: 各枠の状態。
    pub fn snapshot(&self) -> MixerSnapshot {
        MixerSnapshot {
            bgm_cur: self.bgm_cur.map(|v| v.snap()),
            bgm_out: self.bgm_out.map(|v| v.snap()),
            bgm_held: self.bgm_held.map(|h| (h.id, h.pos)),
            bgm_next: self.bgm_next,
            jingle: self.jingle.map(|v| v.snap()),
            jingle_q: self.jingle_q.len,
            se: self.se.map(|s| s.map(|s| (s.v.snap(), s.seq, s.stopping))),
            bgm_bus: self.bgm_bus.cur(),
            bgm_bus_target: self.bgm_bus.target(),
            se_bus: self.se_bus.cur(),
            se_bus_target: self.se_bus.target(),
            cmds: self.cmds.len,
        }
    }
}

#[cfg(test)]
#[path = "mixer_tests.rs"]
mod tests;
