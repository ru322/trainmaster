//! dummy ドライバで描いた絵を `SoftRaster` と比べるテスト(14 §9.4)と、その補助。

mod errors;
mod events;
mod render;
mod ui_screens;

use std::cell::RefCell;
use std::sync::{Mutex, Once};

use tm_platform_api::{
    AssetBundle, AtlasImage, Color, DrawCmd, DrawList, Rect, SoftRaster, SpriteId, SpriteRect,
};

use crate::config::{BatchMode, DisplayMode, SdlConfig};
use crate::core::SdlCore;
use crate::platform::SdlPlatform;

/// SDL の初期化は複数のスレッドから同時にできないので、SDL を使うテストを 1 つずつ動かす。
static SDL_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static LOGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct Capture;

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, r: &log::Record<'_>) {
        LOGS.with(|l| l.borrow_mut().push(r.args().to_string()));
    }
    fn flush(&self) {}
}

static CAPTURE: Capture = Capture;

/// このスレッドで出たログを取り出す。
fn take_logs() -> Vec<String> {
    LOGS.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

fn count_logs(logs: &[String], needle: &str) -> usize {
    logs.iter().filter(|l| l.contains(needle)).count()
}

fn config(batch: BatchMode) -> SdlConfig {
    let mut cfg = SdlConfig::dev_default();
    cfg.display = DisplayMode::Windowed { scale: 1 };
    cfg.batch = batch;
    cfg.vsync = false;
    cfg.audio.enabled = false;
    cfg.dev_hotkeys = false;
    cfg
}

pub(crate) fn sdl_lock() -> std::sync::MutexGuard<'static, ()> {
    SDL_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn init_dummy_sdl() {
    static LOGGER: Once = Once::new();
    LOGGER.call_once(|| {
        if log::set_logger(&CAPTURE).is_ok() {
            log::set_max_level(log::LevelFilter::Trace);
        }
    });
    sdl2::hint::set("SDL_VIDEODRIVER", "dummy");
    sdl2::hint::set("SDL_AUDIODRIVER", "dummy");
}

/// PL-V-15: `SdlCore::new` がウィンドウの大きさと出力の大きさをログに出す。
#[test]
fn core_logs_window_and_output_size() {
    let _guard = sdl_lock();
    init_dummy_sdl();
    take_logs();
    let mut cfg = config(BatchMode::Geometry);
    cfg.display = DisplayMode::Windowed { scale: 2 };
    let core = SdlCore::new(&cfg).expect("SdlCore::new");
    let logs = take_logs();
    let line = logs
        .iter()
        .find(|l| l.starts_with("SDL "))
        .expect("{logs:?}");
    assert!(line.contains("ウィンドウ 1280×720"), "{line}");
    assert!(line.contains("出力 1280×720"), "{line}");
    assert!(line.contains("Windowed { scale: 2 }"), "{line}");
    drop(core);
}

/// SDL を 1 つずつ使い、`assets` を読み込んだプラットフォームで `f` を動かす。
fn with_platform(cfg: &SdlConfig, assets: &AssetBundle, f: impl FnOnce(&mut SdlPlatform<'_>)) {
    let _guard = sdl_lock();
    init_dummy_sdl();
    let core = SdlCore::new(cfg).expect("SdlCore::new");
    let tc = core.texture_creator();
    let mut p = SdlPlatform::new(core, &tc, cfg).expect("SdlPlatform::new");
    tm_platform_api::Platform::load_assets(&mut p, assets).expect("load_assets");
    take_logs();
    f(&mut p);
}

/// SDL と `SoftRaster` の差の許容。
#[derive(Clone, Copy, Debug)]
struct Tolerance {
    /// 1 チャンネルの差の上限。
    max: u8,
    /// ±2 を超えてよい画素の割合(千分率)。
    over2_permille: usize,
}

/// 合成の絵(半透明を重ねない)。SDL2 のソフトウェアの描画は色を掛けない等倍の転送を `>> 8` で近似し、
/// ブレンド 1 回で最大 2 ずれる。半透明を重ねると 1 回ごとに最大 2 ずつ増える。
const EXACT: Tolerance = Tolerance {
    max: 2,
    over2_permille: 0,
};

/// 実際の UI の画面(UI-V-08)。重なりの深さ 4 まで(±8)、±2 を超える画素は 1% まで。
const UI_SCREEN: Tolerance = Tolerance {
    max: 8,
    over2_permille: 10,
};

/// 描画の 4 通り: (バッチの方式, 直接描画モードか)。
const MODES: [(BatchMode, bool); 4] = [
    (BatchMode::Geometry, false),
    (BatchMode::Copy, false),
    (BatchMode::Geometry, true),
    (BatchMode::Copy, true),
];

/// `list` をその方式で描いて読んだ画素。
fn sdl_frame(p: &mut SdlPlatform<'_>, list: &DrawList) -> Vec<u8> {
    let r = p.draw_only(list);
    assert_eq!(r.error, None);
    p.capture_frame().expect("capture_frame")
}

fn soft_frame(assets: &AssetBundle, list: &DrawList) -> Vec<u8> {
    let mut s = SoftRaster::new();
    s.draw(assets, list);
    s.pixels().to_vec()
}

/// `EXACT` の許容で同じか。直接描画モードのウィンドウはアルファを持たないので RGB だけ比べる。
fn assert_same(got: &[u8], want: &[u8], rgb_only: bool, what: &str) {
    assert_close(got, want, rgb_only, what, EXACT);
}

fn assert_close(got: &[u8], want: &[u8], rgb_only: bool, what: &str, tol: Tolerance) {
    assert_eq!(got.len(), want.len(), "{what}");
    let n = if rgb_only { 3 } else { 4 };
    let diff = |g: &[u8], w: &[u8]| (0..n).map(|k| g[k].abs_diff(w[k])).max().unwrap_or(0);
    let mut bad = Vec::new();
    let (mut over2, mut max) = (0, 0);
    for (i, (g, w)) in got.chunks(4).zip(want.chunks(4)).enumerate() {
        let d = diff(g, w);
        max = max.max(d);
        over2 += usize::from(d > 2);
        if d > tol.max {
            bad.push((
                i % 640,
                i / 640,
                d,
                [g[0], g[1], g[2], g[3]],
                [w[0], w[1], w[2], w[3]],
            ));
        }
    }
    let pixels = got.len() / 4;
    println!("{what}: 最大差 {max}、±2 超 {over2}/{pixels}");
    let ok = bad.is_empty() && over2 * 1000 <= tol.over2_permille * pixels;
    if !ok {
        dump(&format!("{what}-got"), got);
        dump(&format!("{what}-want"), want);
    }
    assert!(
        ok,
        "{what}: ±{} を超える画素 {}、±2 を超える画素 {over2}/{pixels}。最初の 5 つ (x, y, 差, SDL, SoftRaster): {:?}",
        tol.max,
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

/// `TM_SDL_DUMP_DIR` があれば、画素を PNG に書く(目での確認用)。
fn dump(name: &str, rgba: &[u8]) {
    if let Ok(dir) = std::env::var("TM_SDL_DUMP_DIR") {
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = std::path::Path::new(&dir).join(format!("{safe}.png"));
        std::fs::write(&path, png(640, 360, rgba)).expect("PNG を書けない");
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// 無圧縮の deflate で RGBA の PNG を作る(依存を足さないため)。
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in rgba.chunks(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &v in &raw {
        a = (a + u32::from(v)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    for (kind, data) in [(&b"IHDR"[..], &ihdr[..]), (b"IDAT", &z), (b"IEND", &[])] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    out
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

fn fill(x: i32, y: i32, w: u32, h: u32, color: Color) -> DrawCmd {
    DrawCmd::FillRect {
        rect: rect(x, y, w, h),
        color,
    }
}

fn sprite(id: u16, dst: Rect, tint: Color) -> DrawCmd {
    DrawCmd::Sprite {
        id: SpriteId(id),
        dst,
        tint,
    }
}

/// 16×16 のアトラス: 白 2×2(0)、5×1 の色の帯(1)、半透明の 1 画素(2)、アルファがまちまちの 6×5 の模様(3)、
/// 16×4 の文字風の白(4)、アルファ 1〜254 の淡い 4×1(5)。
fn test_assets() -> &'static AssetBundle {
    static BUNDLE: std::sync::OnceLock<AssetBundle> = std::sync::OnceLock::new();
    BUNDLE.get_or_init(|| {
        let mut px = vec![0u8; 16 * 16 * 4];
        let mut put = |x: usize, y: usize, c: [u8; 4]| {
            px[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4].copy_from_slice(&c)
        };
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(x, y, [255; 4]);
        }
        let strip = [
            [200, 10, 10, 255],
            [10, 200, 10, 255],
            [10, 10, 200, 255],
            [200, 200, 10, 255],
            [10, 200, 200, 255],
        ];
        for (i, c) in strip.into_iter().enumerate() {
            put(2 + i, 0, c);
        }
        put(7, 0, [200, 100, 50, 128]);
        for y in 0..5 {
            for x in 0..6 {
                let v = (x * 41 + y * 67) as u8;
                put(
                    8 + x,
                    2 + y,
                    [v, 255 - v, v / 2 + 60, (x * 50 + y * 13) as u8],
                );
            }
        }
        for y in 0..4 {
            for x in 0..16 {
                let a = if (x + y) % 3 == 0 { 255 } else { 0 };
                put(x, 8 + y, [255, 255, 255, a]);
            }
        }
        for (i, a) in [1, 2, 128, 254].into_iter().enumerate() {
            put(i, 12, [0, 255, 10, a]);
        }
        let atlases: &'static [AtlasImage] = Box::leak(Box::new([AtlasImage {
            width: 16,
            height: 16,
            rgba: Box::leak(px.into_boxed_slice()),
        }]));
        let sprite = |x, y, w, h| SpriteRect {
            atlas: 0,
            x,
            y,
            w,
            h,
        };
        let sprites: &'static [SpriteRect] = Box::leak(Box::new([
            sprite(0, 0, 2, 2),
            sprite(2, 0, 5, 1),
            sprite(7, 0, 1, 1),
            sprite(8, 2, 6, 5),
            sprite(0, 8, 16, 4),
            sprite(0, 12, 4, 1),
        ]));
        AssetBundle {
            atlases,
            sprites,
            sounds: &[],
            white: SpriteId(0),
        }
    })
}
