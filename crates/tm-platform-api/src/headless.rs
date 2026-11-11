//! SDL を使わない `Platform` の実装、参照用のソフトウェアラスタライザ、テスト用の最小の `AssetBundle` [14 §2.2、§2.3]。

use std::collections::BTreeMap;

use crate::assets::{AssetBundle, AtlasImage, SpriteRect};
use crate::audio::{AudioCmd, SoundData, SoundKind};
use crate::draw::{DrawCmd, DrawList, SpriteId};
use crate::geom::{Color, LOGICAL_HEIGHT, LOGICAL_WIDTH, Rect};
use crate::input::{Key, PhysicalInput};
use crate::platform::{Platform, PlatformError};
use crate::time::MonoTime;

/// 時計の進め方。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HeadlessClock {
    /// 1 フレーム = 1/60 秒。`now = frame × 1_000_000 / 60`(整数。ずれがたまらない)。
    #[default]
    Exact60,
    /// 1 フレーム = 固定のマイクロ秒(処理落ちの再現用)。
    FixedUs(u64),
}

/// `DrawList` をどれだけ覚えておくか。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RecordPolicy {
    /// 覚えない。
    None,
    /// 直近の 1 フレームだけ。
    #[default]
    LastOnly,
    /// 先頭から `max_frames` フレームまで。直近の 1 フレームは常に `last_frame` で読める。
    All {
        /// 覚える最大のフレーム数。
        max_frames: u32,
    },
}

/// `HeadlessPlatform` の設定。既定は `Exact60`、`LastOnly`、`check_audio = true`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadlessConfig {
    /// 時計の進め方。
    pub clock: HeadlessClock,
    /// `DrawList` の記録の方針。
    pub record: RecordPolicy,
    /// この回数の `present` の後、`poll_input` が `QuitRequested` を返す。
    pub quit_after_frames: Option<u64>,
    /// このフレーム番号の `present` で `Err(Render)` を返す(故障の注入)。
    pub fail_present_at: Option<u64>,
    /// `true` なら `AudioCmd` を検査する。ID の検査は `load_assets` の成功後だけ。音量の検査は常に行う。
    pub check_audio: bool,
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            clock: HeadlessClock::default(),
            record: RecordPolicy::default(),
            quit_after_frames: None,
            fail_present_at: None,
            check_audio: true,
        }
    }
}

/// 音の命令の契約違反。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioViolation {
    /// `SoundId` が表の範囲外。
    UnknownId {
        /// 命令を受け取ったフレーム番号。
        frame: u64,
        /// 命令。
        cmd: AudioCmd,
    },
    /// 種類の違う ID(`PlaySe` に `Bgm` の ID など)。
    WrongKind {
        /// 命令を受け取ったフレーム番号。
        frame: u64,
        /// 命令。
        cmd: AudioCmd,
    },
    /// `SetVolume` の `level` が 10 を超える。
    VolumeOutOfRange {
        /// 命令を受け取ったフレーム番号。
        frame: u64,
        /// 指定された値。
        level: u8,
    },
}

/// `load_assets` が受け取った束の大きさ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssetSummary {
    /// アトラスの枚数。
    pub atlases: u32,
    /// スプライトの数。
    pub sprites: u32,
    /// 音の数。
    pub sounds: u32,
}

/// 入力の注入、手動の時計、`DrawList` と `AudioCmd` の記録を持つ `Platform` の実装。
/// 検査で失敗した `load_assets` は 1 回目に数えず、呼び直せる。
#[derive(Debug)]
pub struct HeadlessPlatform {
    cfg: HeadlessConfig,
    frame: u64,
    extra_us: u64,
    scheduled: BTreeMap<u64, Vec<PhysicalInput>>,
    last: Option<DrawList>,
    frames: Vec<DrawList>,
    audio_log: Vec<(u64, AudioCmd)>,
    violations: Vec<AudioViolation>,
    sounds: &'static [SoundData],
    summary: Option<AssetSummary>,
}

impl HeadlessPlatform {
    /// 設定から作る。
    pub fn new(cfg: HeadlessConfig) -> Self {
        Self {
            cfg,
            frame: 0,
            extra_us: 0,
            scheduled: BTreeMap::new(),
            last: None,
            frames: Vec::new(),
            audio_log: Vec::new(),
            violations: Vec::new(),
            sounds: &[],
            summary: None,
        }
    }

