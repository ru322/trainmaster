//! TTF からグリフの帯の PNG と `glyphs.rs` を、`ja.toml` から `texts.rs` を作り、字を検査する開発用ツール。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod charset;
pub mod config;
pub mod emit;
pub mod error;
pub mod kanji;
pub mod pack;
pub mod proof;
pub mod raster;
pub mod report;
pub mod text;

pub use error::FontgenError;
pub use proof::{PREVIEW_BG, preview};

use charset::Source;
use config::{Config, Strip};
use emit::Image;
use kanji::AllowedKanji;
use raster::Face;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use text::Texts;

/// 読み込んだ入力(ファイルの中身そのまま)。
#[derive(Debug, Clone)]
pub struct Inputs {
    /// `fontgen.toml` の中身。
    pub config: Config,
    /// TTF のバイト列。
    pub font: Vec<u8>,
    /// `atlas.toml` の中身。
    pub atlas: String,
    /// `ja.toml` の中身。
    pub ja: String,
    /// `allowed_kanji.txt` の中身。
    pub allowed_kanji: String,
}

fn read_err(path: &Path, e: std::io::Error) -> FontgenError {
    FontgenError::Read {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

/// `config_path` を読み、そこに書かれたパスを `root` からの相対パスとして読む(FG-E01)。
pub fn load(config_path: &Path, root: &Path) -> Result<Inputs, FontgenError> {
    let text = std::fs::read_to_string(config_path).map_err(|e| read_err(config_path, e))?;
    let config = Config::parse(&text, &config_path.display().to_string())?;
    let read_text = |rel: &str| {
        let p = root.join(rel);
        std::fs::read_to_string(&p).map_err(|e| read_err(&p, e))
    };
    let font_path = root.join(&config.font);
    Ok(Inputs {
        font: std::fs::read(&font_path).map_err(|e| read_err(&font_path, e))?,
        atlas: read_text(&config.atlas)?,
        ja: read_text(&config.text.ja)?,
        allowed_kanji: read_text(&config.text.allowed_kanji)?,
        config,
    })
}

/// SHA-256 の 16 進(小文字)。
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// フォントのラスタライズの前までの検査の結果。
#[derive(Debug, Clone)]
pub struct Checked {
    /// フォントの帯。
    pub strip: Strip,
    /// 許可漢字。
    pub allowed: AllowedKanji,
    /// フォントに入れる字と出どころ。
    pub chars: BTreeMap<char, Source>,
    /// 検査を通った文言。
    pub texts: Texts,
    /// `texts.rs` の中身。
    pub texts_rs: String,
}

/// TTF を読まずにできる検査(FG-E01〜E09、E12〜E15)。
pub fn validate(inputs: &Inputs) -> Result<Checked, FontgenError> {
    let c = &inputs.config;
    let strip = config::parse_strip(&inputs.atlas, &c.strip_owner, &c.atlas)?;
    let actual = sha256_hex(&inputs.font);
    if actual != c.font_sha256.to_ascii_lowercase() {
        return Err(FontgenError::FontHash {
            expected: c.font_sha256.clone(),
            actual,
        });
    }
    let allowed = AllowedKanji::parse(&inputs.allowed_kanji)?;
    let charset = c.charset_chars("fontgen.toml")?;
    let numeric = charset::numeric_chars();
    charset::check_numeric(&charset, &allowed, &numeric)?;
    let units = charset::unit_words();
    let rules = text::Rules {
        file: &c.text.ja,
        charset: &charset,
        allowed: &allowed,
        unit_words: &units,
        warn_line_chars: c.text.warn_line_chars,
    };
    let texts = text::check_texts(&inputs.ja, &rules)?;
    let chars = charset::font_chars(&charset, &texts.kanji, &numeric);
    let texts_rs = text::texts_rs(&texts, &c.text.ja);
    Ok(Checked {
        strip,
        allowed,
        chars,
        texts,
        texts_rs,
    })
}

/// `generate` と同じ検査(FG-E01〜E15)をすべて行い、ファイルは書かない。`check` コマンド用。
pub fn check(inputs: &Inputs) -> Result<Checked, FontgenError> {
    generate(inputs).map(|g| g.checked)
}

/// 生成物。
#[derive(Debug, Clone)]
pub struct Generated {
    /// 検査の結果。
    pub checked: Checked,
    /// フォントの大きさごとのグリフ(`fontgen.toml` の順)。
    pub faces: Vec<Face>,
    /// グリフの通し番号の順の左上(ページの絶対座標)。
    pub positions: Vec<(u32, u32)>,
    /// 使った帯の行数。
    pub rows_used: u32,
    /// 帯の画像。
    pub image: Image,
    /// 帯の PNG。
    pub png: Vec<u8>,
    /// `glyphs.rs` の中身。
    pub glyphs_rs: String,
    /// 警告(1 行の字数と帯の使用率。終了コードは 0)。
    pub warnings: Vec<String>,
}

impl Generated {
    /// PNG のファイル名(`atlas_p<page>_s<strip>_<owner>.png`)。
    pub fn png_name(&self, owner: &str) -> String {
        let s = &self.checked.strip;
        format!("atlas_p{}_s{}_{owner}.png", s.page, s.index)
    }

    /// `id` のフォント。
    pub fn face(&self, id: &str) -> Option<&Face> {
        self.faces.iter().find(|f| f.def.id == id)
    }
}

/// 検査してから、帯の画像・PNG・`glyphs.rs`・`texts.rs` を作る。同じ入力から同じバイト列になる。
pub fn generate(inputs: &Inputs) -> Result<Generated, FontgenError> {
    let checked = validate(inputs)?;
    let c = &inputs.config;
    let mut faces = Vec::with_capacity(c.face.len());
    for (i, def) in c.face.iter().enumerate() {
        let font = raster::load_font(&inputs.font, def.size_px, &c.font)?;
        faces.push(raster::rasterize_face(&font, def, &checked.chars, &c.font)?);
        if i == 0 {
            raster::check_glyphs(&font, &charset::allowed_chars(&checked.allowed))?;
        }
    }
    let bitmaps: Vec<(usize, char, &raster::Bitmap)> = faces
        .iter()
        .enumerate()
        .flat_map(|(fi, f)| {
            f.glyphs
                .iter()
                .filter_map(move |g| g.bitmap.as_ref().map(|b| (fi, g.ch, b)))
        })
        .collect();
    let items: Vec<pack::Item> = bitmaps
        .iter()
        .map(|&(fi, ch, b)| pack::Item {
            w: b.w,
            h: b.h,
            size: u32::from(faces[fi].def.size_px),
            ch,
            face: fi,
        })
        .collect();
    let strip = checked.strip;
    let (positions, rows_used) = pack::pack(&items, &strip, c.padding)?;
    let mut image = Image::new(strip.width, strip.height, [0; 4]);
    for (&(_, _, b), &(x, y)) in bitmaps.iter().zip(&positions) {
        for by in 0..b.h {
            for bx in 0..b.w {
                let a = b.alpha[(by * b.w + bx) as usize];
                image.set(x + bx, y - strip.y + by, [255, 255, 255, a]);
            }
        }
    }
    let glyphs_rs = emit::glyphs_rs(&faces, &positions, &strip, &sha256_hex(&inputs.font));
    let mut warnings = checked.texts.warnings.clone();
    warnings.extend(strip_warning(rows_used, &strip, c.text.warn_strip_percent));
    Ok(Generated {
        png: emit::encode_png(&image)?,
        checked,
        faces,
        positions,
        rows_used,
        image,
        glyphs_rs,
        warnings,
    })
}

/// 帯の使用率が `percent` を超えたときの警告(15 §3.2)。
pub fn strip_warning(rows_used: u32, strip: &Strip, percent: u32) -> Option<String> {
    (u64::from(rows_used) * 100 > u64::from(strip.height) * u64::from(percent)).then(|| {
        format!(
            "フォントの帯の使用率が {percent}% を超えた({rows_used}/{} 行)。atlas.toml の帯の配分を見直す",
            strip.height
        )
    })
}

/// 生成して `out` に PNG・`glyphs.rs`・`texts.rs` を書く。
pub fn build(inputs: &Inputs, out: &Path) -> Result<Generated, FontgenError> {
    let g = generate(inputs)?;
    std::fs::create_dir_all(out).map_err(|e| read_err(out, e))?;
    let write = |name: &str, bytes: &[u8]| {
        let p = out.join(name);
        std::fs::write(&p, bytes).map_err(|e| read_err(&p, e))
    };
    write(&g.png_name(&inputs.config.strip_owner), &g.png)?;
    write("glyphs.rs", g.glyphs_rs.as_bytes())?;
    write("texts.rs", g.checked.texts_rs.as_bytes())?;
    Ok(g)
}
