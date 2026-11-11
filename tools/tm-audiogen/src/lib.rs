//! 曲と音色の定義から 8bit 風の BGM と効果音を合成して WAV を作る開発用ツール。

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::float_arithmetic)]

mod codegen;
mod load;
mod render;
mod schema;
mod synth;
mod tables;
mod tracker;
mod validate;
mod wav;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use codegen::{manifest, sounds_rs};
pub use validate::{BUDGET_BYTES, Code, Diagnostic, EXPECTED_IDS};
pub use wav::{WAV_HEADER_BYTES, fnv64, wav_bytes};

use load::Source;
use render::Song;
use tables::SAMPLE_RATE;

/// 音の種類(名前の接頭辞 `se_` / `bgm_` / `jingle_` で決まる)。
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundKind {
    Se,
    Bgm,
    Jingle,
}

/// 合成した 1 つの音。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sound {
    /// 音の ID(ファイル名から拡張子を除いたもの)。
    pub name: String,
    /// 種類。
    pub kind: SoundKind,
    /// 22050Hz・モノラルのサンプル列。
    pub samples: Vec<i16>,
    /// ループの戻り先のサンプル番号(Bgm だけ)。
    pub loop_start: Option<u32>,
    /// 切り詰めたサンプルの数。
    pub clipped: u64,
    /// 最大の振幅の絶対値。
    pub peak: u32,
}

impl Sound {
    /// 長さ(ミリ秒、切り捨て)。
    pub fn duration_ms(&self) -> u64 {
        self.samples.len() as u64 * 1000 / u64::from(SAMPLE_RATE)
    }

    /// WAV のファイル全体のバイト列。
    pub fn wav(&self) -> Vec<u8> {
        wav_bytes(&self.samples)
    }
}

/// 入出力のエラー(終了コード 2)。
#[derive(Debug)]
pub struct IoError {
    /// 対象のパス。
    pub path: PathBuf,
    /// 元のエラー。
    pub source: io::Error,
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for IoError {}

pub(crate) enum Fail {
    Check(Diagnostic),
    Io(IoError),
}

/// ディレクトリのすべての音を検査して合成した結果。
#[derive(Debug, Default)]
pub struct Build {
    /// 合成できた音(名前の辞書順)。
    pub sounds: Vec<Sound>,
    /// エラーと警告。
    pub diagnostics: Vec<Diagnostic>,
}

impl Build {
    /// エラーが 1 つでもあれば true。
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.code.is_error())
    }

    /// WAV のファイルの大きさの合計。
    pub fn total_bytes(&self) -> u64 {
        self.sounds
            .iter()
            .map(|s| (WAV_HEADER_BYTES + s.samples.len() * 2) as u64)
            .sum()
    }
}

fn render_source(base: &Path, src: &Source) -> Result<(Sound, Vec<Diagnostic>), Fail> {
    let label = format!("{}.ron", src.name);
    let merged = load::load_sound(base, &label, &src.text)?;
    validate::check_def(&label, src.kind, &merged).map_err(Fail::Check)?;
    let song = Song::compile(&label, src.kind, &merged).map_err(Fail::Check)?;
    let out = song.render().map_err(Fail::Check)?;
    let peak = out
        .samples
        .iter()
        .map(|&v| i32::from(v).unsigned_abs())
        .max()
        .unwrap_or(0);
    let sound = Sound {
        name: src.name.clone(),
        kind: src.kind,
        samples: out.samples,
        loop_start: out.loop_start,
        clipped: out.clipped,
        peak,
    };
    let diags = validate::check_sound(&label, &sound, merged.def.allow_clip);
    Ok((sound, diags))
}

/// `in_dir` のすべての音を読み込み、検査し、合成する(E1〜E10、W1〜W4)。
pub fn build_dir(in_dir: &Path) -> Result<Build, IoError> {
    let (sources, mut diagnostics) = load::list_sounds(in_dir)?;
    let mut sounds = Vec::new();
    for src in &sources {
        match render_source(in_dir, src) {
            Ok((sound, diags)) => {
                sounds.push(sound);
                diagnostics.extend(diags);
            }
            Err(Fail::Check(d)) => diagnostics.push(d),
            Err(Fail::Io(e)) => return Err(e),
        }
    }
    let mut build = Build {
        sounds,
        diagnostics,
    };
    let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
    let set = validate::check_set(&names, build.total_bytes());
    build.diagnostics.extend(set);
    Ok(build)
}

/// 1 つのファイルを合成する(imports はファイルのあるディレクトリから読む)。合成できなければ `None`。
pub fn render_file(path: &Path) -> Result<(Option<Sound>, Vec<Diagnostic>), IoError> {
    let base = path.parent().unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = file_name.strip_suffix(".ron").unwrap_or(&file_name);
    let Some(kind) = load::kind_of(name) else {
        let d = Diagnostic::new(
            Code::E1,
            &file_name,
            "名前が ^(se|bgm|jingle)_[a-z0-9_]+\\.ron$ に合わない",
        );
        return Ok((None, vec![d]));
    };
    let text = fs::read_to_string(path).map_err(|source| IoError {
        path: path.to_path_buf(),
        source,
    })?;
    let src = Source {
        name: name.to_string(),
        kind,
        text,
    };
    match render_source(base, &src) {
        Ok((sound, diags)) => Ok((Some(sound), diags)),
        Err(Fail::Check(d)) => Ok((None, vec![d])),
        Err(Fail::Io(e)) => Err(e),
    }
}

/// 一時ファイルに書いてから rename する(途中で失敗しても半端なファイルを残さない)。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), IoError> {
    let io_err = |source| IoError {
        path: path.to_path_buf(),
        source,
    };
    let file_name = path
        .file_name()
        .ok_or_else(|| io_err(io::Error::from(io::ErrorKind::InvalidInput)))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    fs::write(&tmp, bytes).map_err(io_err)?;
    fs::rename(&tmp, path).map_err(io_err)
}

/// WAV、`sounds.rs`、`audio_manifest.txt` を `out_dir` に書く(ほかのファイルは消さない)。
pub fn write_outputs(build: &Build, out_dir: &Path) -> Result<(), IoError> {
    fs::create_dir_all(out_dir).map_err(|source| IoError {
        path: out_dir.to_path_buf(),
        source,
    })?;
    for s in &build.sounds {
        write_atomic(&out_dir.join(format!("{}.wav", s.name)), &s.wav())?;
    }
    write_atomic(
        &out_dir.join("sounds.rs"),
        sounds_rs(&build.sounds).as_bytes(),
    )?;
    write_atomic(
        &out_dir.join("audio_manifest.txt"),
        manifest(&build.sounds).as_bytes(),
    )
}