    /// フレーム番号 `frame` の `poll_input` で `input` を返す。同じフレームでは登録順。
    pub fn schedule(&mut self, frame: u64, input: PhysicalInput) {
        self.scheduled.entry(frame).or_default().push(input);
    }

    /// `KeyDown` を `frame` に、`KeyUp` を `frame + hold_frames` に予定する。
    pub fn tap(&mut self, frame: u64, key: Key, hold_frames: u32) {
        self.schedule(frame, PhysicalInput::KeyDown(key));
        self.schedule(frame + u64::from(hold_frames), PhysicalInput::KeyUp(key));
    }

    /// 時計だけを進める(処理落ちや長い停止の再現)。次の `now` から効く。
    pub fn advance_clock_us(&mut self, us: u64) {
        self.extra_us = self.extra_us.saturating_add(us);
    }

    /// `present` を呼んだ回数。
    pub fn frame_index(&self) -> u64 {
        self.frame
    }

    /// 直近に描いた `DrawList`。`RecordPolicy::None` では常に `None`。
    pub fn last_frame(&self) -> Option<&DrawList> {
        self.last.as_ref()
    }

    /// 記録した `DrawList`。`RecordPolicy::All` のときだけ入る。
    pub fn frames(&self) -> &[DrawList] {
        &self.frames
    }

    /// 受け取った `AudioCmd` と、そのフレーム番号。
    pub fn audio_log(&self) -> &[(u64, AudioCmd)] {
        &self.audio_log
    }

    /// `check_audio` で見つけた契約違反。
    pub fn audio_violations(&self) -> &[AudioViolation] {
        &self.violations
    }

    /// `load_assets` に成功した束の大きさ。
    pub fn assets_summary(&self) -> Option<AssetSummary> {
        self.summary
    }

    fn check_cmd(&mut self, cmd: AudioCmd) {
        let frame = self.frame;
        let target = match cmd {
            AudioCmd::PlaySe(id) | AudioCmd::StopSe(id) => Some((id, SoundKind::Se)),
            AudioCmd::PlayBgm { id, .. } => Some((id, SoundKind::Bgm)),
            AudioCmd::PlayJingle(id) => Some((id, SoundKind::Jingle)),
            AudioCmd::StopBgm { .. } | AudioCmd::SetBgmDuck(_) | AudioCmd::SetVolume { .. } => None,
        };
        if let (Some((id, want)), Some(_)) = (target, self.summary) {
            match self.sounds.get(usize::from(id.0)) {
                None => self
                    .violations
                    .push(AudioViolation::UnknownId { frame, cmd }),
                Some(sound) if sound.kind != want => {
                    self.violations
                        .push(AudioViolation::WrongKind { frame, cmd });
                }
                Some(_) => {}
            }
        }
        if let AudioCmd::SetVolume { level, .. } = cmd
            && level > 10
        {
            self.violations
                .push(AudioViolation::VolumeOutOfRange { frame, level });
        }
    }

    fn record(&mut self, list: &DrawList) {
        match self.cfg.record {
            RecordPolicy::None => return,
            RecordPolicy::LastOnly => {}
            RecordPolicy::All { max_frames } => {
                if self.frames.len() < max_frames as usize {
                    self.frames.push(list.clone());
                }
            }
        }
        match &mut self.last {
            Some(last) => last.clone_from(list),
            None => self.last = Some(list.clone()),
        }
    }
}

impl Platform for HeadlessPlatform {
    fn now(&self) -> MonoTime {
        let elapsed = match self.cfg.clock {
            HeadlessClock::Exact60 => self.frame.saturating_mul(1_000_000) / 60,
            HeadlessClock::FixedUs(us) => self.frame.saturating_mul(us),
        };
        MonoTime(elapsed.saturating_add(self.extra_us))
    }

    fn poll_input(&mut self, out: &mut Vec<PhysicalInput>) {
        while let Some(entry) = self.scheduled.first_entry() {
            if *entry.key() > self.frame {
                break;
            }
            out.extend(entry.remove());
        }
        if self.cfg.quit_after_frames.is_some_and(|n| self.frame >= n) {
            out.push(PhysicalInput::QuitRequested);
        }
    }

