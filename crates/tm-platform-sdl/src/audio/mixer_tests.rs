//! ミキサーの単体テスト M-01〜M-26 とランダムな命令列のテスト [14 §9.2]。

use super::*;
use crate::audio::gain::VOLUME_Q16;
use tm_platform_api::AudioBus;

const fn ramp<const N: usize>() -> [i16; N] {
    let mut a = [0i16; N];
    let mut i = 0;
    while i < N {
        a[i] = (i + 1) as i16;
        i += 1;
    }
    a
}

static A_DATA: [i16; 3000] = ramp();
static B_DATA: [i16; 4000] = [3000; 4000];
static TITLE_DATA: [i16; 4000] = [1500; 4000];
static J_DATA: [i16; 500] = [2000; 500];
static K_DATA: [i16; 300] = [4000; 300];
static X_DATA: [i16; 1000] = [500; 1000];
static Y_DATA: [i16; 2000] = [700; 2000];
static MAX_DATA: [i16; 1000] = [i16::MAX; 1000];
static MIN_DATA: [i16; 1000] = [i16::MIN; 1000];

const fn snd(
    name: &'static str,
    kind: SoundKind,
    samples: &'static [i16],
    loop_start: Option<u32>,
) -> SoundData {
    SoundData {
        name,
        kind,
        samples,
        loop_start,
    }
}

static SOUNDS: [SoundData; 12] = [
    snd("bgm_a", SoundKind::Bgm, &A_DATA, Some(1000)),
    snd("bgm_b", SoundKind::Bgm, &B_DATA, Some(0)),
    snd("bgm_title", SoundKind::Bgm, &TITLE_DATA, Some(0)),
    snd("jingle_j", SoundKind::Jingle, &J_DATA, None),
    snd("jingle_k", SoundKind::Jingle, &K_DATA, None),
    snd("se_x", SoundKind::Se, &X_DATA, None),
    snd("se_y", SoundKind::Se, &Y_DATA, None),
    snd("bgm_max", SoundKind::Bgm, &MAX_DATA, Some(0)),
    snd("se_max", SoundKind::Se, &MAX_DATA, None),
    snd("bgm_min", SoundKind::Bgm, &MIN_DATA, Some(0)),
    snd("se_min", SoundKind::Se, &MIN_DATA, None),
    snd("jingle_empty", SoundKind::Jingle, &[], None),
];

const A: SoundId = SoundId(0);
const B: SoundId = SoundId(1);
const TITLE: SoundId = SoundId(2);
const J: SoundId = SoundId(3);
const K: SoundId = SoundId(4);
const X: SoundId = SoundId(5);
const Y: SoundId = SoundId(6);
const BGM_MAX: SoundId = SoundId(7);
const SE_MAX: SoundId = SoundId(8);
const BGM_MIN: SoundId = SoundId(9);
const SE_MIN: SoundId = SoundId(10);
const EMPTY_J: SoundId = SoundId(11);
const FADE: u32 = STOP_FADE_SAMPLES;

fn play(id: SoundId, crossfade_ms: u16) -> AudioCmd {
    AudioCmd::PlayBgm { id, crossfade_ms }
}

fn vol(bus: AudioBus, level: u8) -> AudioCmd {
    AudioCmd::SetVolume { bus, level }
}

/// 既定の音量のミキサー。
fn mixer_default() -> Mixer {
    let mut m = Mixer::new();
    m.set_sounds(&SOUNDS);
    m
}

/// 両方のバスを音量 10 にそろえたミキサー(出力 = 声の和)。
fn mixer() -> Mixer {
    let mut m = mixer_default();
    run(
        &mut m,
        &[vol(AudioBus::Bgm, 10), vol(AudioBus::Se, 10)],
        LEVEL_RAMP_SAMPLES as usize,
    );
    m
}

fn run(m: &mut Mixer, cmds: &[AudioCmd], n: usize) -> Vec<i16> {
    for c in cmds {
        assert!(m.enqueue(*c));
    }
    let mut out = vec![1234; n];
    m.render(&mut out);
    out
}

fn a_at(pos: u32) -> i32 {
    i32::from(A_DATA[pos as usize])
}

/// 頭から `n` サンプル進めたときの A の位置(ループを含む)。
fn a_pos(n: u32) -> u32 {
    if n < 3000 {
        n
    } else {
        1000 + (n - 1000) % 2000
    }
}

