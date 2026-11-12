//! `tm-fontgen` のコマンドライン(15 §4.1)。

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;
use tm_fontgen::{build, emit, generate, load, preview, proof, report};

const USAGE: &str = "使い方:
  tm-fontgen build [--config assets/fonts/fontgen.toml] [--out crates/tm-assets/generated]
  tm-fontgen check [--config ..]
  tm-fontgen preview [--face <id>] [--scale 1] \"電車の会社\" [-o preview.png]
  tm-fontgen proof [--scale 1] [-o proof]
  tm-fontgen report [--config ..]
  --face の既定は fontgen.toml の最初の face
  共通: --root <dir>(fontgen.toml の中のパスの起点。既定はカレントディレクトリ)";

struct Opts {
    config: PathBuf,
    root: PathBuf,
    out: Option<PathBuf>,
    face: Option<String>,
    scale: u32,
    positional: Vec<String>,
}

fn parse_opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        config: "assets/fonts/fontgen.toml".into(),
        root: ".".into(),
        out: None,
        face: None,
        scale: 1,
        positional: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{a} に値がない"));
        match a.as_str() {
            "--config" => o.config = value()?.into(),
            "--root" => o.root = value()?.into(),
            "--out" | "-o" => o.out = Some(value()?.into()),
            "--face" => o.face = Some(value()?),
            "--scale" => {
                o.scale = value()?
                    .parse()
                    .ok()
                    .filter(|&s| s >= 1)
                    .ok_or("--scale は 1 以上の整数")?;
            }
            _ => o.positional.push(a.clone()),
        }
    }
    Ok(o)
}

fn print_warnings(w: &[String]) {
    for line in w {
        eprintln!("警告: {line}");
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let (cmd, rest) = args.split_first().ok_or(USAGE)?;
    let o = parse_opts(rest)?;
    let inputs = load(&o.config, &o.root).map_err(|e| e.to_string())?;
    match cmd.as_str() {
        "build" => {
            let out = o.out.unwrap_or_else(|| "crates/tm-assets/generated".into());
            let g = build(&inputs, &out).map_err(|e| e.to_string())?;
            print_warnings(&g.warnings);
            println!(
                "{} 字、グリフ {}、帯 {}/{} 行、文言 {}",
                g.checked.chars.len(),
                g.positions.len(),
                g.rows_used,
                g.checked.strip.height,
                g.checked.texts.entries.len()
            );
        }
        "check" => {
            let g = generate(&inputs).map_err(|e| e.to_string())?;
            print_warnings(&g.warnings);
            println!(
                "OK: {} 字、文言 {}",
                g.checked.chars.len(),
                g.checked.texts.entries.len()
            );
        }
        "preview" => {
            let text = o.positional.join("\n");
            let g = generate(&inputs).map_err(|e| e.to_string())?;
            let id = o.face.unwrap_or_else(|| inputs.config.face[0].id.clone());
            let face = g.face(&id).ok_or(format!("face `{id}` がない"))?;
            let img = preview(face, &text, o.scale);
            let out = o.out.unwrap_or_else(|| "preview.png".into());
            let png = emit::encode_png(&img).map_err(|e| e.to_string())?;
            std::fs::write(&out, png).map_err(|e| e.to_string())?;
        }
        "proof" => {
            let g = generate(&inputs).map_err(|e| e.to_string())?;
            let dir = o.out.unwrap_or_else(|| "proof".into());
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let images = proof::proof(&g, o.scale);
            for (name, img) in &images {
                let png = emit::encode_png(img).map_err(|e| e.to_string())?;
                std::fs::write(dir.join(name), png).map_err(|e| e.to_string())?;
            }
            println!("{} 枚を {} に書いた", images.len(), dir.display());
        }
        "report" => {
            let g = generate(&inputs).map_err(|e| e.to_string())?;
            print!("{}", report::report(&g));
            print_warnings(&g.warnings);
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
