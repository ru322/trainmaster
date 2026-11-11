//! 本番の音(`assets/audio`)の波形の検査: 切り詰め、音量のそろい、長さ、音の高さ、音色、ループの継ぎ目、段階ごとの主旋律(IMP-302、IMP-812)。

use std::f64::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tm_audiogen::{BUDGET_BYTES, Sound, SoundKind, build_dir, render_file};

const SR: usize = 22_050;

fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/audio")
}

fn sounds() -> Vec<Sound> {
    let b = build_dir(&assets_dir()).unwrap();
    assert!(b.diagnostics.is_empty(), "{:?}", b.diagnostics);
    assert!(b.total_bytes() <= BUDGET_BYTES);
    b.sounds
}

fn find<'a>(s: &'a [Sound], name: &str) -> &'a Sound {
    s.iter().find(|x| x.name == name).unwrap()
}

fn source(name: &str) -> String {
    fs::read_to_string(assets_dir().join(format!("{name}.ron"))).unwrap()
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// `keep` 以外のチャンネルの音量を 0 にして合成する。
fn solo(name: &str, keep: &[&str]) -> Vec<i16> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tm-audiogen-solo")
        .join(format!(
            "{name}-{}-{}",
            keep.join("-"),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("_lib")).unwrap();
    for e in fs::read_dir(assets_dir().join("_lib")).unwrap() {
        let e = e.unwrap();
        fs::copy(e.path(), dir.join("_lib").join(e.file_name())).unwrap();
    }
    let text: Vec<String> = source(name)
        .lines()
        .map(|l| {
            let ch = l
                .trim_start()
                .strip_prefix("(name: \"")
                .and_then(|r| r.split('"').next());
            match ch {
                Some(c) if !keep.contains(&c) => l.replace("volume: 15", "volume: 0"),
                _ => l.to_string(),
            }
        })
        .collect();
    let path = dir.join(format!("{name}.ron"));
    fs::write(&path, text.join("\n")).unwrap();
    let (s, d) = render_file(&path).unwrap();
    assert!(d.is_empty(), "{name}: {d:?}");
    s.unwrap().samples
}

fn pct(peak: u32) -> u32 {
    peak * 100 / 32_768
}

fn rms_ac(w: &[i16]) -> f64 {
    let mean = w.iter().map(|&v| f64::from(v)).sum::<f64>() / w.len() as f64;
    let sum: f64 = w.iter().map(|&v| (f64::from(v) - mean).powi(2)).sum();
    (sum / w.len() as f64).sqrt()
}

/// 直流を除いた、いちばん大きい 100ms の区間の RMS(dBFS)。短い音は全体。
fn loudness_db(s: &[i16]) -> f64 {
    let win = (SR / 10).min(s.len());
    let best = (0..=s.len() - win)
        .step_by(SR / 50)
        .map(|i| rms_ac(&s[i..i + win]))
        .fold(0.0, f64::max);
    20.0 * (best / 32_768.0).log10()
}

fn goertzel(w: &[i16], freq: f64) -> f64 {
    let mean = w.iter().map(|&v| f64::from(v)).sum::<f64>() / w.len() as f64;
    let k = 2.0 * (2.0 * PI * freq / SR as f64).cos();
    let (mut s1, mut s2) = (0.0, 0.0);
    for &v in w {
        let s0 = f64::from(v) - mean + k * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - k * s1 * s2
}

/// 音名(`C5`、`F#5`)の MIDI 番号。
fn midi(note: &str) -> i32 {
    let b = note.as_bytes();
    let semi = match b[0] {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        _ => 11,
    } + i32::from(b[1] == b'#');
    let octave = i32::from(*b.last().unwrap() - b'0');
    12 * (octave + 1) + semi
}

fn hz(m: i32) -> f64 {
    440.0 * 2f64.powf(f64::from(m - 69) / 12.0)
}

/// `start` から `len` サンプルの区間の基音が `m` であること(±2 半音と ±1 オクターブより強い)。
fn assert_pitch(label: &str, s: &[i16], start: usize, len: usize, m: i32) {
    let w = &s[start..start + len];
    let p = goertzel(w, hz(m));
    for d in [-12, -2, -1, 1, 2, 12] {
        let q = goertzel(w, hz(m + d));
        assert!(
            p > q,
            "{label}: {start} サンプル目の音が MIDI {m} でない(+{d} が強い)"
        );
    }
}

fn row_start(bpm: usize, row: usize) -> usize {
    row * 1_323_000 / (bpm * 4)
}

fn ms(v: usize) -> usize {
    v * SR / 1000
}

fn order_rows(name: &str) -> Vec<(String, i32)> {
    source(name)
        .lines()
        .filter_map(|l| l.trim().strip_prefix("(patterns: [\""))
        .map(|r| {
            let first = r.split('"').next().unwrap().to_string();
            let tr = r.rsplit("transpose: ").next().unwrap();
            (first, tr.trim_end_matches(&[')', ','][..]).parse().unwrap())
        })
        .collect()
}

/// 矩形波の正の側の割合(デューティ比の目安)。
fn duty(s: &[i16]) -> f64 {
    let pos = s.iter().filter(|&&v| v > 0).count();
    let neg = s.iter().filter(|&&v| v < 0).count();
    pos as f64 / (pos + neg) as f64
}

#[test]
fn no_clipping_and_peaks_in_range() {
    for s in sounds() {
        assert_eq!(s.clipped, 0, "{}", s.name);
        let (lo, hi) = match (s.name.as_str(), s.kind) {
            ("se_coin", _) => (20, 35),
            (_, SoundKind::Bgm) => (30, 50),
            (_, SoundKind::Jingle) => (80, 90),
            (_, SoundKind::Se) => (25, 50),
        };
        let p = pct(s.peak);
        assert!((lo..=hi).contains(&p), "{}: 最大の振幅 {p}%", s.name);
    }
}

/// 初期の音量のバスのゲイン(音楽 6、音 8)を掛けた後の大きさ(dBFS)。14 §6.1 PL-D-23。
fn heard_db(s: &Sound) -> f64 {
    let bus: f64 = match s.kind {
        SoundKind::Se => 41_943.0,
        _ => 23_593.0,
    };
    loudness_db(&s.samples) + 20.0 * (bus / 65_536.0).log10()
}

#[test]
fn loudness_is_balanced() {
    let all = sounds();
    let of = |kind| {
        all.iter()
            .filter(move |s| s.kind == kind)
            .map(|s| (s.name.as_str(), heard_db(s)))
            .collect::<Vec<_>>()
    };
    let spread = |v: &[(&str, f64)]| {
        let max = v.iter().map(|x| x.1).fold(f64::MIN, f64::max);
        let min = v.iter().map(|x| x.1).fold(f64::MAX, f64::min);
        (min, max)
    };
    let bgm = of(SoundKind::Bgm);
    let jingle = of(SoundKind::Jingle);
    let (bgm_min, bgm_max) = spread(&bgm);
    let (j_min, j_max) = spread(&jingle);
    assert!(bgm_max - bgm_min <= 2.5, "{bgm:?}");
    assert!(j_max - j_min <= 2.0, "{jingle:?}");
    assert!(
        j_min >= bgm_max + 4.0,
        "ジングル ≥ BGM + 4dB: {jingle:?} {bgm:?}"
    );
    for s in all.iter().filter(|s| s.kind == SoundKind::Se) {
        let db = loudness_db(&s.samples);
        assert!((-19.0..=-10.0).contains(&db), "{}: {db:.1} dBFS", s.name);
    }
    let buy = heard_db(find(&all, "se_buy"));
    let coin = heard_db(find(&all, "se_coin"));
    assert!(
        j_min >= buy - 3.0,
        "ジングル ≥ se_buy − 3dB: {j_min:.1} / {buy:.1}"
    );
    assert!(
        coin <= buy - 2.0,
        "se_coin ≤ se_buy − 2dB: {coin:.1} / {buy:.1}"
    );
}

#[test]
fn no_dc_offset() {
    for s in sounds() {
        let mean = s.samples.iter().map(|&v| i64::from(v)).sum::<i64>() / s.samples.len() as i64;
        assert!(mean.abs() <= 328, "{}: 直流 {mean}", s.name);
    }
}

#[test]
fn lengths_match_the_plan() {
    let all = sounds();
    for (name, bars, bpm) in [
        ("bgm_title", 16, 132),
        ("bgm_game_1", 32, 112),
        ("bgm_game_2", 32, 124),
        ("bgm_game_3", 32, 136),
    ] {
        let s = find(&all, name);
        let body = bars * 4 * 1_323_000 / bpm;
        assert_eq!(s.samples.len(), body + SR, "{name}");
        assert_eq!(s.loop_start, Some(SR as u32), "{name}");
    }
    for (name, lo, hi) in [
        ("jingle_company_up", 1_800, 2_400),
        ("jingle_all_unlocked", 2_700, 3_600),
        ("jingle_goal", 5_700, 6_600),
    ] {
        let ms = find(&all, name).duration_ms();
        assert!((lo..=hi).contains(&ms), "{name}: {ms} ms");
    }
}

/// パターン名 → 行(`"C-5 mel v15"` など)。
fn patterns(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for l in text.lines().map(str::trim) {
        if let Some(name) = l.strip_suffix("\": (rows: [") {
            out.push((name.trim_start_matches('"').to_string(), Vec::new()));
        } else if let (Some(r), Some(last)) = (l.strip_suffix("\","), out.last_mut()) {
            last.1.push(r.trim_start_matches('"').to_string());
        }
    }
    out
}

/// 1 小節の主旋律: (行, 音の MIDI 番号。`===` は None)。
type Bar = Vec<(usize, Option<i32>)>;

const GAME: [(&str, usize, i32); 3] = [
    ("bgm_game_1", 112, 0),
    ("bgm_game_2", 124, 0),
    ("bgm_game_3", 136, 2),
];

/// order の 1 列目(p1)の譜面を、移調の前の音で小節ごとに返す。
fn melody(name: &str) -> Vec<(String, Bar)> {
    let mut pats = patterns(&fs::read_to_string(assets_dir().join("_lib/game_theme.ron")).unwrap());
    pats.extend(patterns(&source(name)));
    order_rows(name)
        .into_iter()
        .map(|(p, _)| {
            let rows = &pats.iter().find(|x| x.0 == p).unwrap().1;
            assert_eq!(rows.len(), 16, "{name} {p}");
            let bar = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| !r.starts_with("..."))
                .map(|(i, r)| {
                    (
                        i,
                        (!r.starts_with("===")).then(|| midi(&r[..3].replace('-', ""))),
                    )
                })
                .collect();
            (p, bar)
        })
        .collect()
}

/// 3 曲の主旋律は、冒頭 2 小節の共有の動機(`_lib`)だけが同じで、3 小節目からは小節ごとに音の並びが別(14 §6.3、§6.5)。
#[test]
fn game_melodies_differ_after_the_shared_motive() {
    let lib = fs::read_to_string(assets_dir().join("_lib/game_theme.ron")).unwrap();
    assert!(lib.contains("\"nz_drums\": (rows"));
    let songs: Vec<_> = GAME.iter().map(|g| melody(g.0)).collect();
    for ((name, _, tr), m) in GAME.iter().zip(&songs) {
        assert_eq!(m.len(), 32, "{name}");
        assert!(order_rows(name).iter().all(|x| x.1 == *tr), "{name}");
        assert_eq!(
            (m[0].0.as_str(), m[1].0.as_str()),
            ("th_m1", "th_m2"),
            "{name}"
        );
        for (p, bar) in m {
            for n in bar.iter().filter_map(|x| x.1) {
                assert!(
                    (midi("C5")..=midi("F6")).contains(&n),
                    "{name} {p}: 音域 C5〜F6 の外 {n}"
                );
            }
        }
        if *name != "bgm_game_1" {
            assert!(m[2..].iter().all(|x| !x.0.starts_with("th_")), "{name}");
        }
    }
    let pitches = |b: &Bar| b.iter().filter_map(|x| x.1).collect::<Vec<_>>();
    for i in 0..32 {
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            let (pa, pb) = (pitches(&songs[a][i].1), pitches(&songs[b][i].1));
            if i < 2 {
                assert_eq!(songs[a][i].1, songs[b][i].1);
            } else {
                assert_ne!(
                    pa,
                    pb,
                    "{} と {} の {} 小節目が同じ並び",
                    GAME[a].0,
                    GAME[b].0,
                    i + 1
                );
            }
        }
    }
}