fn fade_gain(k: u32, n: u32) -> i32 {
    (65536 - 65536 * i64::from(k) / i64::from(n)) as i32
}

fn scaled(v: i32, gain: i32) -> i32 {
    ((i64::from(v) * i64::from(gain)) >> 16) as i32
}

fn cur(m: &Mixer) -> VoiceSnap {
    m.snapshot().bgm_cur.expect("bgm_cur")
}

#[test]
fn m01_play_bgm_starts_from_head_at_unity() {
    let mut m = mixer();
    let out = run(&mut m, &[play(A, 0)], 10);
    assert_eq!(out, (1..=10).collect::<Vec<i16>>());
    let s = cur(&m);
    assert_eq!((s.id, s.pos, s.gain, s.remaining), (A, 10, UNITY, 0));
}

#[test]
fn m02_same_bgm_twice_does_nothing() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    let out = run(&mut m, &[play(A, 0), play(A, 500)], 2);
    assert_eq!(out, [101, 102]);
    assert_eq!(m.snapshot().bgm_out, None);
}

#[test]
fn m03_loop_returns_to_loop_start() {
    let mut m = mixer();
    let out = run(&mut m, &[play(A, 0)], 3000);
    assert_eq!(out[2999], 3000);
    let out = run(&mut m, &[], 2);
    assert_eq!(out, [1001, 1002]);
    let out = run(&mut m, &[], 4000);
    assert_eq!(out[1997], 3000);
    assert_eq!(out[1998], 1001);
}

#[test]
fn m04_crossfade_is_linear_and_clears_bgm_out() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    assert!(m.enqueue(play(B, 1000)));
    let n = 22050;
    for k in 1..=n {
        let out = run(&mut m, &[], 1);
        let s = m.snapshot();
        let a_gain = fade_gain(k, n);
        let b_gain = 65536 - a_gain;
        assert_eq!(s.bgm_cur.unwrap().gain, b_gain as u32, "{k}");
        if k < n {
            let o = s.bgm_out.unwrap();
            assert_eq!((o.id, o.gain), (A, a_gain as u32), "{k}");
        } else {
            assert_eq!(s.bgm_out, None);
        }
        let want = scaled(a_at(a_pos(100 + k - 1)), a_gain) + scaled(3000, b_gain);
        assert_eq!(i32::from(out[0]), want, "{k}");
    }
    let s = cur(&m);
    assert_eq!((s.id, s.gain, s.remaining), (B, UNITY, 0));
}

#[test]
fn m05_jingle_holds_bgm_and_fades_it() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    let out = run(&mut m, &[AudioCmd::PlayJingle(J)], FADE as usize);
    let s = m.snapshot();
    assert_eq!(s.bgm_held, Some((A, 100)));
    assert_eq!(s.bgm_cur, None);
    assert_eq!(s.bgm_out, None);
    let j = s.jingle.unwrap();
    assert_eq!((j.id, j.pos, j.gain), (J, FADE, UNITY));
    for (k, o) in out.iter().enumerate() {
        let k = k as u32;
        let want = scaled(a_at(100 + k), fade_gain(k + 1, FADE)) + 2000;
        assert_eq!(i32::from(*o), want, "{k}");
    }
    assert_eq!(out[FADE as usize - 1], 2000);
}

fn assert_resumed_a_at(m: &Mixer, pos: u32) {
    let s = m.snapshot();
    assert_eq!(s.jingle, None);
    assert_eq!(s.bgm_held, None);
    assert_eq!(s.bgm_next, None);
    let c = s.bgm_cur.unwrap();
    assert_eq!(
        (c.id, c.pos, c.gain, c.target, c.remaining),
        (A, pos, 0, UNITY, FADE)
    );
}

#[test]
fn m06_jingle_end_resumes_held_bgm() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    let out = run(&mut m, &[AudioCmd::PlayJingle(J)], 500);
    assert_eq!(out[499], 2000);
    assert_resumed_a_at(&m, 100);
    let out = run(&mut m, &[], FADE as usize);
    for (k, o) in out.iter().enumerate() {
        let k = k as u32;
        assert_eq!(
            i32::from(*o),
            scaled(a_at(100 + k), 65536 - fade_gain(k + 1, FADE)),
            "{k}"
        );
    }
    assert_eq!(cur(&m).gain, UNITY);
}