    fn load_assets(&mut self, assets: &AssetBundle) -> Result<(), PlatformError> {
        if self.summary.is_some() {
            return Err(PlatformError::Assets(
                "load_assets は 1 回だけ呼べる".to_owned(),
            ));
        }
        assets
            .validate()
            .map_err(|e| PlatformError::Assets(format!("{e:?}")))?;
        self.sounds = assets.sounds;
        self.summary = Some(AssetSummary {
            atlases: u32::try_from(assets.atlases.len()).unwrap_or(u32::MAX),
            sprites: u32::try_from(assets.sprites.len()).unwrap_or(u32::MAX),
            sounds: u32::try_from(assets.sounds.len()).unwrap_or(u32::MAX),
        });
        Ok(())
    }

    fn present(&mut self, frame: &DrawList) -> Result<(), PlatformError> {
        let index = self.frame;
        self.frame += 1;
        if self.cfg.fail_present_at == Some(index) {
            return Err(PlatformError::Render(format!(
                "注入した故障(フレーム {index})"
            )));
        }
        self.record(frame);
        Ok(())
    }

    fn audio(&mut self, cmds: &[AudioCmd]) {
        for &cmd in cmds {
            self.audio_log.push((self.frame, cmd));
            if self.cfg.check_audio {
                self.check_cmd(cmd);
            }
        }
    }

    fn logical_size(&self) -> (u32, u32) {
        (LOGICAL_WIDTH, LOGICAL_HEIGHT)
    }
}

const LAYERS: usize = 4;
const W: i64 = LOGICAL_WIDTH as i64;
const H: i64 = LOGICAL_HEIGHT as i64;
const OPAQUE_BLACK: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};

#[derive(Clone, Copy)]
struct Area {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
}

impl Area {
    fn of(r: Rect) -> Area {
        let (x, y) = (i64::from(r.x), i64::from(r.y));
        Area {
            x0: x,
            y0: y,
            x1: x + i64::from(r.w),
            y1: y + i64::from(r.h),
        }
    }

    fn meet(self, o: Area) -> Area {
        Area {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        }
    }
}

const SCREEN: Area = Area {
    x0: 0,
    y0: 0,
    x1: W,
    y1: H,
};

/// `begin` の `BeginCachedLayer` の区間の終わり(深さが 0 に戻る End、なければ列の最後)と、入れ子のない閉じた区間か(14 §3.5.5)。
fn layer_interval(cmds: &[DrawCmd], begin: usize) -> (usize, bool) {
    let mut depth = 1u32;
    let mut nested = false;
    for (j, cmd) in cmds.iter().enumerate().skip(begin + 1) {
        match cmd {
            DrawCmd::BeginCachedLayer { .. } => {
                depth += 1;
                nested = true;
            }
            DrawCmd::EndCachedLayer => {
                depth -= 1;
                if depth == 0 {
                    return (j, !nested);
                }
            }
            _ => {}
        }
    }
    (cmds.len() - 1, false)
}

/// 層ごとの、このフレームの最初の正しい区間(Begin の添字、End の添字、置く矩形)。
fn layer_sections(cmds: &[DrawCmd]) -> [Option<(usize, usize, Rect)>; LAYERS] {
    let mut sections = [None; LAYERS];
    let mut i = 0;
    while i < cmds.len() {
        if let DrawCmd::BeginCachedLayer {
            layer, x, y, w, h, ..
        } = cmds[i]
        {
            let (end, ok) = layer_interval(cmds, i);
            if ok && let Some(slot @ None) = sections.get_mut(usize::from(layer.0)) {
                *slot = Some((i, end, Rect { x, y, w, h }));
            }
            i = end;
        }
        i += 1;
    }
    sections
}

/// `DrawList` を 640×360 の RGBA(ストレートアルファ、行優先)に描く参照用のラスタライザ [14 §2.3]。
/// 整数だけで計算し、キャッシュ層は常にその場描きする(14 §3.5.3)。拡大縮小は SDL2 のソフトウェアの描画と同じ 16.16 の固定小数点の最近傍。
/// SDL の描画との一致を保証するのは、スプライトの座標の絶対値が 2^24 未満で、幅と高さが `i32::MAX / 2` 以下のときだけ。
#[derive(Clone, Debug)]
pub struct SoftRaster {
    px: Vec<u8>,
}

impl Default for SoftRaster {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftRaster {
    /// 不透明の黒で埋めた画面を作る。
    pub fn new() -> Self {
        let mut s = SoftRaster {
            px: vec![0; (W * H * 4) as usize],
        };
        s.set_all(OPAQUE_BLACK);
        s
    }

