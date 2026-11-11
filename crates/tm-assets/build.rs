//! 生成した PNG を復号し、同じページの帯を縦につなげて生の RGBA にする [15 §3.5]。

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

const MAX_PAGES: usize = 2;
const MAX_SIZE: u32 = 1024;

struct Strip {
    page: u8,
    index: u8,
    owner: String,
    path: PathBuf,
}

struct Decoded {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn parse_name(name: &str) -> Option<(u8, u8, String)> {
    let rest = name.strip_prefix("atlas_p")?.strip_suffix(".png")?;
    let (page, rest) = rest.split_once("_s")?;
    let (index, owner) = rest.split_once('_')?;
    if owner.is_empty() {
        return None;
    }
    Some((page.parse().ok()?, index.parse().ok()?, owner.to_string()))
}

fn list_strips(dir: &Path) -> Vec<Strip> {
    let mut strips = Vec::new();
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("{} を読めない: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("ディレクトリの列挙").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some((page, index, owner)) = parse_name(name) {
            strips.push(Strip {
                page,
                index,
                owner,
                path,
            });
        }
    }
    strips.sort_by_key(|s| (s.page, s.index));
    strips
}

fn decode(path: &Path) -> Decoded {
    let file = File::open(path).unwrap_or_else(|e| panic!("{} を開けない: {e}", path.display()));
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|e| panic!("{} のヘッダが不正: {e}", path.display()));
    let size = reader
        .output_buffer_size()
        .unwrap_or_else(|| panic!("{} が大きすぎる", path.display()));
    let mut rgba = vec![0; size];
    let info = reader
        .next_frame(&mut rgba)
        .unwrap_or_else(|e| panic!("{} を復号できない: {e}", path.display()));
    assert!(
        info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight,
        "{} は 8 ビット RGBA でない: {:?} {:?}",
        path.display(),
        info.color_type,
        info.bit_depth
    );
    rgba.truncate(info.buffer_size());
    Decoded {
        width: info.width,
        height: info.height,
        rgba,
    }
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let generated = manifest.join("generated");
    println!("cargo:rerun-if-changed={}", generated.display());

    let strips = list_strips(&generated);
    let mut pages: BTreeMap<u8, Vec<&Strip>> = BTreeMap::new();
    for strip in &strips {
        println!("cargo:rerun-if-changed={}", strip.path.display());
        pages.entry(strip.page).or_default().push(strip);
    }
    assert!(
        !pages.is_empty() && pages.len() <= MAX_PAGES,
        "アトラスの PNG が {} ページ分ある(1〜{MAX_PAGES} が必要)。`mise run assets:gen` で生成する",
        pages.len()
    );

    let mut dims = String::new();
    let mut rgba_refs = String::new();
    let mut strip_dims = String::new();
    for (expected, (&page, list)) in pages.iter().enumerate() {
        assert_eq!(
            usize::from(page),
            expected,
            "ページ番号が 0 から連続していない"
        );
        let mut width = None;
        let mut y = 0u32;
        let mut rgba = Vec::new();
        for (i, strip) in list.iter().enumerate() {
            assert_eq!(
                usize::from(strip.index),
                i,
                "p{page} の帯の番号が 0 から連続していない"
            );
            let image = decode(&strip.path);
            let w = *width.get_or_insert(image.width);
            assert_eq!(
                image.width,
                w,
                "{} の幅が帯でそろっていない",
                strip.path.display()
            );
            writeln!(
                strip_dims,
                "    ({:?}, StripInfo {{ page: {page}, y: {y}, height: {} }}),",
                strip.owner, image.height
            )
            .expect("String への書き込み");
            y += image.height;
            rgba.extend_from_slice(&image.rgba);
        }
        let width = width.expect("帯が 1 つ以上ある");
        assert!(
            width <= MAX_SIZE && y <= MAX_SIZE,
            "p{page} が {width}x{y} で {MAX_SIZE} を超える"
        );
        fs::write(out.join(format!("atlas_p{page}.rgba")), &rgba).expect("RGBA の書き込み");
        writeln!(dims, "    ({width}, {y}),").expect("String への書き込み");
        writeln!(
            rgba_refs,
            "    include_bytes!(concat!(env!(\"OUT_DIR\"), \"/atlas_p{page}.rgba\")),"
        )
        .expect("String への書き込み");
    }

    let source = format!(
        "/// アトラスのページ数。\n\
         pub const PAGE_COUNT: usize = {};\n\
         /// ページごとの (幅, 高さ)。\n\
         pub const PAGE_DIMS: [(u32, u32); PAGE_COUNT] = [\n{dims}];\n\
         /// ページごとの生の RGBA。\n\
         pub static PAGE_RGBA: [&[u8]; PAGE_COUNT] = [\n{rgba_refs}];\n\
         /// 連結した帯の (持ち主, 位置)。ツールが書いた帯の位置との照合に使う。\n\
         pub static STRIP_DIMS: [(&str, StripInfo); {}] = [\n{strip_dims}];\n",
        pages.len(),
        strips.len()
    );
    fs::write(out.join("pages.rs"), source).expect("pages.rs の書き込み");
}