#[test]
fn m07_same_bgm_during_jingle_resumes_held_position() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    run(&mut m, &[AudioCmd::PlayJingle(J)], 200);
    run(&mut m, &[play(A, 0)], 300);
    assert_resumed_a_at(&m, 100);
}

#[test]
fn m08_other_bgm_during_jingle_starts_from_head() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    run(&mut m, &[AudioCmd::PlayJingle(J)], 200);
    assert_eq!(m.snapshot().bgm_next, None);
    run(&mut m, &[play(B, 1000)], 300);
    let c = cur(&m);
    assert_eq!((c.id, c.pos, c.gain, c.remaining), (B, 0, UNITY, 0));
    assert_eq!(m.snapshot().bgm_held, None);
    assert_eq!(run(&mut m, &[], 1), [3000]);
}

#[test]
fn m09_sixteen_jingles_play_back_to_back() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    let mut want = Vec::new();
    let mut cmds = Vec::new();
    for i in 0..16 {
        let (id, data): (SoundId, &[i16]) = if i % 2 == 0 {
            (J, &J_DATA)
        } else {
            (K, &K_DATA)
        };
        cmds.push(AudioCmd::PlayJingle(id));
        want.extend_from_slice(data);
    }
    let out = run(&mut m, &cmds, want.len());
    assert_eq!(m.snapshot().jingle_q, 0);
    assert_eq!(&out[FADE as usize..], &want[FADE as usize..]);
    assert_resumed_a_at(&m, 100);
}

#[test]
fn m10_jingle_queue_overflow_drops_last() {
    let mut m = mixer();
    let cmds = [AudioCmd::PlayJingle(J); 34];
    run(&mut m, &cmds, 1);
    assert_eq!(m.snapshot().jingle_q, JINGLE_QUEUE_LEN);
    assert_eq!(m.take_report().dropped_jingles, 1);
    run(&mut m, &[], 33 * 500);
    let s = m.snapshot();
    assert_eq!((s.jingle, s.jingle_q, s.bgm_cur), (None, 0, None));
}

#[test]
fn m11_jingle_during_crossfade_drops_outgoing_and_holds_incoming() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    run(&mut m, &[play(B, 1000)], 5000);
    let before = m.snapshot();
    assert_eq!(before.bgm_out.unwrap().id, A);
    let b = before.bgm_cur.unwrap();
    run(&mut m, &[AudioCmd::PlayJingle(J)], 0);
    let s = m.snapshot();
    assert_eq!(s.bgm_held, Some((B, b.pos)));
    let o = s.bgm_out.unwrap();
    assert_eq!((o.id, o.gain, o.target, o.remaining), (B, b.gain, 0, FADE));
    assert_eq!(s.jingle.unwrap().id, J);
    run(&mut m, &[], 500);
    let c = cur(&m);
    assert_eq!((c.id, c.pos, c.target), (B, b.pos, UNITY));
}

#[test]
fn m12_stop_bgm_during_jingle_clears_everything() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    run(
        &mut m,
        &[AudioCmd::PlayJingle(J), AudioCmd::PlayJingle(K), play(B, 0)],
        50,
    );
    let s = m.snapshot();
    assert_eq!(
        (s.jingle_q, s.bgm_held, s.bgm_next),
        (1, Some((A, 100)), Some(B))
    );
    let out = run(&mut m, &[AudioCmd::StopBgm { fade_ms: 2000 }], 2000);
    let s = m.snapshot();
    assert_eq!(
        (
            s.jingle, s.jingle_q, s.bgm_held, s.bgm_next, s.bgm_cur, s.bgm_out
        ),
        (None, 0, None, None, None, None)
    );
    assert_eq!(i32::from(out[0]), scaled(2000, fade_gain(1, FADE)));
    assert!(out[FADE as usize - 1..].iter().all(|s| *s == 0));
}

#[test]
fn m13_stop_then_play_title_in_same_render() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    let out = run(
        &mut m,
        &[AudioCmd::StopBgm { fade_ms: 0 }, play(TITLE, 0)],
        FADE as usize + 1,
    );
    for (k, o) in out.iter().enumerate() {
        let k = k as u32;
        let old = if k < FADE {
            scaled(a_at(100 + k), fade_gain(k + 1, FADE))
        } else {
            0
        };
        assert_eq!(i32::from(*o), old + 1500, "{k}");
    }
    let s = m.snapshot();
    assert_eq!(s.bgm_out, None);
    assert_eq!(s.bgm_cur.unwrap().id, TITLE);
}