    /// 画面を不透明の黒で初期化してから `list` を描く(14 §3.3.1、§3.4)。
    pub fn draw(&mut self, assets: &AssetBundle, list: &DrawList) {
        self.set_all(OPAQUE_BLACK);
        let cmds = &list.cmds;
        let sections = layer_sections(cmds);
        let mut i = 0;
        while i < cmds.len() {
            match cmds[i] {
                DrawCmd::BeginCachedLayer { .. } => i = layer_interval(cmds, i).0,
                DrawCmd::EndCachedLayer => {}
                DrawCmd::DrawCachedLayer { layer } => {
                    if let Some(Some((begin, end, rect))) = sections.get(usize::from(layer.0))
                        && rect.w > 0
                        && rect.h > 0
                    {
                        self.inline(assets, &cmds[begin + 1..*end], *rect);
                    }
                }
                DrawCmd::Clear(c) => self.set_all(c),
                cmd => self.cmd(assets, cmd, (0, 0), SCREEN),
            }
            i += 1;
        }
    }

    /// 画素(RGBA、640×360、行優先)。
    pub fn pixels(&self) -> &[u8] {
        &self.px
    }

    fn set_all(&mut self, c: Color) {
        for p in self.px.as_chunks_mut::<4>().0 {
            *p = [c.r, c.g, c.b, c.a];
        }
    }

    fn inline(&mut self, assets: &AssetBundle, content: &[DrawCmd], place: Rect) {
        let clip = Area::of(place).meet(SCREEN);
        for &cmd in content {
            let cmd = match cmd {
                DrawCmd::Clear(color) => DrawCmd::FillRect {
                    rect: Rect {
                        x: 0,
                        y: 0,
                        w: place.w,
                        h: place.h,
                    },
                    color,
                },
                other => other,
            };
            self.cmd(assets, cmd, (i64::from(place.x), i64::from(place.y)), clip);
        }
    }

    fn cmd(&mut self, assets: &AssetBundle, cmd: DrawCmd, shift: (i64, i64), clip: Area) {
        match cmd {
            DrawCmd::FillRect { rect, color } => self.fill(Area::of(rect), shift, clip, color),
            DrawCmd::Line {
                x0,
                y0,
                x1,
                y1,
                color,
            } => {
                let from = (i64::from(x0) + shift.0, i64::from(y0) + shift.1);
                let to = (i64::from(x1) + shift.0, i64::from(y1) + shift.1);
                for (x, y) in line_pixels(from, to, clip) {
                    let a = Area {
                        x0: x,
                        y0: y,
                        x1: x + 1,
                        y1: y + 1,
                    };
                    self.fill(a, (0, 0), clip, color);
                }
            }
            DrawCmd::Sprite { id, dst, tint } => self.sprite(assets, id, dst, shift, clip, tint),
            DrawCmd::Clear(_)
            | DrawCmd::BeginCachedLayer { .. }
            | DrawCmd::EndCachedLayer
            | DrawCmd::DrawCachedLayer { .. } => {}
        }
    }

    fn fill(&mut self, a: Area, shift: (i64, i64), clip: Area, c: Color) {
        let a = Area {
            x0: a.x0 + shift.0,
            y0: a.y0 + shift.1,
            x1: a.x1 + shift.0,
            y1: a.y1 + shift.1,
        }
        .meet(clip);
        for y in a.y0..a.y1 {
            for x in a.x0..a.x1 {
                self.blend(x, y, [c.r, c.g, c.b], c.a);
            }
        }
    }

    fn sprite(
        &mut self,
        assets: &AssetBundle,
        id: SpriteId,
        dst: Rect,
        shift: (i64, i64),
        clip: Area,
        tint: Color,
    ) {
        let Some(src) = assets.sprites.get(usize::from(id.0)) else {
            return;
        };
        let Some(page) = assets.atlases.get(usize::from(src.atlas)) else {
            return;
        };
        if dst.w == 0 || dst.h == 0 {
            return;
        }
        let (dx, dy) = (i64::from(dst.x) + shift.0, i64::from(dst.y) + shift.1);
        let (dw, dh) = (i64::from(dst.w), i64::from(dst.h));
        let (sw, sh) = (i64::from(src.w), i64::from(src.h));
        let a = Area {
            x0: dx,
            y0: dy,
            x1: dx + dw,
            y1: dy + dh,
        }
        .meet(clip);
        let pitch = i64::from(page.width);
        let (inc_x, inc_y) = ((sw << 16) / dw, (sh << 16) / dh);
        for y in a.y0..a.y1 {
            let sy = i64::from(src.y) + ((inc_y / 2 + (y - dy) * inc_y) >> 16);
            for x in a.x0..a.x1 {
                let sx = i64::from(src.x) + ((inc_x / 2 + (x - dx) * inc_x) >> 16);
                let i = ((sy * pitch + sx) * 4) as usize;
                let t = &page.rgba[i..i + 4];
                let m = |v: u8, k: u8| (u32::from(v) * u32::from(k) / 255) as u8;
                self.blend(
                    x,
                    y,
                    [m(t[0], tint.r), m(t[1], tint.g), m(t[2], tint.b)],
                    m(t[3], tint.a),
                );
            }
        }
    }

