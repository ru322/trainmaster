//! ドット絵の定義からスプライトの帯の PNG と表を作る開発用ツール。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod art;
pub mod atlas;
pub mod def;
pub mod draw;
pub mod emit;
pub mod error;
pub mod image;
pub mod pack;
pub mod palette;
pub mod resolve;
pub mod upscale;
pub mod vector;

pub use error::SpriteError;

use atlas::Strip;
use image::Image;
use palette::Palette;
use resolve::Sprite;
use std::path::Path;

pub(crate) fn ron_options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

/// 名前の規則 `^[a-z][a-z0-9_]*$` を検査する(SG-E02)。
pub fn check_name(name: &str) -> Result<(), SpriteError> {
    let mut chars = name.chars();
    let ok = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(SpriteError::Name {
            name: name.into(),
            reason: "名前は ^[a-z][a-z0-9_]*$".into(),
        })
    }
}

/// 読み込んだ元データ(ファイルの中身そのまま)。
#[derive(Debug, Clone, Default)]
pub struct Sources {
    /// `palette.ron` の中身。
    pub palette: String,
    /// スプライト定義の (ファイル名, 中身)。
    pub files: Vec<(String, String)>,
}

/// 生成物。
#[derive(Debug, Clone)]
pub struct Generated {
    /// 帯の位置。
    pub strip: Strip,
    /// 名前の辞書順(= `SpriteId` の順)のスプライト。
    pub sprites: Vec<Sprite>,
    /// `sprites` と同じ順の左上座標(ページの絶対座標)。
    pub positions: Vec<(u32, u32)>,
    /// `sprites` と同じ順の、大きい版の左上座標。
    pub alt_positions: Vec<Option<(u32, u32)>>,
    /// 帯の画像(幅はページの幅、高さは帯の高さ)。
    pub image: Image,
    /// 帯の PNG。
    pub png: Vec<u8>,
    /// `sprites.rs` の中身。
    pub sprites_rs: String,
    /// `palette.rs` の中身。
    pub palette_rs: String,
    /// パレットの色数。
    pub colors: usize,
    /// 使った帯の行数。
    pub rows_used: u32,
}

impl Generated {
    /// PNG のファイル名(`atlas_p<page>_s<strip>_sprites.png`)。
    pub fn png_name(&self) -> String {
        format!(
            "atlas_p{}_s{}_sprites.png",
            self.strip.page, self.strip.index
        )
    }
}

fn io_err(path: &Path, e: std::io::Error) -> SpriteError {
    SpriteError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

/// `dir` の `palette.ron` と、そのほかの `*.ron`(ファイル名順)を読む。
pub fn load_sources(dir: &Path) -> Result<Sources, SpriteError> {
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| io_err(p, e));
    let palette = read(&dir.join("palette.ron"))?;
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| io_err(dir, e))?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".ron") && n != "palette.ron")
        .collect();
    names.sort();
    let mut files = Vec::new();
    for n in names {
        let text = read(&dir.join(&n))?;
        files.push((n, text));
    }
    Ok(Sources { palette, files })
}

/// 元データから、帯の画像・PNG・`sprites.rs`・`palette.rs` を作る。同じ入力から同じバイト列になる。
pub fn generate(src: &Sources, strip: &Strip) -> Result<Generated, SpriteError> {
    let pal = Palette::parse(&src.palette)?;
    let mut files = Vec::new();
    for (name, text) in &src.files {
        let f: def::SpriteFile = ron_options()
            .from_str(text)
            .map_err(|e| SpriteError::Syntax {
                file: name.clone(),
                message: e.to_string(),
            })?;
        files.push((name.clone(), f));
    }
    let mut resolved = resolve::resolve(&files, &pal, strip.render_scale)?;
    resolved.sprites.sort_by(|a, b| a.name.cmp(&b.name));
    check_frames_consecutive(&resolved)?;
    let alt_names: Vec<String> = resolved
        .sprites
        .iter()
        .map(|s| format!("{}@2", s.name))
        .collect();
    let mut images: Vec<&Image> = Vec::new();
    let mut items: Vec<(u32, u32, &str)> = Vec::new();
    for (s, alt_name) in resolved.sprites.iter().zip(&alt_names) {
        images.push(&s.image);
        items.push((s.image.w, s.image.h, s.name.as_str()));
        if let Some(a) = &s.alt {
            images.push(a);
            items.push((a.w, a.h, alt_name.as_str()));
        }
    }
    let (placed, rows_used) = pack::pack(&items, strip)?;
    let mut image = Image::new(strip.width, strip.height);
    for (img, &(x, y)) in images.iter().zip(&placed) {
        for sy in 0..img.h {
            for sx in 0..img.w {
                let p = img.px[(sy * img.w + sx) as usize];
                image.set((x + sx) as i32, (y - strip.y + sy) as i32, p);
            }
        }
    }
    let mut it = placed.into_iter();
    let mut positions = Vec::with_capacity(resolved.sprites.len());
    let mut alt_positions = Vec::with_capacity(resolved.sprites.len());
    for s in &resolved.sprites {
        positions.push(it.next().expect("詰めた数"));
        alt_positions.push(s.alt.as_ref().map(|_| it.next().expect("詰めた数")));
    }
    let rows: Vec<emit::SpriteRow<'_>> = resolved
        .sprites
        .iter()
        .zip(positions.iter().zip(&alt_positions))
        .map(|(s, (&pos, &alt))| emit::SpriteRow {
            name: &s.name,
            pos,
            texels: (s.image.w, s.image.h),
            alt: alt.zip(s.alt.as_ref().map(|a| (a.w, a.h))),
            size: s.size,
            pivot: s.pivot,
            tint: s.tint,
        })
        .collect();
    let sprites_rs = emit::sprites_rs(&rows, &resolved, strip)?;
    Ok(Generated {
        strip: *strip,
        png: emit::encode_png(&image)?,
        image,
        sprites_rs,
        palette_rs: emit::palette_rs(&pal),
        colors: pal.colors.len(),
        rows_used,
        positions,
        alt_positions,
        sprites: resolved.sprites,
    })
}