#[test]
fn m14_duck_halves_bgm_bus_including_jingle() {
    let mut m = mixer_default();
    run(&mut m, &[AudioCmd::PlayJingle(J)], 10);
    run(&mut m, &[AudioCmd::SetBgmDuck(true)], 219);
    let s = m.snapshot();
    assert_ne!(s.bgm_bus, VOLUME_Q16[6] >> 1);
    let out = run(&mut m, &[], 1);
    assert_eq!(m.snapshot().bgm_bus, VOLUME_Q16[6] >> 1);
    assert_eq!(m.snapshot().se_bus, VOLUME_Q16[8]);
    assert_eq!(i32::from(out[0]), scaled(2000, (VOLUME_Q16[6] >> 1) as i32));
    run(&mut m, &[AudioCmd::SetBgmDuck(false)], 220);
    assert_eq!(m.snapshot().bgm_bus, VOLUME_Q16[6]);
}

#[test]
fn m15_volume_levels_follow_table_and_clamp() {
    let mut m = mixer_default();
    let s = m.snapshot();
    assert_eq!((s.bgm_bus, s.se_bus), (VOLUME_Q16[6], VOLUME_Q16[8]));
    for level in 0..=11u8 {
        for bus in [AudioBus::Bgm, AudioBus::Se] {
            run(&mut m, &[vol(bus, level)], 220);
            let s = m.snapshot();
            let got = if bus == AudioBus::Bgm {
                s.bgm_bus
            } else {
                s.se_bus
            };
            assert_eq!(
                got,
                VOLUME_Q16[usize::from(level.min(10))],
                "{bus:?} {level}"
            );
        }
    }
    assert_eq!(m.take_report(), MixerReport::default());
}

fn se_slots(m: &Mixer) -> Vec<(SoundId, u64, bool)> {
    m.snapshot()
        .se
        .iter()
        .flatten()
        .map(|(v, seq, stopping)| (v.id, *seq, *stopping))
        .collect()
}

#[test]
fn m16_stop_se_stops_oldest_of_that_id() {
    let mut m = mixer();
    run(&mut m, &[AudioCmd::PlaySe(X)], 10);
    run(&mut m, &[AudioCmd::PlaySe(Y), AudioCmd::PlaySe(X)], 10);
    run(&mut m, &[AudioCmd::StopSe(X)], 1);
    assert_eq!(se_slots(&m), [(X, 0, true), (Y, 1, false), (X, 2, false)]);
    run(&mut m, &[AudioCmd::StopSe(X)], FADE as usize - 1);
    assert_eq!(se_slots(&m), [(Y, 1, false), (X, 2, true)]);
    let out = run(&mut m, &[AudioCmd::StopSe(X)], 1);
    assert_eq!(se_slots(&m), [(Y, 1, false)]);
    assert_eq!(out, [700]);
}

#[test]
fn m17_ninth_se_steals_oldest() {
    let mut m = mixer();
    run(&mut m, &[AudioCmd::PlaySe(X); 8], 1);
    run(&mut m, &[AudioCmd::PlaySe(Y)], 1);
    let slots = se_slots(&m);
    assert_eq!(slots.len(), SE_SLOTS);
    assert_eq!(slots[0], (Y, 8, false));
    assert!(slots[1..].iter().all(|s| s.0 == X && s.1 >= 1));
    assert_eq!(m.take_report().stolen_se, 1);
}

#[test]
fn m18_command_queue_overflow_drops_129th() {
    let mut m = mixer();
    for i in 0..127u8 {
        assert!(m.enqueue(vol(AudioBus::Se, i % 11)));
    }
    assert!(m.enqueue(AudioCmd::PlaySe(X)));
    assert!(!m.enqueue(AudioCmd::PlaySe(Y)));
    assert_eq!(m.snapshot().cmds, CMD_QUEUE_LEN);
    run(&mut m, &[], 220);
    let s = m.snapshot();
    assert_eq!(s.se_bus_target, VOLUME_Q16[126 % 11]);
    assert_eq!(se_slots(&m), [(X, 0, false)]);
    assert_eq!(s.cmds, 0);
    assert_eq!(m.take_report().dropped_cmds, 1);
    assert!(m.enqueue(AudioCmd::PlaySe(Y)));
}

