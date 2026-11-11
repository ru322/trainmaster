//! 1 チャンネルで鳴っている音の状態と、音色による変調。

use super::env::{Env, quantize};
use super::osc::{Lfsr, pitch_inc, pulse, saw, sine, triangle};
use crate::schema::{Duty, NoiseMode, Wave};
use crate::tables::{NOISE_SHIFT_Q16, SAMPLE_RATE, SINE256};

/// ミリ秒をサンプル数にする(切り捨て)。
pub(crate) fn samples(ms: u32) -> u32 {
    (u64::from(ms) * u64::from(SAMPLE_RATE) / 1000) as u32
}

/// 周波数(ミリヘルツ)を LFO の位相の増分にする。
pub(crate) fn lfo_inc(rate_millihz: u32) -> u32 {
    ((u64::from(rate_millihz) << 32) / (u64::from(SAMPLE_RATE) * 1000)) as u32
}

pub(crate) fn lfo_cents(depth: i32, lfo_phase: u32) -> i32 {
    depth * i32::from(SINE256[(lfo_phase >> 24) as usize]) / 8192
}

#[derive(Clone, Debug)]
pub(crate) struct SampleSeq<T> {
    pub steps: Vec<T>,
    pub step: u32,
    pub looped: bool,
}

impl<T: Copy> SampleSeq<T> {
    fn at(&self, age: u32) -> T {
        let i = (age / self.step) as usize;
        let last = self.steps.len() - 1;
        if self.looped {
            self.steps[i % self.steps.len()]
        } else {
            self.steps[i.min(last)]
        }
    }
}

/// 時間をサンプル数にした音色。
#[derive(Clone, Debug)]
pub(crate) struct Inst {
    pub wave: Wave,
    pub attack: u32,
    pub decay: u32,
    pub sustain: u8,
    pub release: u32,
    pub volume: u8,
    pub quantize: bool,
    pub arp: Option<SampleSeq<i8>>,
    pub duty_seq: Option<SampleSeq<Duty>>,
    pub vibrato: Option<(i32, u32, u32)>,
    pub slide: Option<(i32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Pitch(i32),
    Noise(u8),
}

#[derive(Clone, Debug)]
pub(crate) struct Voice {
    pub inst: usize,
    pub tone: Tone,
    age: u32,
    phase: u32,
    noise_acc: u32,
    env: Env,
}

impl Voice {
    pub(crate) fn new(inst_id: usize, inst: &Inst, tone: Tone) -> Voice {
        Voice {
            inst: inst_id,
            tone,
            age: 0,
            phase: 0,
            noise_acc: 0,
            env: Env::new(inst.attack, inst.decay, inst.sustain, inst.release),
        }
    }

    /// 位相とエンベロープを保ったまま、音の高さと音色を変える(ポルタメント)。
    pub(crate) fn legato(&mut self, inst_id: usize, tone: Tone) {
        self.inst = inst_id;
        self.tone = tone;
    }

    pub(crate) fn release(&mut self) {
        self.env.release();
    }

    pub(crate) fn is_off(&self) -> bool {
        self.env.is_off()
    }

    /// 変調を除いた音の高さ(セント)。ノイズは None。
    pub(crate) fn base_pitch(&self) -> Option<i32> {
        match self.tone {
            Tone::Pitch(p) => Some(p),
            Tone::Noise(_) => None,
        }
    }

    /// 1 サンプル分を出して状態を進める。`gain` は行の音量 × チャンネルの音量(最大 15 × 15)。
    pub(crate) fn next(
        &mut self,
        inst: &Inst,
        fx_cents: i32,
        fx_duty: Option<Duty>,
        lfsr: &mut Lfsr,
        gain: i64,
    ) -> i32 {
        let env = if inst.quantize {
            quantize(self.env.value())
        } else {
            self.env.value()
        };
        let wave = match (inst.wave, self.tone) {
            (Wave::Noise { mode }, Tone::Noise(n)) => {
                let v = lfsr.output();
                self.step_noise(mode, n, lfsr);
                v
            }
            (wave, _) => {
                let v = match wave {
                    Wave::Pulse { duty } => {
                        let seq = inst.duty_seq.as_ref().map(|s| s.at(self.age));
                        pulse(self.phase, fx_duty.or(seq).unwrap_or(duty))
                    }
                    Wave::Triangle => triangle(self.phase),
                    Wave::Saw => saw(self.phase),
                    Wave::Sine => sine(self.phase),
                    Wave::Noise { .. } => 0,
                };
                let pitch = self.base_pitch().unwrap_or(0) + fx_cents + self.inst_cents(inst);
                self.phase = self.phase.wrapping_add(pitch_inc(pitch));
                v
            }
        };
        self.env.advance();
        self.age = self.age.saturating_add(1);
        let num = i64::from(wave) * i64::from(env) * i64::from(inst.volume) * gain;
        (num / (65_536 * 100 * 15 * 15)) as i32
    }

    fn step_noise(&mut self, mode: NoiseMode, n: u8, lfsr: &mut Lfsr) {
        self.noise_acc += NOISE_SHIFT_Q16[usize::from(n)];
        for _ in 0..self.noise_acc >> 16 {
            lfsr.shift(mode);
        }
        self.noise_acc &= 0xFFFF;
    }

    fn inst_cents(&self, inst: &Inst) -> i32 {
        let mut c = 0;
        if let Some(arp) = &inst.arp {
            c += 100 * i32::from(arp.at(self.age));
        }
        if let Some((depth, inc, delay)) = inst.vibrato
            && self.age >= delay
        {
            let lfo = (u64::from(inc) * u64::from(self.age - delay)) as u32;
            c += lfo_cents(depth, lfo);
        }
        if let Some((cps, len)) = inst.slide {
            c += (i64::from(cps) * i64::from(self.age.min(len)) / i64::from(SAMPLE_RATE)) as i32;
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_and_lfo() {
        assert_eq!(samples(1000), 22050);
        assert_eq!(samples(2), 44);
        assert_eq!(samples(1), 22);
        assert_eq!(lfo_inc(5500), 1_071_307);
        assert_eq!(lfo_cents(15, 0x4000_0000), 15);
        assert_eq!(lfo_cents(15, 0xC000_0000), -15);
    }
}