    /// `dst = src × a / 255 + dst × (255 − a) / 255`(項ごとに切り捨て。SDL2 のソフトウェアの矩形と拡大の描画と同じ丸め)。
    fn blend(&mut self, x: i64, y: i64, src: [u8; 3], a: u8) {
        let i = ((y * W + x) * 4) as usize;
        let p = &mut self.px[i..i + 4];
        let (a, inv) = (u32::from(a), 255 - u32::from(a));
        for k in 0..3 {
            p[k] = (u32::from(src[k]) * a / 255 + u32::from(p[k]) * inv / 255) as u8;
        }
        p[3] = (a + u32::from(p[3]) * inv / 255) as u8;
    }
}

/// 両端を含むブレゼンハムの線の画素のうち、`clip` の中のもの(14 §3.4)。主軸の見える範囲だけを数える。
/// 主軸の i 番目の画素の副軸は `⌊(2·i·副 + 主) / (2·主)⌋`。
fn line_pixels(from: (i64, i64), to: (i64, i64), clip: Area) -> impl Iterator<Item = (i64, i64)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let x_major = dx.abs() >= dy.abs();
    let (m0, k0, dm, dk, lo, hi) = if x_major {
        (from.0, from.1, dx, dy, clip.x0, clip.x1)
    } else {
        (from.1, from.0, dy, dx, clip.y0, clip.y1)
    };
    let (n, amin) = (dm.abs(), dk.abs());
    let (sm, sk) = (if dm < 0 { -1 } else { 1 }, if dk < 0 { -1 } else { 1 });
    let (first, last) = if sm > 0 {
        (lo - m0, hi - 1 - m0)
    } else {
        (m0 - hi + 1, m0 - lo)
    };
    (first.max(0)..=last.min(n)).map(move |i| {
        let q = if n == 0 {
            0
        } else {
            ((2 * i128::from(i) * i128::from(amin) + i128::from(n)) / (2 * i128::from(n))) as i64
        };
        let (m, k) = (m0 + sm * i, k0 + sk * q);
        if x_major { (m, k) } else { (k, m) }
    })
}

static PIXELS: [u8; 4 * 4 * 4] = [0xFF; 4 * 4 * 4];
static SILENCE: [i16; 16] = [0; 16];
static ATLASES: [AtlasImage; 1] = [AtlasImage {
    width: 4,
    height: 4,
    rgba: &PIXELS,
}];
static SPRITES: [SpriteRect; 2] = [
    SpriteRect {
        atlas: 0,
        x: 0,
        y: 0,
        w: 2,
        h: 2,
    },
    SpriteRect {
        atlas: 0,
        x: 2,
        y: 0,
        w: 2,
        h: 2,
    },
];
static SOUNDS: [SoundData; 2] = [
    SoundData {
        name: "bgm_test",
        kind: SoundKind::Bgm,
        samples: &SILENCE,
        loop_start: Some(0),
    },
    SoundData {
        name: "se_test",
        kind: SoundKind::Se,
        samples: &SILENCE,
        loop_start: None,
    },
];
static MINIMAL: AssetBundle = AssetBundle {
    atlases: &ATLASES,
    sprites: &SPRITES,
    sounds: &SOUNDS,
    white: SpriteId(0),
};

impl AssetBundle {
    /// `tm-assets` に依存しない最小の束。4 × 4 のページ 1 枚、スプライト 2 個、無音の `bgm_test`(`SoundId(0)`)と `se_test`(`SoundId(1)`)。
    pub fn test_minimal() -> &'static AssetBundle {
        &MINIMAL
    }
}