#[test]
fn m19_wrong_kind_or_unknown_id_is_ignored() {
    let mut m = mixer();
    let bad = [
        AudioCmd::PlaySe(A),
        AudioCmd::PlaySe(J),
        AudioCmd::PlaySe(SoundId(99)),
        AudioCmd::StopSe(A),
        AudioCmd::StopSe(SoundId(12)),
        play(X, 0),
        play(J, 100),
        play(SoundId(u16::MAX), 0),
        AudioCmd::PlayJingle(A),
        AudioCmd::PlayJingle(X),
        AudioCmd::PlayJingle(SoundId(12)),
    ];
    let out = run(&mut m, &bad, 256);
    assert!(out.iter().all(|s| *s == 0));
    let s = m.snapshot();
    assert_eq!(
        (s.bgm_cur, s.bgm_out, s.jingle, s.se),
        (None, None, None, [None; SE_SLOTS])
    );
    let r = m.take_report();
    assert_eq!(r.invalid_cmds, bad.len() as u32);
    assert_eq!(m.take_report().invalid_cmds, 0);
}

#[test]
fn m20_mix_is_clipped_to_i16() {
    let mut m = mixer();
    let out = run(
        &mut m,
        &[
            play(BGM_MAX, 0),
            AudioCmd::PlaySe(SE_MAX),
            AudioCmd::PlaySe(SE_MAX),
        ],
        1000,
    );
    assert!(out.iter().all(|s| *s == i16::MAX));
    let mut m = mixer();
    let out = run(
        &mut m,
        &[
            play(BGM_MIN, 0),
            AudioCmd::PlaySe(SE_MIN),
            AudioCmd::PlaySe(SE_MIN),
        ],
        1000,
    );
    assert!(out.iter().all(|s| *s == i16::MIN));
}

#[test]
fn m21_empty_table_is_silent() {
    let mut m = Mixer::new();
    let cmds = [
        play(A, 0),
        AudioCmd::PlayJingle(J),
        AudioCmd::PlaySe(X),
        AudioCmd::StopSe(X),
        AudioCmd::StopBgm { fade_ms: 100 },
        AudioCmd::SetBgmDuck(true),
        vol(AudioBus::Se, 10),
    ];
    let out = run(&mut m, &cmds, 4096);
    assert!(out.iter().all(|s| *s == 0));
    assert_eq!(m.take_report().invalid_cmds, 4);
}

/// 命令を出すサンプルの位置と命令。
fn scenario() -> Vec<(usize, Vec<AudioCmd>)> {
    vec![
        (
            0,
            vec![play(A, 0), AudioCmd::PlaySe(X), vol(AudioBus::Bgm, 7)],
        ),
        (
            3000,
            vec![
                play(B, 200),
                AudioCmd::PlaySe(Y),
                AudioCmd::SetBgmDuck(true),
            ],
        ),
        (
            9000,
            vec![
                AudioCmd::PlayJingle(J),
                AudioCmd::PlayJingle(K),
                AudioCmd::PlaySe(X),
            ],
        ),
        (9400, vec![play(TITLE, 0), AudioCmd::StopSe(X)]),
        (
            15000,
            vec![
                AudioCmd::StopBgm { fade_ms: 300 },
                AudioCmd::SetBgmDuck(false),
            ],
        ),
        (16000, vec![play(A, 50), vol(AudioBus::Se, 3)]),
    ]
}

/// `scenario` を、命令の位置をまたがない最大 `chunk` サンプルのバッファで合成する。
fn render_scenario(chunk: usize, total: usize) -> Vec<i16> {
    let mut m = mixer_default();
    let events = scenario();
    let mut out = vec![0i16; total];
    let mut pos = 0;
    let mut next = 0;
    while pos < total {
        while next < events.len() && events[next].0 == pos {
            for c in &events[next].1 {
                assert!(m.enqueue(*c));
            }
            next += 1;
        }
        let limit = events.get(next).map_or(total, |e| e.0);
        let end = (pos + chunk).min(limit);
        m.render(&mut out[pos..end]);
        pos = end;
    }
    out
}

#[test]
fn m22_buffer_size_does_not_change_output() {
    let total = 40_000;
    let one = render_scenario(1, total);
    assert!(one.iter().filter(|s| **s != 0).count() > total / 2);
    for chunk in [1023, 1024, 4096] {
        assert!(render_scenario(chunk, total) == one, "{chunk}");
    }
}

