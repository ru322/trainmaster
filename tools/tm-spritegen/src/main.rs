//! `tm-spritegen` のコマンドライン(15 §5.7)。

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;
use tm_spritegen::{
    Generated, Layer, SpriteError, atlas, build, compose, emit, generate, load_sources,
};

const USAGE: &str = "使い方:
  tm-spritegen build --dir assets/sprites --atlas assets/atlas.toml --out crates/tm-assets/generated
  tm-spritegen preview <名前[:RRGGBB][@x,y]>... [--scale 6] [-o out.png] [--dir ..] [--atlas ..]
  tm-spritegen report [--dir ..] [--atlas ..]";

struct Opts {
    dir: PathBuf,
    atlas: PathBuf,
    out: PathBuf,
    scale: u32,
    positional: Vec<String>,
}

fn parse_opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        dir: "assets/sprites".into(),
        atlas: "assets/atlas.toml".into(),
        out: "crates/tm-assets/generated".into(),
        scale: 6,
        positional: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{a} に値がない"));
        match a.as_str() {
            "--dir" => o.dir = value()?.into(),
            "--atlas" => o.atlas = value()?.into(),
            "--out" | "-o" => o.out = value()?.into(),
            "--scale" => o.scale = value()?.parse().map_err(|_| "--scale は整数".to_string())?,
            _ => o.positional.push(a.clone()),
        }
    }
    Ok(o)
}

fn generate_from(o: &Opts) -> Result<Generated, SpriteError> {
    let text = std::fs::read_to_string(&o.atlas).map_err(|e| SpriteError::Io {
        path: o.atlas.display().to_string(),
        message: e.to_string(),
    })?;
    generate(&load_sources(&o.dir)?, &atlas::parse(&text)?)
}

fn parse_layer(s: &str) -> Result<Layer, String> {
    let (rest, at) = match s.split_once('@') {
        None => (s, (0, 0)),
        Some((rest, xy)) => {
            let (x, y) = xy.split_once(',').ok_or("@ の後は x,y")?;
            let n = |v: &str| {
                v.parse::<i32>()
                    .map_err(|_| format!("位置 `{v}` が整数でない"))
            };
            (rest, (n(x)?, n(y)?))
        }
    };
    let (name, tint) = match rest.split_once(':') {
        None => (rest, None),
        Some((name, hex)) => {
            let v =
                u32::from_str_radix(hex, 16).map_err(|_| format!("色 `{hex}` が 16 進でない"))?;
            (name, Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]))
        }
    };
    Ok(Layer {
        name: name.into(),
        tint,
        at,
    })
}

fn run(args: &[String]) -> Result<(), String> {
    let (cmd, rest) = args.split_first().ok_or(USAGE)?;
    let o = parse_opts(rest)?;
    match cmd.as_str() {
        "build" => {
            let g = build(&o.dir, &o.atlas, &o.out).map_err(|e| e.to_string())?;
            println!(
                "{} 個のスプライト、帯 {}/{} 行",
                g.sprites.len(),
                g.rows_used,
                g.strip.height
            );
        }
        "preview" => {
            let g = generate_from(&o).map_err(|e| e.to_string())?;
            let layers = o
                .positional
                .iter()
                .map(|s| parse_layer(s))
                .collect::<Result<Vec<_>, _>>()?;
            let img = compose(&g, &layers, o.scale)?;
            let out = if o.out.extension().is_some_and(|e| e == "png") {
                o.out.clone()
            } else {
                "preview.png".into()
            };
            let png = emit::encode_png(&img).map_err(|e| e.to_string())?;
            std::fs::write(&out, png).map_err(|e| e.to_string())?;
        }
        "report" => {
            let g = generate_from(&o).map_err(|e| e.to_string())?;
            let pixels: u32 = g.sprites.iter().map(|s| s.image.w * s.image.h).sum();
            let alt: u32 = g
                .sprites
                .iter()
                .filter_map(|s| s.alt.as_ref())
                .map(|a| a.w * a.h)
                .sum();
            println!(
                "スプライト: {}(大きい版 {})",
                g.sprites.len(),
                g.sprites.iter().filter(|s| s.alt.is_some()).count()
            );
            println!("画素: {pixels}(大きい版 {alt})");
            println!("帯の使用: {}/{} 行", g.rows_used, g.strip.height);
            println!("パレット: {}/32 色", g.colors);
            println!("PNG: {} バイト", g.png.len());
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
