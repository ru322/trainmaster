//! 譜面を解決して i16 の列に合成する。ループの継ぎ目の処理と検査(14 §5.4、§5.5)。

use std::collections::BTreeMap;

use crate::SoundKind;
use crate::load::Merged;
use crate::schema::{Channel, Duty, Instrument, Seq, Timing, Wave};
use crate::synth::osc::Lfsr;
use crate::synth::voice::{Inst, SampleSeq, Tone, Voice, lfo_cents, lfo_inc, samples};
use crate::tables::SAMPLE_RATE;
use crate::tracker::{Fx, InstRef, NoteCmd, RowError, parse_row};
use crate::validate::{Code, Diagnostic, check_length};

const SR: u64 = SAMPLE_RATE as u64;
/// ループの継ぎ目のために続けて合成する長さ(1 秒)。
const SEAM: u64 = SR;
/// 終わりのフェードの長さ(2ms)。
const FADE: usize = 44;
/// 直流除去の係数(約 20Hz。14 §5.4 PL-D-30)。
const DC_A: i64 = 32_581;

/// 1 次のハイパス `y[n] = x[n] − x[n−1] + DC_A × y[n−1] / 32768`(ゼロ方向への切り捨て)。
#[derive(Default)]
struct DcBlock {
    x: i64,
    y: i64,
}

impl DcBlock {
    fn next(&mut self, x: i64) -> i64 {
        self.y = x - self.x + DC_A * self.y / 32_768;
        self.x = x;
        self.y
    }
}

#[derive(Clone, Debug)]
struct Row {
    note: NoteCmd,
    inst: Option<usize>,
    vol: Option<u8>,
    fx: Vec<Fx>,
}

#[derive(Clone, Debug)]
struct OrderLine {
    cells: Vec<Option<usize>>,
    transpose: i8,
    rows: usize,
}

/// 名前を番号に解決した曲。
#[derive(Clone, Debug)]
pub(crate) struct Song {
    label: String,
    kind: SoundKind,
    timing: Timing,
    master: u16,
    channels: Vec<Channel>,
    insts: Vec<Inst>,
    patterns: Vec<(String, Vec<Row>)>,
    order: Vec<OrderLine>,
    loop_to: Option<usize>,
    tail: u64,
}

/// 合成の結果。
pub(crate) struct Output {
    pub samples: Vec<i16>,
    pub loop_start: Option<u32>,
    pub clipped: u64,
}

/// 行 k の始まりのサンプル番号。
pub(crate) fn row_start(timing: Timing, k: u64) -> u64 {
    match timing {
        Timing::Bpm { bpm, rows_per_beat } => {
            k * 1_323_000 / (u64::from(bpm) * u64::from(rows_per_beat))
        }
        Timing::RowMs(m) => k * u64::from(m) * SR / 1000,
    }
}

/// 効果の tick j の境目(効果を置いた時刻からのサンプル数)。
pub(crate) fn tick_start(j: u64) -> u64 {
    j * 735 / 2
}

/// 効果を置いてから `d` サンプル後までに過ぎた tick の数。
fn ticks_since(d: u64) -> u64 {
    (2 * d + 1) / 735
}

fn to_seq<T: Clone>(s: &Seq<T>) -> SampleSeq<T> {
    SampleSeq {
        steps: s.steps.clone(),
        step: samples(u32::from(s.step_ms)).max(1),
        looped: s.looped,
    }
}

fn to_inst(i: &Instrument) -> Inst {
    Inst {
        wave: i.wave,
        attack: samples(u32::from(i.env.attack_ms)),
        decay: samples(u32::from(i.env.decay_ms)),
        sustain: i.env.sustain,
        release: samples(u32::from(i.env.release_ms)),
        volume: i.volume,
        quantize: i.quantize,
        arp: i.arp.as_ref().map(to_seq),
        duty_seq: i.duty_seq.as_ref().map(to_seq),
        vibrato: i.vibrato.map(|v| {
            (
                i32::from(v.depth_cents),
                lfo_inc(v.rate_millihz),
                samples(u32::from(v.delay_ms)),
            )
        }),
        slide: i.slide.map(|s| (s.cents_per_sec, samples(u32::from(s.ms)))),
    }
}