#[test]
fn m23_reset_path_clears_jingles_and_starts_title() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    run(
        &mut m,
        &[AudioCmd::PlayJingle(J), AudioCmd::PlayJingle(K)],
        200,
    );
    let out = run(
        &mut m,
        &[AudioCmd::StopBgm { fade_ms: 0 }, play(TITLE, 0)],
        1,
    );
    let s = m.snapshot();
    assert_eq!(
        (s.jingle, s.jingle_q, s.bgm_held, s.bgm_next),
        (None, 0, None, None)
    );
    let o = s.bgm_out.unwrap();
    assert_eq!((o.id, o.target, o.remaining), (J, 0, FADE - 1));
    let c = s.bgm_cur.unwrap();
    assert_eq!((c.id, c.pos, c.gain), (TITLE, 1, UNITY));
    assert_eq!(i32::from(out[0]), scaled(2000, fade_gain(1, FADE)) + 1500);
    let out = run(&mut m, &[], 2000);
    assert!(out[FADE as usize - 1..].iter().all(|s| *s == 1500));
    let s = m.snapshot();
    assert_eq!(
        (s.jingle, s.bgm_out, s.bgm_cur.unwrap().id),
        (None, None, TITLE)
    );
}

#[test]
fn m24_play_bgm_while_stopped_jingle_fades_starts_now() {
    let mut m = mixer();
    run(&mut m, &[AudioCmd::PlayJingle(J)], 100);
    run(&mut m, &[AudioCmd::StopBgm { fade_ms: 0 }], 50);
    assert_eq!(m.snapshot().bgm_out.unwrap().id, J);
    let out = run(&mut m, &[play(B, 0)], 1);
    let s = m.snapshot();
    assert_eq!(s.bgm_next, None);
    let c = s.bgm_cur.unwrap();
    assert_eq!((c.id, c.pos, c.gain), (B, 1, UNITY));
    assert_eq!(s.bgm_out.unwrap().id, J);
    assert_eq!(i32::from(out[0]), scaled(2000, fade_gain(51, FADE)) + 3000);
}

#[test]
fn m25_all_stopping_slots_reuse_oldest_stopping() {
    let mut m = mixer();
    run(&mut m, &[AudioCmd::PlaySe(X); 8], 1);
    run(&mut m, &[AudioCmd::StopSe(X); 8], 1);
    assert!(se_slots(&m).iter().all(|s| s.2));
    run(&mut m, &[AudioCmd::PlaySe(Y)], 1);
    let slots = se_slots(&m);
    assert_eq!(slots[0], (Y, 8, false));
    assert!(slots[1..].iter().all(|s| s.0 == X && s.2));
    assert_eq!(m.take_report().stolen_se, 0);
}

#[test]
fn m26_stopping_slot_is_reused_before_playing_one() {
    let mut m = mixer();
    run(&mut m, &[AudioCmd::PlaySe(X); 7], 1);
    run(&mut m, &[AudioCmd::PlaySe(Y)], 1);
    run(&mut m, &[AudioCmd::StopSe(X); 7], 1);
    run(&mut m, &[AudioCmd::PlaySe(X)], 1);
    let slots = se_slots(&m);
    assert_eq!(slots.len(), SE_SLOTS);
    assert_eq!(slots[0], (X, 8, false));
    assert!(slots.contains(&(Y, 7, false)));
    assert_eq!(slots.iter().filter(|s| s.2).count(), 6);
    assert_eq!(m.take_report().stolen_se, 0);
}

#[test]
fn se_ends_after_its_samples_and_frees_slot() {
    let mut m = mixer();
    let out = run(&mut m, &[AudioCmd::PlaySe(X)], 1001);
    assert!(out[..1000].iter().all(|s| *s == 500));
    assert_eq!(out[1000], 0);
    assert!(se_slots(&m).is_empty());
}

#[test]
fn stop_bgm_uses_at_least_five_ms_and_given_fade() {
    for (fade_ms, n) in [(0u16, FADE), (3, FADE), (100, 2205)] {
        let mut m = mixer();
        run(&mut m, &[play(B, 0)], 10);
        run(&mut m, &[AudioCmd::StopBgm { fade_ms }], n as usize - 1);
        assert_eq!(m.snapshot().bgm_out.unwrap().remaining, 1, "{fade_ms}");
        assert_eq!(run(&mut m, &[], 1), [0]);
        assert_eq!(m.snapshot().bgm_out, None, "{fade_ms}");
    }
}