/// 生成した波形の主旋律の音の高さが、譜面のとおり(全部の音)。
#[test]
fn game_melodies_sound_as_written() {
    for (name, bpm, tr) in GAME {
        let p1 = solo(name, &["p1"]);
        let mut count = 0;
        for (i, (p, bar)) in melody(name).iter().enumerate() {
            for &(row, n) in bar {
                let Some(n) = n else { continue };
                let at = row_start(bpm, i * 16 + row) + ms(10);
                assert_pitch(&format!("{name} {p}"), &p1, at, ms(80), n + tr);
                count += 1;
            }
        }
        assert!(count >= 100, "{name}: {count}");
    }
}

#[test]
fn bgm_melody_and_bass_pitches() {
    let bar1 = [(0, "C5"), (4, "A5"), (12, "G5")];
    for (name, bpm, tr) in [
        ("bgm_game_1", 112, 0),
        ("bgm_game_2", 124, 0),
        ("bgm_game_3", 136, 2),
    ] {
        let p1 = solo(name, &["p1"]);
        for (row, note) in bar1 {
            assert_pitch(
                name,
                &p1,
                row_start(bpm, row) + ms(10),
                ms(80),
                midi(note) + tr,
            );
        }
        assert_pitch(
            name,
            &p1,
            row_start(bpm, 16) + ms(10),
            ms(80),
            midi("E5") + tr,
        );
    }
    let title = solo("bgm_title", &["p1"]);
    for (row, note) in [(0, "C5"), (4, "A5"), (12, "E5"), (16, "G5")] {
        assert_pitch(
            "bgm_title",
            &title,
            row_start(132, row) + ms(10),
            ms(60),
            midi(note),
        );
    }
    let bass = solo("bgm_game_1", &["tri"]);
    assert_pitch("bgm_game_1 tri", &bass, ms(20), ms(250), midi("C3"));
    assert_pitch(
        "bgm_game_1 tri",
        &bass,
        row_start(112, 16) + ms(20),
        ms(250),
        midi("A2"),
    );
}

