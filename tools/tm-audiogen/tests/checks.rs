//! A-07(ループ)、A-09(E1〜E10)と、効果の時刻の検査。

use std::fs;
use std::path::{Path, PathBuf};

use tm_audiogen::{Code, Diagnostic, Sound, build_dir, render_file};

fn case_dir(case: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tm-audiogen-checks")
        .join(case);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn render(case: &str, file: &str, ron: &str) -> (Option<Sound>, Vec<Diagnostic>) {
    let dir = case_dir(case);
    let path = dir.join(file);
    fs::write(&path, ron).unwrap();
    render_file(&path).unwrap()
}

fn codes(d: &[Diagnostic]) -> Vec<Code> {
    d.iter().map(|x| x.code).collect()
}

fn expect_error(case: &str, file: &str, ron: &str, code: Code) -> Diagnostic {
    let (sound, d) = render(case, file, ron);
    let found = d.iter().find(|x| x.code == code);
    assert!(found.is_some(), "{case}: {code:?} が出ない: {d:?}");
    if code != Code::E9 && code != Code::E10 {
        assert!(sound.is_none(), "{case}");
    }
    found.unwrap().clone()
}

const INST: &str = r#""sq": (wave: Pulse(duty: D50), env: (attack_ms: 0, decay_ms: 0, sustain: 100, release_ms: 0), volume: 100)"#;

struct Def<'a> {
    timing: &'a str,
    master: &'a str,
    imports: &'a str,
    instruments: &'a str,
    channels: &'a str,
    patterns: &'a str,
    order: &'a str,
    extra: &'a str,
}

impl Default for Def<'_> {
    fn default() -> Self {
        Def {
            timing: "RowMs(50)",
            master: "100",
            imports: "",
            instruments: INST,
            channels: r#"(name: "p1", volume: 15, tonal: true)"#,
            patterns: r#""m": (rows: ["C-5 sq v15", "=== .. ..."])"#,
            order: r#"(patterns: ["m"], transpose: 0)"#,
            extra: "",
        }
    }
}

impl Def<'_> {
    fn ron(&self) -> String {
        format!(
            "SoundDef(\n format: 1,\n timing: {},\n master: {},\n imports: [{}],\n instruments: {{ {} }},\n channels: [{}],\n patterns: {{ {} }},\n order: [{}],\n {}\n)\n",
            self.timing,
            self.master,
            self.imports,
            self.instruments,
            self.channels,
            self.patterns,
            self.order,
            self.extra
        )
    }
}

fn ok(case: &str, file: &str, ron: &str) -> Sound {
    let (sound, d) = render(case, file, ron);
    assert!(d.iter().all(|x| !x.code.is_error()), "{case}: {d:?}");
    sound.unwrap()
}

/// 音が止まった位置(最後の大きな段差)。その後ろは直流除去の減衰だけ(14 §5.4)。
fn stop_at(s: &[i16]) -> usize {
    (1..s.len())
        .rev()
        .find(|&i| (i32::from(s[i]) - i32::from(s[i - 1])).abs() > 2000)
        .unwrap()
}

#[test]
fn minimal_se_renders() {
    let s = ok("minimal", "se_min.ron", &Def::default().ron());
    assert_eq!(stop_at(&s.samples), 1102);
    assert!(s.samples.len() < 1102 + 400);
    assert_eq!(s.loop_start, None);
    assert_eq!(s.clipped, 0);
    assert_eq!(*s.samples.last().unwrap(), 0);
    assert_eq!(s.peak, 8599);
}

#[test]
fn a09_e1_name() {
    let ron = Def::default().ron();
    for bad in ["sfx_a.ron", "se_.ron", "se_A.ron", "se-a.ron"] {
        let (s, d) = render("e1", bad, &ron);
        assert!(s.is_none());
        assert_eq!(codes(&d), vec![Code::E1], "{bad}");
    }
    let dir = case_dir("e1_dir");
    fs::write(dir.join("Se_x.ron"), &ron).unwrap();
    fs::write(dir.join("se_ok.ron"), &ron).unwrap();
    fs::write(dir.join("README.md"), "x").unwrap();
    fs::create_dir_all(dir.join("_lib")).unwrap();
    fs::write(dir.join("_lib/x.ron"), "garbage").unwrap();
    let b = build_dir(&dir).unwrap();
    assert!(b.has_errors());
    assert_eq!(b.sounds.len(), 1);
    let e: Vec<_> = b.diagnostics.iter().filter(|d| d.code.is_error()).collect();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].code, Code::E1);
    assert_eq!(e[0].file, "Se_x.ron");
}