#[test]
fn m27_se_volume_is_not_ducked() {
    let mut m = mixer_default();
    run(
        &mut m,
        &[AudioCmd::SetBgmDuck(true), vol(AudioBus::Se, 9)],
        220,
    );
    let s = m.snapshot();
    assert_eq!((s.bgm_bus, s.se_bus), (VOLUME_Q16[6] >> 1, VOLUME_Q16[9]));
    run(&mut m, &[vol(AudioBus::Bgm, 4)], 220);
    assert_eq!(m.snapshot().bgm_bus, VOLUME_Q16[4] >> 1);
}

#[test]
fn m28_bus_gain_is_applied_before_clipping() {
    let mut m = mixer_default();
    let out = run(
        &mut m,
        &[
            vol(AudioBus::Se, 5),
            AudioCmd::PlaySe(SE_MAX),
            AudioCmd::PlaySe(SE_MAX),
        ],
        300,
    );
    assert_eq!(i32::from(out[299]), scaled(2 * 32767, VOLUME_Q16[5] as i32));
    assert_eq!(out[299], 16383);
}

#[test]
fn m29_commands_without_current_bgm_leave_fading_bgm_alone() {
    let cases: [&[AudioCmd]; 5] = [
        &[AudioCmd::StopBgm { fade_ms: 0 }],
        &[play(TITLE, 0)],
        &[play(TITLE, 500)],
        &[play(B, 0)],
        &[AudioCmd::PlayJingle(J)],
    ];
    let n = ms_to_samples(100);
    for cmds in cases {
        let mut m = mixer();
        run(&mut m, &[play(B, 0)], 10);
        run(&mut m, &[AudioCmd::StopBgm { fade_ms: 100 }], 100);
        let before = m.snapshot().bgm_out.unwrap();
        run(&mut m, cmds, 0);
        let s = m.snapshot();
        assert_eq!(s.bgm_out, Some(before), "{cmds:?}");
        assert_eq!((s.bgm_held, s.bgm_next), (None, None), "{cmds:?}");
        let started = match cmds[0] {
            AudioCmd::PlayBgm { id, .. } => s.bgm_cur.map(|c| (c.id, c.pos)) == Some((id, 0)),
            AudioCmd::PlayJingle(id) => s.jingle.map(|j| (j.id, j.pos)) == Some((id, 0)),
            _ => s.bgm_cur.is_none() && s.jingle.is_none(),
        };
        assert!(started, "{cmds:?} {s:?}");
        for k in 101..n {
            let out = run(&mut m, &[], 1);
            let o = m.snapshot().bgm_out.unwrap();
            let g = fade_gain(k, n);
            assert_eq!(
                (o.id, o.gain, o.remaining),
                (B, g as u32, n - k),
                "{cmds:?} {k}"
            );
            if cmds.len() == 1 && matches!(cmds[0], AudioCmd::StopBgm { .. }) {
                assert_eq!(i32::from(out[0]), scaled(3000, g), "{k}");
            }
        }
        run(&mut m, &[], 1);
        let s = m.snapshot();
        assert_eq!(s.bgm_out, None, "{cmds:?}");
        assert_eq!((s.jingle, s.bgm_held), (None, None), "{cmds:?}");
    }
}

#[test]
fn m30_crossfade_from_silence_fades_in() {
    let mut m = mixer();
    let n = 22050u32;
    assert!(m.enqueue(play(B, 1000)));
    for k in 1..=n {
        let out = run(&mut m, &[], 1);
        let c = cur(&m);
        let g = 65536 - fade_gain(k, n);
        assert_eq!((c.id, c.gain, c.remaining), (B, g as u32, n - k), "{k}");
        assert_eq!(i32::from(out[0]), scaled(3000, g), "{k}");
        assert_eq!(m.snapshot().bgm_out, None);
    }
    assert_eq!(cur(&m).gain, UNITY);
}

#[test]
fn stop_bgm_without_current_keeps_fading_bgm() {
    let mut m = mixer();
    run(&mut m, &[play(B, 0)], 10);
    run(&mut m, &[AudioCmd::StopBgm { fade_ms: 1000 }], 100);
    let before = m.snapshot().bgm_out.unwrap();
    run(
        &mut m,
        &[AudioCmd::StopBgm { fade_ms: 0 }, play(TITLE, 0)],
        1,
    );
    let s = m.snapshot();
    let o = s.bgm_out.unwrap();
    assert_eq!((o.id, o.remaining), (B, before.remaining - 1));
    assert_eq!(s.bgm_cur.unwrap().id, TITLE);
}