#[test]
fn bgm_timbre_and_drums() {
    let d = duty(&solo("bgm_game_1", &["p1"]));
    assert!((0.20..=0.30).contains(&d), "game_1 主旋律 25%: {d:.3}");
    let d = duty(&solo("bgm_game_1", &["p2"]));
    assert!((0.09..=0.16).contains(&d), "harm 12.5%: {d:.3}");
    let d = duty(&solo("bgm_game_2", &["p1"]));
    assert!((0.45..=0.55).contains(&d), "lead 50%: {d:.3}");

    let peak = |s: &[i16]| {
        pct(s
            .iter()
            .map(|&v| i32::from(v).unsigned_abs())
            .max()
            .unwrap())
    };
    let rail = solo("bgm_game_1", &["noise"]);
    assert!(
        peak(&rail) <= 5,
        "game_1 のノイズは小さな線路の音: {}%",
        peak(&rail)
    );
    for name in ["bgm_game_2", "bgm_game_3", "bgm_title"] {
        let drums = solo(name, &["noise"]);
        assert!(peak(&drums) >= 9, "{name} のドラム: {}%", peak(&drums));
        let bars_with_hits = drums
            .chunks(drums.len() / 16)
            .filter(|c| peak(c) >= 9)
            .count();
        assert!(bars_with_hits >= 15, "{name}: {bars_with_hits}");
    }
}

