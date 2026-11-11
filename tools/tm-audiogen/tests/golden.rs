//! A-12(ゴールデン)と A-13(2 回の生成が同じ)。

use std::fs;
use std::path::{Path, PathBuf};

use tm_audiogen::{Code, build_dir, fnv64, write_outputs};

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/audio")
}

/// 合成の仕様を変えたときだけ意図して更新する。
const GOLDEN: [(&str, usize, Option<u32>, u64); 4] = [
    ("bgm_loop", 82_186, Some(42_095), 0x88af_35d2_50ec_9161),
    ("jingle_tail", 23_660, None, 0x50ef_9773_9ebe_80ef),
    ("se_effects", 35_280, None, 0xab0b_ea07_8e5e_bc94),
    ("se_waves", 11_923, None, 0xb626_b0e5_876d_34ef),
];

#[test]
fn a12_golden_fnv64() {
    let b = build_dir(&data_dir()).unwrap();
    let errors: Vec<_> = b.diagnostics.iter().filter(|d| d.code.is_error()).collect();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        b.diagnostics.iter().all(|d| d.code == Code::W2),
        "{:?}",
        b.diagnostics
    );
    let got: Vec<_> = b
        .sounds
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.samples.len(),
                s.loop_start,
                fnv64(&s.wav()),
            )
        })
        .collect();
    assert_eq!(got, GOLDEN.to_vec());
}

fn read_tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect();
    v.sort();
    v
}

fn build_twice(input: &Path, case: &str) -> Vec<(String, Vec<u8>)> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tm-audiogen-a13")
        .join(case);
    let _ = fs::remove_dir_all(&root);
    let mut trees = Vec::new();
    for run in ["1", "2"] {
        let b = build_dir(input).unwrap();
        assert!(!b.has_errors(), "{:?}", b.diagnostics);
        let out = root.join(run);
        write_outputs(&b, &out).unwrap();
        trees.push(read_tree(&out));
    }
    assert_eq!(trees[0], trees[1]);
    trees.pop().unwrap()
}

#[test]
fn a13_two_runs_are_identical_test_data() {
    let tree = build_twice(&data_dir(), "data");
    let names: Vec<_> = tree.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "audio_manifest.txt",
            "bgm_loop.wav",
            "jingle_tail.wav",
            "se_effects.wav",
            "se_waves.wav",
            "sounds.rs"
        ]
    );
}

#[test]
fn a13_two_runs_are_identical_assets() {
    let tree = build_twice(&assets_dir(), "assets");
    let wavs = tree.iter().filter(|(n, _)| n.ends_with(".wav")).count();
    assert_eq!(wavs, 23);
    let b = build_dir(&assets_dir()).unwrap();
    assert!(b.diagnostics.is_empty(), "{:?}", b.diagnostics);
}