impl Song {
    /// パターンの解析と名前の解決(E5、E6)。`check_def` を通った定義を受け取る。
    pub(crate) fn compile(label: &str, kind: SoundKind, m: &Merged) -> Result<Song, Diagnostic> {
        let e = |code, msg: String| Diagnostic::new(code, label, &msg);
        let inst_ids: BTreeMap<&str, usize> = m
            .instruments
            .keys()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect();
        let insts = m.instruments.values().map(to_inst).collect();
        let mut pat_ids = BTreeMap::new();
        let mut patterns = Vec::new();
        for (name, p) in &m.patterns {
            if p.rows.is_empty() {
                return Err(e(Code::E6, format!("パターン {name} の行がない")));
            }
            let mut rows = Vec::with_capacity(p.rows.len());
            for (r, line) in p.rows.iter().enumerate() {
                let row = parse_row(line).map_err(|err| {
                    let code = match err {
                        RowError::Syntax(_) => Code::E6,
                        RowError::Range(_) => Code::E7,
                    };
                    e(code, format!("パターン {name} の行 {r}: {}", err.message()))
                })?;
                let inst = match &row.inst {
                    InstRef::Prev => None,
                    InstRef::Name(n) => Some(*inst_ids.get(n.as_str()).ok_or_else(|| {
                        e(
                            Code::E5,
                            format!("パターン {name} の行 {r}: 音色 {n:?} がない"),
                        )
                    })?),
                };
                rows.push(Row {
                    note: row.note,
                    inst,
                    vol: row.vol,
                    fx: row.fx,
                });
            }
            pat_ids.insert(name.as_str(), patterns.len());
            patterns.push((name.clone(), rows));
        }
        let mut order = Vec::with_capacity(m.def.order.len());
        for (o, line) in m.def.order.iter().enumerate() {
            let mut cells = Vec::with_capacity(line.patterns.len());
            let mut rows = None;
            for p in &line.patterns {
                if p == "-" {
                    cells.push(None);
                    continue;
                }
                let id = *pat_ids
                    .get(p.as_str())
                    .ok_or_else(|| e(Code::E5, format!("order {o}: パターン {p:?} がない")))?;
                let n = patterns[id].1.len();
                if rows.is_some_and(|r| r != n) {
                    return Err(e(
                        Code::E6,
                        format!("order {o}: パターンの行数がそろっていない"),
                    ));
                }
                rows = Some(n);
                cells.push(Some(id));
            }
            let Some(rows) = rows else {
                return Err(e(
                    Code::E6,
                    format!("order {o}: すべて \"-\"(行数が決まらない)"),
                ));
            };
            order.push(OrderLine {
                cells,
                transpose: line.transpose,
                rows,
            });
        }
        Ok(Song {
            label: label.to_string(),
            kind,
            timing: m.def.timing,
            master: m.def.master,
            channels: m.def.channels.clone(),
            insts,
            patterns,
            order,
            loop_to: m.def.loop_to.map(usize::from),
            tail: u64::from(samples(u32::from(m.def.tail_ms))),
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct RowPos {
    start: u64,
    order: usize,
    row: usize,
}

/// 行の並び。2 周目からは、戻り先からの行を 1 周目の時刻 + 周の長さ × n に置く。
struct Timeline {
    lap1: Vec<RowPos>,
    loop_k: usize,
    end: u64,
    period: u64,
}

impl Timeline {
    fn new(song: &Song) -> Timeline {
        let mut lap1 = Vec::new();
        let mut loop_k = 0;
        for (o, line) in song.order.iter().enumerate() {
            if Some(o) == song.loop_to {
                loop_k = lap1.len();
            }
            for row in 0..line.rows {
                let start = row_start(song.timing, lap1.len() as u64);
                lap1.push(RowPos {
                    start,
                    order: o,
                    row,
                });
            }
        }
        let end = row_start(song.timing, lap1.len() as u64);
        let period = end - lap1[loop_k].start;
        Timeline {
            lap1,
            loop_k,
            end,
            period,
        }
    }

    fn get(&self, i: usize, looping: bool) -> Option<RowPos> {
        if i < self.lap1.len() {
            return Some(self.lap1[i]);
        }
        if !looping {
            return None;
        }
        let per_lap = self.lap1.len() - self.loop_k;
        let n = i - self.lap1.len();
        let lap = (n / per_lap) as u64 + 1;
        let p = self.lap1[self.loop_k + n % per_lap];
        Some(RowPos {
            start: p.start + self.period * lap,
            ..p
        })
    }

    fn row_at_sample(&self, t: u64, looping: bool) -> RowPos {
        let mut last = self.lap1[0];
        let mut i = 0;
        while let Some(p) = self.get(i, looping) {
            if p.start > t {
                break;
            }
            last = p;
            i += 1;
        }
        last
    }
}

#[derive(Clone, Debug)]
struct Pending {
    at: u64,
    inst: usize,
    tone: Tone,
    fx: Vec<Fx>,
}

#[derive(Clone, Debug)]
struct Chan {
    volume: u8,
    tonal: bool,
    voice: Option<Voice>,
    row_vol: u8,
    prev_inst: Option<usize>,
    transpose: i32,
    lfsr: Lfsr,
    arp: Option<(u8, u8, u64)>,
    bend_base: i32,
    bend_rate: i32,
    bend_origin: u64,
    porta: Option<(i32, i32, u64)>,
    vib: Option<(i32, u32, u64)>,
    duty: Option<Duty>,
    cut_at: Option<u64>,
    retrig: Option<(u64, u64, u64)>,
    pending: Option<Pending>,
}

impl Chan {
    fn new(c: &Channel) -> Chan {
        Chan {
            volume: c.volume,
            tonal: c.tonal,
            voice: None,
            row_vol: 15,
            prev_inst: None,
            transpose: 0,
            lfsr: Lfsr::INIT,
            arp: None,
            bend_base: 0,
            bend_rate: 0,
            bend_origin: 0,
            porta: None,
            vib: None,
            duty: None,
            cut_at: None,
            retrig: None,
            pending: None,
        }
    }

    fn clear_fx(&mut self) {
        self.arp = None;
        self.bend_base = 0;
        self.bend_rate = 0;
        self.porta = None;
        self.vib = None;
        self.duty = None;
        self.cut_at = None;
        self.retrig = None;
    }

    fn bend(&self, t: u64) -> i32 {
        self.bend_base + self.bend_rate * ticks_since(t - self.bend_origin) as i32
    }

    fn porta_offset(&self, t: u64) -> i32 {
        match self.porta {
            None => 0,
            Some((diff, rate, o)) => {
                let moved = i64::from(rate) * ticks_since(t - o) as i64;
                let diff = i64::from(diff);
                let left = if diff > 0 {
                    (diff - moved).max(0)
                } else {
                    (diff + moved).min(0)
                };
                left as i32
            }
        }
    }

    fn fx_cents(&self, t: u64) -> i32 {
        let mut c = self.bend(t) + self.porta_offset(t);
        if let Some((x, y, o)) = self.arp {
            let semis = [0, x, y][(ticks_since(t - o) % 3) as usize];
            c += 100 * i32::from(semis);
        }
        if let Some((depth, inc, o)) = self.vib {
            let at = tick_start(ticks_since(t - o));
            c += lfo_cents(depth, (u64::from(inc) * at) as u32);
        }
        c
    }

    fn apply_fx(&mut self, t: u64, fx: &[Fx]) {
        for f in fx {
            match *f {
                Fx::Arp(x, y) => self.arp = Some((x, y, t)),
                Fx::Up(v) | Fx::Down(v) => {
                    self.bend_base = self.bend(t);
                    self.bend_origin = t;
                    self.bend_rate = if matches!(f, Fx::Up(_)) {
                        i32::from(v)
                    } else {
                        -i32::from(v)
                    };
                }
                Fx::Vibrato(x, y) => {
                    self.vib = (x != 0 || y != 0)
                        .then(|| (i32::from(y) * 10, lfo_inc(u32::from(x) * 500), t));
                }
                Fx::Duty(d) => self.duty = Some(d),
                Fx::Kill(n) => self.cut_at = Some(t + tick_start(u64::from(n))),
                Fx::Retrig(n) => self.retrig = Some((t, u64::from(n), 1)),
                Fx::Glide(_) | Fx::Delay(_) => {}
            }
        }
    }

    fn trigger(&mut self, t: u64, insts: &[Inst], inst: usize, tone: Tone, fx: &[Fx]) {
        let glide = fx.iter().find_map(|f| match f {
            Fx::Glide(r) => Some(i32::from(*r)),
            _ => None,
        });
        let from = match (&self.voice, glide, tone) {
            (Some(v), Some(_), Tone::Pitch(new)) => v
                .base_pitch()
                .map(|p| p + self.bend(t) + self.porta_offset(t) - new),
            _ => None,
        };
        self.clear_fx();
        match (from, glide, self.voice.as_mut()) {
            (Some(diff), Some(rate), Some(v)) => {
                v.legato(inst, tone);
                self.porta = Some((diff, rate, t));
            }
            _ => self.voice = Some(Voice::new(inst, &insts[inst], tone)),
        }
        self.apply_fx(t, fx);
    }

    fn sample(&mut self, t: u64, insts: &[Inst]) -> i32 {
        if self.pending.as_ref().is_some_and(|p| p.at == t)
            && let Some(p) = self.pending.take()
        {
            self.trigger(t, insts, p.inst, p.tone, &p.fx);
        }
        if self.cut_at == Some(t) {
            self.voice = None;
            self.cut_at = None;
        }
        if let Some((o, every, n)) = self.retrig
            && t == o + tick_start(n * every)
        {
            self.retrig = Some((o, every, n + 1));
            if let Some(v) = &self.voice {
                self.voice = Some(Voice::new(v.inst, &insts[v.inst], v.tone));
            }
        }
        let cents = self.fx_cents(t);
        let gain = i64::from(self.row_vol) * i64::from(self.volume);
        let Some(v) = self.voice.as_mut() else {
            return 0;
        };
        let out = v.next(&insts[v.inst], cents, self.duty, &mut self.lfsr, gain);
        if v.is_off() {
            self.voice = None;
        }
        out
    }
}

struct Renderer<'a> {
    song: &'a Song,
    chans: Vec<Chan>,
}

impl Renderer<'_> {
    fn apply_row(&mut self, t: u64, pos: RowPos) -> Result<(), Diagnostic> {
        let song = self.song;
        let line = &song.order[pos.order];
        for (c, ch) in self.chans.iter_mut().enumerate() {
            if pos.row == 0 {
                ch.transpose = if ch.tonal {
                    i32::from(line.transpose) * 100
                } else {
                    0
                };
                if line.cells[c].is_none() {
                    ch.pending = None;
                    if let Some(v) = ch.voice.as_mut() {
                        v.release();
                    }
                }
            }
            let Some(pid) = line.cells[c] else {
                continue;
            };
            let (pname, rows) = &song.patterns[pid];
            let row = &rows[pos.row];
            let err = |msg: &str| {
                Diagnostic::new(
                    Code::E6,
                    &song.label,
                    &format!(
                        "order {} のパターン {pname} の行 {}: {msg}",
                        pos.order, pos.row
                    ),
                )
            };
            if let Some(v) = row.vol {
                ch.row_vol = v;
            }
            let tone = match row.note {
                NoteCmd::Pitch(p) => Tone::Pitch(p + ch.transpose),
                NoteCmd::Noise(n) => Tone::Noise(n),
                NoteCmd::Hold => {
                    ch.apply_fx(t, &row.fx);
                    continue;
                }
                NoteCmd::Release => {
                    if let Some(v) = ch.voice.as_mut() {
                        v.release();
                    }
                    ch.apply_fx(t, &row.fx);
                    continue;
                }
                NoteCmd::Cut => {
                    ch.voice = None;
                    ch.pending = None;
                    ch.apply_fx(t, &row.fx);
                    continue;
                }
            };
            let inst = row
                .inst
                .or(ch.prev_inst)
                .ok_or_else(|| err("`..` の前の音色がない"))?;
            ch.prev_inst = Some(inst);
            let noise_inst = matches!(song.insts[inst].wave, Wave::Noise { .. });
            if noise_inst != matches!(tone, Tone::Noise(_)) {
                return Err(err("ノイズの音色には N-x を、ほかの音色には音名を書く"));
            }
            let delay = row.fx.iter().find_map(|f| match f {
                Fx::Delay(n) if *n > 0 => Some(u64::from(*n)),
                _ => None,
            });
            match delay {
                Some(n) => {
                    ch.pending = Some(Pending {
                        at: t + tick_start(n),
                        inst,
                        tone,
                        fx: row.fx.clone(),
                    });
                }
                None => ch.trigger(t, &song.insts, inst, tone, &row.fx),
            }
        }
        Ok(())
    }
}

impl Song {
    /// 合成する。Bgm は戻り先から 1 秒続けて合成して含め、継ぎ目を検査する(E8)。
    pub(crate) fn render(&self) -> Result<Output, Diagnostic> {
        let tl = Timeline::new(self);
        let looping = self.kind == SoundKind::Bgm;
        let file_len = if looping {
            tl.end + SEAM
        } else {
            tl.end + self.tail
        };
        check_length(&self.label, file_len)?;
        let render_len = if looping { tl.end + 2 * SEAM } else { file_len };
        let loop_from = tl.lap1[tl.loop_k].start;
        let w1 = loop_from + SEAM;
        let w2 = tl.end + SEAM;
        let tonal: Vec<usize> = (0..self.channels.len())
            .filter(|&c| self.channels[c].tonal)
            .collect();
        let mut win1 = vec![Vec::new(); self.channels.len()];
        let mut win2 = vec![Vec::new(); self.channels.len()];

        let mut r = Renderer {
            song: self,
            chans: self.channels.iter().map(Chan::new).collect(),
        };
        let mut out = Vec::with_capacity(file_len as usize);
        let mut clipped = 0u64;
        let mut dc = DcBlock::default();
        let mut next = 0usize;
        let mut next_pos = tl.get(0, looping);
        for t in 0..render_len {
            while let Some(p) = next_pos.filter(|p| p.start == t) {
                r.apply_row(t, p)?;
                next += 1;
                next_pos = tl.get(next, looping);
            }
            let mut mix = 0i32;
            for (c, ch) in r.chans.iter_mut().enumerate() {
                let v = ch.sample(t, &self.insts);
                mix += v;
                if looping && self.channels[c].tonal {
                    if (w1..w1 + SEAM).contains(&t) {
                        win1[c].push(v);
                    }
                    if (w2..w2 + SEAM).contains(&t) {
                        win2[c].push(v);
                    }
                }
            }
            if t < file_len {
                let v = dc.next(i64::from(mix * i32::from(self.master) / 100));
                if !(i64::from(i16::MIN)..=i64::from(i16::MAX)).contains(&v) {
                    clipped += 1;
                }
                out.push(v.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16);
            }
        }
        if looping {
            for &c in &tonal {
                if let Some(i) = (0..win1[c].len()).find(|&i| win1[c][i] != win2[c][i]) {
                    let at = w1 + i as u64;
                    let pos = tl.row_at_sample(at, true);
                    return Err(Diagnostic::new(
                        Code::E8,
                        &self.label,
                        &format!(
                            "ループの継ぎ目が一致しない: チャンネル {}、order {} の行 {}(戻り先から {} ms)",
                            self.channels[c].name,
                            pos.order,
                            pos.row,
                            (at - loop_from) * 1000 / SR
                        ),
                    ));
                }
            }
            return Ok(Output {
                samples: out,
                loop_start: Some((loop_from + SEAM) as u32),
                clipped,
            });
        }
        finish_tail(&mut out);
        Ok(Output {
            samples: out,
            loop_start: None,
            clipped,
        })
    }
}

/// 後ろの 0 を除き、最後の 2ms を直線で 0 まで下げる。
fn finish_tail(s: &mut Vec<i16>) {
    let len = s.iter().rposition(|&v| v != 0).map_or(0, |i| i + 1);
    s.truncate(len);
    let n = len.min(FADE);
    if n == 0 {
        return;
    }
    for (i, v) in s[len - n..].iter_mut().enumerate() {
        let k = if n == 1 { 0 } else { (n - 1 - i) as i32 };
        let d = (n.max(2) - 1) as i32;
        *v = (i32::from(*v) * k / d) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a06_row_start() {
        let t = Timing::Bpm {
            bpm: 132,
            rows_per_beat: 4,
        };
        assert_eq!(row_start(t, 16 * 16), 641_454);
        assert_eq!(row_start(Timing::RowMs(40), 2), 1764);
        assert_eq!(
            row_start(
                Timing::Bpm {
                    bpm: 112,
                    rows_per_beat: 4
                },
                32 * 16
            ),
            1_512_000
        );
    }

    #[test]
    fn ticks() {
        assert_eq!(tick_start(1), 367);
        assert_eq!(tick_start(2), 735);
        for d in 0..5000u64 {
            let j = ticks_since(d);
            assert!(tick_start(j) <= d && tick_start(j + 1) > d, "{d}");
        }
    }

    #[test]
    fn dc_block_removes_offset_and_returns_to_zero() {
        let mut f = DcBlock::default();
        assert_eq!(f.next(1000), 1000);
        let mut y = 0;
        for _ in 0..22_050 {
            y = f.next(1000);
        }
        assert_eq!(y, 0);
        let mut f = DcBlock::default();
        f.next(-1000);
        for _ in 0..22_050 {
            y = f.next(0);
        }
        assert_eq!(y, 0);
    }

    #[test]
    fn tail_fade() {
        let mut s = vec![100i16; 50];
        s.extend([0, 0, 0]);
        finish_tail(&mut s);
        assert_eq!(s.len(), 50);
        assert_eq!(s[5], 100);
        assert_eq!(s[6], 100);
        assert_eq!(s[49], 0);
        assert_eq!(s[7], 100 * 42 / 43);
        let mut z = vec![0i16; 10];
        finish_tail(&mut z);
        assert!(z.is_empty());
        let mut one = vec![5i16];
        finish_tail(&mut one);
        assert_eq!(one, vec![0]);
    }
}