#[test]
fn se_pitches() {
    let all = sounds();
    let cases: [(&str, usize, &[&str]); 7] = [
        ("se_coin", 40, &["E6", "B6"]),
        ("se_buy", 40, &["C5", "E5", "G5"]),
        ("se_buy_max", 40, &["C5", "E5", "G5", "C6", "E6"]),
        ("se_menu_open", 40, &["G4", "C5", "E5"]),
        ("se_menu_close", 40, &["E5", "C5", "G4"]),
        ("se_reset", 125, &["C5", "G4", "E4", "C4"]),
        ("se_cannot_buy", 75, &["G3"]),
    ];
    for (name, row_ms, notes) in cases {
        let s = &find(&all, name).samples;
        for (i, n) in notes.iter().enumerate() {
            assert_pitch(name, s, ms(row_ms * i + 5), ms(row_ms - 10), midi(n));
        }
    }
    let line = &find(&all, "se_line_new").samples;
    let w = &line[ms(200)..ms(500)];
    let a5 = goertzel(w, hz(midi("A5")));
    let fs5 = goertzel(w, hz(midi("F#5")));
    let g5 = goertzel(w, hz(midi("G5")));
    assert!(a5 > g5 * 10.0 && fs5 > g5 * 10.0, "汽笛の和音 F#5 + A5");
}

/// 同じ音高を 8 セントずらした声で、A5 の大きさが 1 秒に数回ゆれる(うなり)。
#[test]
fn line_whistle_beats() {
    let a5 = solo("se_line_new", &["p2", "p3"]);
    let env: Vec<f64> = a5[ms(180)..ms(520)].chunks(ms(20)).map(rms_ac).collect();
    let max = env.iter().cloned().fold(0.0, f64::max);
    let min = env.iter().cloned().fold(f64::MAX, f64::min);
    assert!(min < max * 0.7, "うなりがない: {min:.0} / {max:.0}");
}

/// ノイズのチャンネルを消すと、ループの直前の区間が戻り先の直前と一致する(継ぎ目で位相が続く)。
#[test]
fn loop_seam_is_phase_continuous() {
    let all = sounds();
    for s in all.iter().filter(|s| s.kind == SoundKind::Bgm) {
        let t = solo(&s.name, &["p1", "p2", "tri"]);
        assert_eq!(t.len(), s.samples.len(), "{}", s.name);
        let ls = s.loop_start.unwrap() as usize;
        let k = SR / 2;
        assert_eq!(t[t.len() - k..], t[ls - k..ls], "{}", s.name);
    }
}
