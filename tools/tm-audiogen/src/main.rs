//! `tm-audiogen` の CLI(14 §5.1)。

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tm_audiogen::{
    BUDGET_BYTES, Build, Diagnostic, SoundKind, build_dir, manifest, render_file, wav_bytes,
    write_atomic, write_outputs,
};

const USAGE: &str = "使い方:
  tm-audiogen build [--in assets/audio] [--out crates/tm-assets/generated/audio]
  tm-audiogen check [--in assets/audio]
  tm-audiogen render <file.ron> -o <out.wav> [--loops N]
  tm-audiogen list [--in assets/audio]";

const DEFAULT_IN: &str = "assets/audio";
const DEFAULT_OUT: &str = "crates/tm-assets/generated/audio";

struct Args {
    positional: Vec<String>,
    input: PathBuf,
    out: Option<PathBuf>,
    loops: u32,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut a = Args {
        positional: Vec::new(),
        input: PathBuf::from(DEFAULT_IN),
        out: None,
        loops: 1,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{arg} に値がない"))
        };
        match arg.as_str() {
            "--in" => a.input = PathBuf::from(value()?),
            "--out" | "-o" => a.out = Some(PathBuf::from(value()?)),
            "--loops" => {
                a.loops = value()?
                    .parse()
                    .ok()
                    .filter(|&n| n >= 1)
                    .ok_or("--loops は 1 以上の整数")?;
            }
            s if s.starts_with('-') => return Err(format!("知らないオプション: {s}")),
            s => a.positional.push(s.to_string()),
        }
    }
    Ok(a)
}

fn report(diags: &[Diagnostic]) -> bool {
    for d in diags {
        eprintln!("{d}");
    }
    diags.iter().any(|d| d.code.is_error())
}

fn summary(b: &Build) {
    let total = b.total_bytes();
    let permille = total * 1000 / BUDGET_BYTES;
    println!(
        "{} 個の音、WAV の合計 {total} バイト(予算の {}.{}%)",
        b.sounds.len(),
        permille / 10,
        permille % 10
    );
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some((cmd, rest)) = args.split_first() else {
        return Err("コマンドがない".into());
    };
    let a = parse_args(rest)?;
    let io = |e: tm_audiogen::IoError| {
        eprintln!("I/O のエラー: {e}");
        ExitCode::from(2)
    };
    match cmd.as_str() {
        "build" | "check" | "list" => {
            if !a.positional.is_empty() || (cmd != "build" && a.out.is_some()) {
                return Err(format!("{cmd} に余分な引数がある"));
            }
            let b = match build_dir(&a.input) {
                Ok(b) => b,
                Err(e) => return Ok(io(e)),
            };
            let failed = report(&b.diagnostics);
            if cmd == "list" {
                print!("{}", manifest(&b.sounds));
                return Ok(ExitCode::SUCCESS);
            }
            if failed {
                return Ok(ExitCode::from(1));
            }
            if cmd == "build" {
                let out = a.out.unwrap_or_else(|| PathBuf::from(DEFAULT_OUT));
                if let Err(e) = write_outputs(&b, &out) {
                    return Ok(io(e));
                }
            }
            summary(&b);
            Ok(ExitCode::SUCCESS)
        }
        "render" => {
            let ([file], Some(out)) = (a.positional.as_slice(), a.out.as_ref()) else {
                return Err("render には <file.ron> と -o <out.wav> が要る".into());
            };
            let (sound, diags) = match render_file(Path::new(file)) {
                Ok(r) => r,
                Err(e) => return Ok(io(e)),
            };
            let failed = report(&diags);
            let Some(sound) = sound.filter(|_| !failed) else {
                return Ok(ExitCode::from(1));
            };
            let mut samples = sound.samples.clone();
            if let (SoundKind::Bgm, Some(ls)) = (sound.kind, sound.loop_start) {
                for _ in 1..a.loops {
                    samples.extend_from_slice(&sound.samples[ls as usize..]);
                }
            }
            if let Err(e) = write_atomic(out, &wav_bytes(&samples)) {
                return Ok(io(e));
            }
            println!(
                "{}: {} サンプル({} ms)、loop_start {:?}、最大の振幅 {}",
                sound.name,
                samples.len(),
                samples.len() as u64 * 1000 / 22050,
                sound.loop_start,
                sound.peak
            );
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("知らないコマンド: {other}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("{msg}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