#[test]
fn empty_jingle_ends_at_once() {
    let mut m = mixer();
    run(&mut m, &[play(A, 0)], 100);
    run(&mut m, &[AudioCmd::PlayJingle(EMPTY_J)], 1);
    assert_resumed_a_at(&m, 100);
    let mut m = mixer();
    let out = run(
        &mut m,
        &[AudioCmd::PlayJingle(EMPTY_J), AudioCmd::PlayJingle(J)],
        3,
    );
    assert_eq!(out, [0, 2000, 2000]);
    assert_eq!(m.snapshot().jingle.unwrap().id, J);
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn random_cmd(r: &mut XorShift) -> AudioCmd {
    let id = SoundId(r.below(14) as u16);
    let ms = if r.below(4) == 0 {
        0
    } else {
        r.below(1500) as u16
    };
    match r.below(10) {
        0 | 1 => AudioCmd::PlaySe(id),
        2 => AudioCmd::StopSe(id),
        3 => play(id, ms),
        4 => AudioCmd::StopBgm { fade_ms: ms },
        5 | 6 => AudioCmd::PlayJingle(id),
        7 => AudioCmd::SetBgmDuck(r.below(2) == 0),
        _ => vol(
            if r.below(2) == 0 {
                AudioBus::Bgm
            } else {
                AudioBus::Se
            },
            r.below(13) as u8,
        ),
    }
}

fn check_invariants(s: &MixerSnapshot) -> Result<(), String> {
    let voices = [s.bgm_cur, s.bgm_out, s.jingle]
        .into_iter()
        .flatten()
        .chain(s.se.iter().flatten().map(|v| v.0));
    for v in voices {
        let len = SOUNDS[usize::from(v.id.0)].samples.len() as u32;
        if (len > 0 && v.pos >= len) || v.gain > UNITY || v.target > UNITY {
            return Err(format!("声の状態: {v:?}"));
        }
    }
    if s.jingle.is_none() && (s.bgm_held.is_some() || s.bgm_next.is_some() || s.jingle_q > 0) {
        return Err("ジングルなしで held・next・待ち行列がある".into());
    }
    if s.jingle.is_some() && s.bgm_cur.is_some() {
        return Err("ジングルと bgm_cur が同時に鳴っている".into());
    }
    let mut seqs: Vec<u64> = s.se.iter().flatten().map(|v| v.1).collect();
    seqs.sort_unstable();
    seqs.dedup();
    if seqs.len() != s.se.iter().flatten().count() {
        return Err("効果音の通し番号が重なっている".into());
    }
    if s.se.iter().flatten().any(|v| v.2 && v.0.target != 0) {
        return Err("止めている効果音のゲインの目標が 0 でない".into());
    }
    if s.bgm_bus > UNITY || s.se_bus > UNITY {
        return Err("バスのゲインが 1 を超える".into());
    }
    Ok(())
}

/// ランダムな命令と長さの render を繰り返しても、パニックせず、不変条件を守り、1 サンプルずつ合成した結果と一致する。
#[test]
fn property_random_commands_keep_invariants() {
    for seed in 1..=48u64 {
        let mut r = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut m = mixer_default();
        let mut one = mixer_default();
        let mut buf = vec![0i16; 6000];
        let mut single = [0i16; 1];
        for step in 0..300 {
            if r.below(5) < 3 {
                let burst = r.below(20) == 0;
                let count = if burst { 40 } else { 1 + r.below(4) };
                let mut c = random_cmd(&mut r);
                for _ in 0..count {
                    assert_eq!(m.enqueue(c), one.enqueue(c), "seed {seed} step {step}");
                    if !burst {
                        c = random_cmd(&mut r);
                    }
                }
                continue;
            }
            let n = match r.below(8) {
                0 => r.below(6000),
                1 => 0,
                _ => r.below(400),
            } as usize;
            m.render(&mut buf[..n]);
            one.render(&mut []);
            for (k, s) in buf[..n].iter().enumerate() {
                one.render(&mut single);
                assert_eq!(*s, single[0], "seed {seed} step {step} sample {k}");
            }
            let snap = m.snapshot();
            assert_eq!(snap, one.snapshot(), "seed {seed} step {step}");
            if let Err(e) = check_invariants(&snap) {
                panic!("seed {seed} step {step}: {e} {snap:?}");
            }
        }
        assert_eq!(m.take_report(), one.take_report());
    }
}