fn check_frames_consecutive(r: &resolve::Resolved) -> Result<(), SpriteError> {
    for a in &r.anims {
        let first = format!("{}_f0", a.name);
        let Some(i) = r.sprites.iter().position(|s| s.name == first) else {
            continue;
        };
        for k in 0..a.frames {
            if r.sprites.get(i + k).map(|s| s.name.as_str()) != Some(&format!("{}_f{k}", a.name)) {
                return Err(SpriteError::Shape {
                    sprite: a.name.clone(),
                    reason: "フレームの名前が辞書順で連続しない".into(),
                });
            }
        }
    }
    Ok(())
}

/// `dir` と `atlas_path` を読んで生成し、`out` に 3 つのファイルを書く。
pub fn build(dir: &Path, atlas_path: &Path, out: &Path) -> Result<Generated, SpriteError> {
    let atlas_text = std::fs::read_to_string(atlas_path).map_err(|e| io_err(atlas_path, e))?;
    let strip = atlas::parse(&atlas_text)?;
    let generated = generate(&load_sources(dir)?, &strip)?;
    std::fs::create_dir_all(out).map_err(|e| io_err(out, e))?;
    let write = |name: &str, bytes: &[u8]| {
        let p = out.join(name);
        std::fs::write(&p, bytes).map_err(|e| io_err(&p, e))
    };
    write(&generated.png_name(), &generated.png)?;
    write("sprites.rs", generated.sprites_rs.as_bytes())?;
    write("palette.rs", generated.palette_rs.as_bytes())?;
    Ok(generated)
}

/// `compose` の 1 層。
#[derive(Debug, Clone)]
pub struct Layer {
    /// スプライトの名前。
    pub name: String,
    /// tint の色。
    pub tint: Option<[u8; 3]>,
    /// 基準点を置く位置(px)。
    pub at: (i32, i32),
}

/// 層を基準点で位置合わせして重ね、市松の地(設計の 16px)の上に、設計の 1px を `scale` 画素にして描く。
pub fn compose(generated: &Generated, layers: &[Layer], scale: u32) -> Result<Image, String> {
    let mut found = Vec::new();
    for l in layers {
        let s = generated
            .sprites
            .iter()
            .find(|s| s.name == l.name)
            .ok_or_else(|| format!("スプライト `{}` がない", l.name))?;
        let k = scale as i32;
        let at = ((l.at.0 - s.pivot.0) * k, (l.at.1 - s.pivot.1) * k);
        found.push((s.drawing.render(scale), l, at));
    }
    let x0 = found.iter().map(|f| f.2.0).min().unwrap_or(0);
    let y0 = found.iter().map(|f| f.2.1).min().unwrap_or(0);
    let x1 = found
        .iter()
        .map(|f| f.2.0 + f.0.w as i32)
        .max()
        .unwrap_or(1);
    let y1 = found
        .iter()
        .map(|f| f.2.1 + f.0.h as i32)
        .max()
        .unwrap_or(1);
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let tile = 16 * scale as i32;
    let mut out = Image::new(w, h);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let (wx, wy) = (x + x0, y + y0);
            let mut c: [u8; 3] = if (wx.div_euclid(tile) + wy.div_euclid(tile)) & 1 == 0 {
                [0xE8, 0xE2, 0xCC]
            } else {
                [0xDE, 0xD7, 0xBE]
            };
            for (img, l, o) in &found {
                let [r, g, b, a] = img.get(wx - o.0, wy - o.1);
                let t = l.tint.unwrap_or([255, 255, 255]);
                let src = [r, g, b].map(|v| v as u32);
                for k in 0..3 {
                    let v = src[k] * t[k] as u32 / 255;
                    c[k] = ((v * a as u32 + c[k] as u32 * (255 - a as u32)) / 255) as u8;
                }
            }
            out.set(x, y, [c[0], c[1], c[2], 255]);
        }
    }
    Ok(out)
}