#[test]
fn a09_e2_loop() {
    let se_loop = Def {
        extra: "loop_to: Some(0),",
        ..Def::default()
    };
    expect_error("e2a", "se_a.ron", &se_loop.ron(), Code::E2);
    expect_error("e2b", "jingle_a.ron", &se_loop.ron(), Code::E2);
    expect_error("e2c", "bgm_a.ron", &Def::default().ron(), Code::E2);
}

#[test]
fn a09_e4_format_and_syntax() {
    let ron = Def::default().ron();
    expect_error(
        "e4a",
        "se_a.ron",
        &ron.replace("format: 1", "format: 2"),
        Code::E4,
    );
    expect_error(
        "e4b",
        "se_a.ron",
        &ron.replace("master: 100,", "master: 100"),
        Code::E4,
    );
    expect_error(
        "e4c",
        "se_a.ron",
        &ron.replace("master: 100,", "master: 100, foo: 1,"),
        Code::E4,
    );
    let d = expect_error(
        "e4d",
        "se_a.ron",
        &ron.replace("master: 100,", "master: 100.5,"),
        Code::E4,
    );
    assert!(d.message.contains("RON"), "{d}");
    expect_error(
        "e4e",
        "se_a.ron",
        &ron.replace("volume: 100)", "volume: 100, bar: 2)"),
        Code::E4,
    );
    expect_error(
        "e4f",
        "se_a.ron",
        &ron.replace("Pulse(duty: D50)", "Pulse(duty: D50, x: 1)"),
        Code::E4,
    );
}

#[test]
fn a09_e5_names() {
    let unknown_inst = Def {
        patterns: r#""m": (rows: ["C-5 lead v15"])"#,
        ..Def::default()
    };
    let d = expect_error("e5a", "se_a.ron", &unknown_inst.ron(), Code::E5);
    assert!(d.message.contains("lead"), "{d}");
    let missing_lib = Def {
        imports: r#""_lib/nothing.ron""#,
        ..Def::default()
    };
    expect_error("e5b", "se_a.ron", &missing_lib.ron(), Code::E5);
    let unknown_pat = Def {
        order: r#"(patterns: ["x"], transpose: 0)"#,
        ..Def::default()
    };
    expect_error("e5c", "se_a.ron", &unknown_pat.ron(), Code::E5);

    let dir = case_dir("e5d");
    fs::create_dir_all(dir.join("_lib")).unwrap();
    fs::write(
        dir.join("_lib/a.ron"),
        format!("LibDef(format: 1, instruments: {{ {INST} }}, patterns: {{}})"),
    )
    .unwrap();
    let dup = Def {
        imports: r#""_lib/a.ron""#,
        ..Def::default()
    };
    fs::write(dir.join("se_a.ron"), dup.ron()).unwrap();
    let (_, d) = render_file(&dir.join("se_a.ron")).unwrap();
    assert_eq!(codes(&d), vec![Code::E5]);
    let shared = Def {
        imports: r#""_lib/a.ron""#,
        instruments: "",
        ..Def::default()
    };
    fs::write(dir.join("se_b.ron"), shared.ron()).unwrap();
    let (s, d) = render_file(&dir.join("se_b.ron")).unwrap();
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(stop_at(&s.unwrap().samples), 1102);
    let bad_path = Def {
        imports: r#""../a.ron""#,
        ..Def::default()
    };
    expect_error("e5e", "se_a.ron", &bad_path.ron(), Code::E5);
}

#[test]
fn a09_e6_score() {
    let bad_row = Def {
        patterns: r#""m": (rows: ["C-5 sq v15", "X-5 .. ..."])"#,
        ..Def::default()
    };
    let d = expect_error("e6a", "se_a.ron", &bad_row.ron(), Code::E6);
    assert!(d.message.contains("パターン m の行 1"), "{d}");
    let uneven = Def {
        channels: r#"(name: "p1", volume: 15, tonal: true), (name: "p2", volume: 15, tonal: true)"#,
        patterns: r#""m": (rows: ["C-5 sq v15", "=== .. ..."]), "n": (rows: ["C-5 sq v15"])"#,
        order: r#"(patterns: ["m", "n"], transpose: 0)"#,
        ..Def::default()
    };
    expect_error("e6b", "se_a.ron", &uneven.ron(), Code::E6);
    let five = Def {
        channels: &[r#"(name: "p", volume: 15, tonal: true)"#; 5].join(","),
        order: r#"(patterns: ["m", "m", "m", "m", "m"], transpose: 0)"#,
        ..Def::default()
    };
    expect_error("e6c", "se_a.ron", &five.ron(), Code::E6);
    let cols = Def {
        order: r#"(patterns: ["m", "m"], transpose: 0)"#,
        ..Def::default()
    };
    expect_error("e6d", "se_a.ron", &cols.ron(), Code::E6);
    let no_prev = Def {
        patterns: r#""m": (rows: ["C-5 .. v15"])"#,
        ..Def::default()
    };
    let d = expect_error("e6e", "se_a.ron", &no_prev.ron(), Code::E6);
    assert!(d.message.contains("order 0 のパターン m の行 0"), "{d}");
    let noise_mismatch = Def {
        patterns: r#""m": (rows: ["N-3 sq v15"])"#,
        ..Def::default()
    };
    expect_error("e6f", "se_a.ron", &noise_mismatch.ron(), Code::E6);
    let all_rest = Def {
        order: r#"(patterns: ["-"], transpose: 0)"#,
        ..Def::default()
    };
    expect_error("e6g", "se_a.ron", &all_rest.ron(), Code::E6);
    let empty_order = Def {
        order: "",
        ..Def::default()
    };
    expect_error("e6h", "se_a.ron", &empty_order.ron(), Code::E6);
}

#[test]
fn a09_e7_ranges() {
    let base = Def::default();
    let cases = [
        Def {
            timing: "Bpm(bpm: 29, rows_per_beat: 4)",
            ..Def::default()
        },
        Def {
            timing: "Bpm(bpm: 301, rows_per_beat: 4)",
            ..Def::default()
        },
        Def {
            timing: "Bpm(bpm: 120, rows_per_beat: 9)",
            ..Def::default()
        },
        Def {
            timing: "RowMs(4)",
            ..Def::default()
        },
        Def {
            master: "401",
            ..Def::default()
        },
        Def {
            channels: r#"(name: "p1", volume: 16, tonal: true)"#,
            ..Def::default()
        },
        Def {
            instruments: r#""sq": (wave: Pulse(duty: D50), env: (attack_ms: 0, decay_ms: 0, sustain: 101, release_ms: 0), volume: 100)"#,
            ..Def::default()
        },
        Def {
            instruments: r#""sq": (wave: Pulse(duty: D50), env: (attack_ms: 0, decay_ms: 0, sustain: 100, release_ms: 0), volume: 101)"#,
            ..Def::default()
        },
        Def {
            instruments: r#""sq": (wave: Triangle, env: (attack_ms: 0, decay_ms: 0, sustain: 100, release_ms: 0), volume: 100, duty_seq: Some((steps: [D12], step_ms: 10, looped: true)))"#,
            ..Def::default()
        },
        Def {
            instruments: r#""sq": (wave: Sine, env: (attack_ms: 0, decay_ms: 0, sustain: 100, release_ms: 0), volume: 100, arp: Some((steps: [], step_ms: 10, looped: true)))"#,
            ..Def::default()
        },
    ];
    for (i, def) in cases.iter().enumerate() {
        expect_error(&format!("e7_{i}"), "se_a.ron", &def.ron(), Code::E7);
    }
    for (i, row) in ["C-9 sq v15", "C-5 sq v16", "C-5 sq v15 R00"]
        .iter()
        .enumerate()
    {
        let pat = format!(r#""m": (rows: ["{row}"])"#);
        let def = Def {
            patterns: &pat,
            ..Def::default()
        };
        expect_error(&format!("e7_row{i}"), "se_a.ron", &def.ron(), Code::E7);
    }
    for (i, timing) in [
        "Bpm(bpm: 30, rows_per_beat: 1)",
        "Bpm(bpm: 300, rows_per_beat: 8)",
        "RowMs(5)",
        "RowMs(1000)",
    ]
    .iter()
    .enumerate()
    {
        let def = Def {
            timing,
            ..Def::default()
        };
        ok(&format!("e7_edge{i}"), "se_a.ron", &def.ron());
    }
    let bgm = Def {
        extra: "loop_to: Some(1),",
        ..base
    };
    expect_error("e7_loop", "bgm_a.ron", &bgm.ron(), Code::E7);
}

const BGM_120: &str = "Bpm(bpm: 120, rows_per_beat: 4)";

#[test]
fn a07_loop_start_and_seam() {
    let one_bar = Def {
        timing: BGM_120,
        patterns: r#""a": (rows: ["C-5 sq v15", "=== .. ...", "E-5 sq v15", "=== .. ...", "G-5 sq v15", "=== .. ...", "... .. ...", "... .. ..."]),
                    "b": (rows: ["D-5 sq v15", "=== .. ...", "F-5 sq v15", "=== .. ...", "A-5 sq v15", "=== .. ...", "... .. ...", "... .. ..."])"#,
        order: r#"(patterns: ["a"], transpose: 0), (patterns: ["b"], transpose: 0), (patterns: ["a"], transpose: 2)"#,
        extra: "loop_to: Some(1),",
        ..Def::default()
    };
    let s = ok("a07", "bgm_a.ron", &one_bar.ron());
    let end = 3 * 22_050;
    assert_eq!(s.samples.len(), end + 22_050);
    assert_eq!(s.loop_start, Some(22_050 + 22_050));
    assert_eq!(&s.samples[end..], &s.samples[22_050..44_100]);

    let long_tail = Def {
        timing: BGM_120,
        instruments: r#""sq": (wave: Pulse(duty: D50), env: (attack_ms: 0, decay_ms: 0, sustain: 100, release_ms: 1500), volume: 100)"#,
        patterns: &format!(
            r#""a": (rows: [{}, "C-5 sq v15", "... .. ...", "... .. ...", "=== .. ..."])"#,
            [r#""... .. ...""#; 12].join(",")
        ),
        order: r#"(patterns: ["a"], transpose: 0)"#,
        extra: "loop_to: Some(0),",
        ..Def::default()
    };
    let d = expect_error("a07_e8", "bgm_b.ron", &long_tail.ron(), Code::E8);
    assert!(d.message.contains("チャンネル p1"), "{d}");
}

#[test]
fn a07_seam_matches_continuation() {
    let def = |order: &str, loop_to: u8| {
        let extra = format!("loop_to: Some({loop_to}),");
        Def {
            timing: BGM_120,
            channels: r#"(name: "p1", volume: 15, tonal: true), (name: "nz", volume: 0, tonal: false)"#,
            instruments: r#""sq": (wave: Pulse(duty: D25), env: (attack_ms: 2, decay_ms: 50, sustain: 60, release_ms: 40), volume: 80, vibrato: Some((depth_cents: 20, rate_millihz: 6000, delay_ms: 30))),
                            "hat": (wave: Noise(mode: Short), env: (attack_ms: 0, decay_ms: 25, sustain: 0, release_ms: 0), volume: 50)"#,
            patterns: r#""a": (rows: ["C-5 sq v15", "... .. ... A47", "=== .. ...", "G-5 sq v12 V35", "=== .. ...", "E-5 sq v15", "... .. ...", "=== .. ..."]),
                        "h": (rows: ["N-1 hat v10", "... .. ...", "N-1 .. ...", "... .. ...", "N-1 .. ...", "... .. ...", "N-1 .. ...", "... .. ..."])"#,
            order,
            extra: &extra,
            ..Def::default()
        }
        .ron()
    };
    let lap = r#"(patterns: ["a", "h"], transpose: 5), (patterns: ["a", "-"], transpose: -3)"#;
    let first = r#"(patterns: ["a", "h"], transpose: 0)"#;
    let looped = ok("a07_cont", "bgm_c.ron", &def(&format!("{first}, {lap}"), 1));
    let unrolled = ok(
        "a07_unrolled",
        "bgm_c.ron",
        &def(&format!("{first}, {lap}, {lap}"), 3),
    );
    assert_eq!(looped.loop_start, Some(22_050 + 22_050));
    assert_eq!(looped.samples.len(), 3 * 22_050 + 22_050);
    assert_eq!(looped.samples, unrolled.samples[..looped.samples.len()]);
}

#[test]
fn a09_e8_mismatch_inside_window() {
    let hold = r#""... .. ...""#;
    let pat = format!(
        r#""a": (rows: ["C-5 sq v15", "=== .. ...", {six}]),
            "z": (rows: [{twelve}, "C-5 .. v15", "=== .. ...", {six}, "E-5 tri v15", "=== .. ...", {hold}, {hold}])"#,
        six = [hold; 6].join(","),
        twelve = [hold; 12].join(","),
    );
    let def = Def {
        timing: BGM_120,
        instruments: &format!(
            r#"{INST}, "tri": (wave: Triangle, env: (attack_ms: 0, decay_ms: 0, sustain: 100, release_ms: 0), volume: 100)"#
        ),
        patterns: &pat,
        order: r#"(patterns: ["a"], transpose: 0), (patterns: ["z"], transpose: 0)"#,
        extra: "loop_to: Some(1),",
        ..Def::default()
    };
    let d = expect_error("e8_mid", "bgm_m.ron", &def.ron(), Code::E8);
    assert!(d.message.contains("order 1 の行 12"), "{d}");
    assert!(d.message.contains("戻り先から 1500 ms"), "{d}");
}

#[test]
fn order_rest_releases_held_note() {
    let def = Def {
        channels: r#"(name: "p1", volume: 15, tonal: true), (name: "p2", volume: 15, tonal: true)"#,
        patterns: r#""held": (rows: ["C-5 sq v15", "... .. ..."]), "quiet": (rows: ["... .. ...", "... .. ..."])"#,
        order: r#"(patterns: ["held", "quiet"], transpose: 0), (patterns: ["-", "quiet"], transpose: 0)"#,
        ..Def::default()
    };
    assert_eq!(stop_at(&ok("rest", "se_a.ron", &def.ron()).samples), 2205);
}

#[test]
fn a09_e8_wrong_continuation_is_detected() {
    let z = format!(
        r#""a": (rows: ["C-5 sq v15", {hold}, {hold}, {hold}, {hold}, {hold}, {hold}, {hold}]),
            "z": (rows: [{twelve}, "E-5 sq v15", {hold}, {hold}, {hold}])"#,
        hold = r#""... .. ...""#,
        twelve = [r#""... .. ...""#; 12].join(",")
    );
    let def = Def {
        timing: BGM_120,
        patterns: &z,
        order: r#"(patterns: ["a"], transpose: 0), (patterns: ["z"], transpose: 0)"#,
        extra: "loop_to: Some(1),",
        ..Def::default()
    };
    let d = expect_error("e8b", "bgm_d.ron", &def.ron(), Code::E8);
    assert!(d.message.contains("order 1"), "{d}");
}

#[test]
fn a09_e9_clip() {
    let loud = Def {
        channels: &[r#"(name: "p", volume: 15, tonal: true)"#; 4].join(","),
        instruments: r#""sq": (wave: Pulse(duty: D50), env: (attack_ms: 0, decay_ms: 0, sustain: 100, release_ms: 0), volume: 100, quantize: false)"#,
        order: r#"(patterns: ["m", "m", "m", "m"], transpose: 0)"#,
        master: "400",
        ..Def::default()
    };
    let (s, d) = render("e9", "se_a.ron", &loud.ron());
    assert!(codes(&d).contains(&Code::E9), "{d:?}");
    assert!(s.unwrap().clipped > 0);
    let allowed = Def {
        extra: "allow_clip: true,",
        ..loud
    };
    let (s, d) = render("e9b", "se_a.ron", &allowed.ron());
    assert!(d.iter().all(|x| !x.code.is_error()), "{d:?}");
    assert!(codes(&d).contains(&Code::W3));
    assert_eq!(s.unwrap().peak, 32_768);
}

#[test]
fn a09_e9_negative_clip_only() {
    let inst = |name: &str, wave: &str| {
        format!(
            r#""{name}": (wave: {wave}, env: (attack_ms: 0, decay_ms: 0, sustain: 100, release_ms: 0), volume: 100, quantize: false)"#
        )
    };
    let def = Def {
        channels: &[r#"(name: "p", volume: 15, tonal: true)"#; 4].join(","),
        instruments: &format!("{}, {}", inst("saw", "Saw"), inst("tri", "Triangle")),
        patterns: r#""s": (rows: ["C-5 saw v15"]), "t": (rows: ["C-5 tri v15"])"#,
        order: r#"(patterns: ["s", "s", "t", "t"], transpose: 0)"#,
        master: "110",
        ..Def::default()
    };
    let (s, d) = render("e9_neg", "se_a.ron", &def.ron());
    assert!(codes(&d).contains(&Code::E9), "{d:?}");
    let s = s.unwrap();
    assert!(s.clipped > 0);
    assert_eq!(s.samples.iter().min(), Some(&i16::MIN));
    assert!(*s.samples.iter().max().unwrap() < 20_000);
}

#[test]
fn a09_e10_length() {
    let rows = [r#""C-5 sq v15""#; 181].join(",");
    let pat = format!(r#""m": (rows: [{rows}])"#);
    let long = Def {
        timing: "RowMs(1000)",
        patterns: &pat,
        ..Def::default()
    };
    expect_error("e10a", "se_a.ron", &long.ron(), Code::E10);
    let silent = Def {
        patterns: r#""m": (rows: ["C-5 sq v00"])"#,
        ..Def::default()
    };
    let (s, d) = render("e10b", "se_a.ron", &silent.ron());
    assert_eq!(codes(&d), vec![Code::E10]);
    assert!(s.unwrap().samples.is_empty());
}

fn first_nonzero(s: &[i16]) -> usize {
    s.iter().position(|&v| v != 0).unwrap()
}

#[test]
fn effect_timing_kill_delay_retrig() {
    let kill = Def {
        timing: "RowMs(200)",
        patterns: r#""m": (rows: ["C-5 sq v15 K03"])"#,
        ..Def::default()
    };
    assert_eq!(stop_at(&ok("fx_k", "se_a.ron", &kill.ron()).samples), 1102);
    let delay = Def {
        timing: "RowMs(200)",
        patterns: r#""m": (rows: ["C-5 sq v15 S02", "=== .. ..."])"#,
        ..Def::default()
    };
    assert_eq!(
        first_nonzero(&ok("fx_s", "se_a.ron", &delay.ron()).samples),
        735
    );
    let retrig = Def {
        timing: "RowMs(100)",
        instruments: r#""sq": (wave: Pulse(duty: D50), env: (attack_ms: 0, decay_ms: 10, sustain: 0, release_ms: 0), volume: 100)"#,
        patterns: r#""m": (rows: ["C-5 sq v15 R02"])"#,
        ..Def::default()
    };
    let s = ok("fx_r", "se_a.ron", &retrig.ron()).samples;
    assert_ne!(s[735], 0);
    assert_eq!(s[734], 0);
    assert_ne!(s[1470], 0);
    let cut = Def {
        patterns: r#""m": (rows: ["C-5 sq v15", "^^^ .. ...", "C-5 .. ..."])"#,
        ..Def::default()
    };
    let s = ok("fx_cut", "se_a.ron", &cut.ron()).samples;
    assert_eq!(stop_at(&s[..2205]), 1102);
    assert!(s[1102 + 400..2205].iter().all(|&v| v == 0));
    assert_ne!(s[2205], 0);
}

#[test]
fn transpose_skips_noise_channel() {
    let def = Def {
        channels: r#"(name: "p1", volume: 15, tonal: true)"#,
        patterns: r#""m": (rows: ["C-5 sq v15", "=== .. ..."])"#,
        order: r#"(patterns: ["m"], transpose: 12)"#,
        ..Def::default()
    };
    let up = ok("tr_up", "se_a.ron", &def.ron());
    let plain = ok("tr_plain", "se_a.ron", &Def::default().ron());
    assert_ne!(up.samples, plain.samples);
    let flat = Def {
        channels: r#"(name: "p1", volume: 15, tonal: false)"#,
        order: r#"(patterns: ["m"], transpose: 12)"#,
        ..Def::default()
    };
    assert_eq!(
        ok("tr_flat", "se_a.ron", &flat.ron()).samples,
        plain.samples
    );
}
