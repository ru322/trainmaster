//! 描画の解像度(1920×1080)での描画と、ウィンドウへの置き方 [14 §3.3]。
//! 出力が 1920×1080 以上ならウィンドウに直接(整数倍と黒い帯)、小さければフレームテクスチャに描いて縮めて写す。

mod batch;
mod exec;
mod layers;

use sdl2::pixels::{Color as SdlColor, PixelFormatEnum};
use sdl2::rect::Rect as SdlRect;
use sdl2::render::{BlendMode, Canvas, ScaleMode, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};
use tm_platform_api::{AssetBundle, DrawList, RENDER_HEIGHT, RENDER_SCALE, RENDER_WIDTH, Rect};

use self::batch::{Area, QuadBatch};
use self::exec::{Drawer, SpriteTable, Xf, sdl_rect};
use self::layers::{
    LAYER_BUDGET_BYTES, LAYER_COUNT, LAYER_MAX_DIM, LayerSlot, LayerUse, PlanIssues, placed, plan,
    starts_opaque,
};
use crate::config::BatchMode;

const BLACK: SdlColor = SdlColor::RGBA(0, 0, 0, 255);
const TRANSPARENT: SdlColor = SdlColor::RGBA(0, 0, 0, 0);

/// 1 フレームの描画の結果。
pub(crate) struct DrawResult {
    pub(crate) quads: u32,
    pub(crate) error: Option<String>,
    pub(crate) bad_sprite: bool,
    pub(crate) bad_layer: bool,
    /// 入れ子の Begin、End のない Begin があった(14 §3.5.5)。
    pub(crate) broken_layer: bool,
    /// 最初のコマンドが不透明な `Clear` でない層があった(14 §3.5.5)。
    pub(crate) layer_not_opaque: bool,
}

/// テストで描画の失敗を起こす。
#[cfg(test)]
#[derive(Default)]
pub(crate) struct Inject {
    pub(crate) frame_tex: bool,
    pub(crate) layer_tex: bool,
    pub(crate) layer_draw: bool,
    pub(crate) frame_draw: bool,
}

/// レンダラの最大テクスチャサイズ。0(ソフトウェアのレンダラ)は上限なし。
fn tex_limit(v: u32) -> u32 {
    if v == 0 { u32::MAX } else { v }
}

/// 出力(ウィンドウ)への描画の置き方。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    /// ウィンドウに直接描く。描画の 1 画素を `s` × `s` 画素にし、`(x, y)` に置く(出力が 1920×1080 以上)。
    Direct {
        /// 整数倍の倍率。
        s: u32,
        /// 左の帯の幅。
        x: i32,
        /// 上の帯の高さ。
        y: i32,
    },
    /// 1920×1080 のフレームテクスチャに描き、縦横比を保って `dst` へ線形補間で縮めて写す(出力が小さいとき)。
    Scaled {
        /// 写す先。
        dst: SdlRect,
    },
}

/// 出力の大きさ `(w, h)` に描画の解像度を置く [14 §3.3.1]。
pub(crate) fn place(w: u32, h: u32) -> Placement {
    let s = (w / RENDER_WIDTH).min(h / RENDER_HEIGHT);
    if s >= 1 {
        let (dw, dh) = (RENDER_WIDTH * s, RENDER_HEIGHT * s);
        return Placement::Direct {
            s,
            x: ((w - dw) / 2) as i32,
            y: ((h - dh) / 2) as i32,
        };
    }
    let (w64, h64) = (u64::from(w.max(1)), u64::from(h.max(1)));
    let (rw, rh) = (u64::from(RENDER_WIDTH), u64::from(RENDER_HEIGHT));
    let (dw, dh) = if w64 * rh <= h64 * rw {
        (w64, (w64 * rh / rw).max(1))
    } else {
        ((h64 * rw / rh).max(1), h64)
    };
    Placement::Scaled {
        dst: SdlRect::new(
            ((w64 - dw) / 2) as i32,
            ((h64 - dh) / 2) as i32,
            dw as u32,
            dh as u32,
        ),
    }
}

/// このフレームの描き先。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    /// ウィンドウに直接(`Placement::Direct`)。
    Window { s: u32, x: i32, y: i32 },
    /// フレームテクスチャ(`Placement::Scaled`)。
    Frame { dst: SdlRect },
    /// レンダーターゲットが使えず出力が小さいとき。SDL の論理サイズ(1920×1080)でウィンドウに縮めて描く。
    Logical,
}

/// 層のテクスチャ。作れなかった大きさは、大きさが変わるかデバイスのリセットまで作り直さない。
struct LayerPool<'tc> {
    slots: [Option<LayerSlot<'tc>>; LAYER_COUNT],
    failed: [Option<(u32, u32)>; LAYER_COUNT],
    recreates: u32,
}

impl<'tc> LayerPool<'tc> {
    fn new() -> Self {
        LayerPool {
            slots: [const { None }; LAYER_COUNT],
            failed: [None; LAYER_COUNT],
            recreates: 0,
        }
    }

    fn reset(&mut self) {
        self.slots = [const { None }; LAYER_COUNT];
        self.failed = [None; LAYER_COUNT];
    }

    fn size(&self, i: usize) -> Option<(u32, u32)> {
        self.slots[i].as_ref().map(|s| (s.w, s.h))
    }

    fn bytes(&self) -> u64 {
        (0..LAYER_COUNT)
            .filter_map(|j| self.size(j))
            .map(|(w, h)| u64::from(w) * u64::from(h) * 4)
            .sum()
    }

    /// 層 `i` のテクスチャを `(w, h)`(描画の画素)で用意する。予算を超えるときは、このフレームに出ない層(`active` が false)を先に捨てる。
    fn ensure(
        &mut self,
        i: usize,
        tc: &'tc TextureCreator<WindowContext>,
        (w, h): (u32, u32),
        max_tex: (u32, u32),
        active: &[bool; LAYER_COUNT],
        fail: bool,
    ) -> Result<&mut LayerSlot<'tc>, String> {
        if self.size(i).is_some_and(|s| s != (w, h)) {
            self.slots[i] = None;
            self.recreates += 1;
        }
        if self.slots[i].is_none() {
            if w > LAYER_MAX_DIM.min(max_tex.0) || h > LAYER_MAX_DIM.min(max_tex.1) {
                return Err(format!("大きすぎる {w}×{h}"));
            }
            let need = u64::from(w) * u64::from(h) * 4;
            if self.bytes() + need > LAYER_BUDGET_BYTES {
                for j in (0..LAYER_COUNT).filter(|&j| !active[j]) {
                    self.slots[j] = None;
                }
            }
            if self.bytes() + need > LAYER_BUDGET_BYTES {
                return Err(format!("予算を超える {w}×{h}"));
            }
            if self.failed[i] == Some((w, h)) {
                return Err(String::new());
            }
            let created = if fail {
                Err("注入した層のテクスチャの失敗".to_owned())
            } else {
                tc.create_texture_target(PixelFormatEnum::RGBA32, w, h)
                    .map_err(|e| e.to_string())
            };
            let mut tex = created.inspect_err(|_| self.failed[i] = Some((w, h)))?;
            tex.set_blend_mode(BlendMode::None);
            self.failed[i] = None;
            self.slots[i] = Some(LayerSlot {
                tex,
                w,
                h,
                revision: None,
            });
        }
        self.slots[i].as_mut().ok_or_else(String::new)
    }
}

/// 描画に使うテクスチャ。フィールドの順は破棄の順(14 §3.8)。
pub(crate) struct Renderer<'tc> {
    layers: LayerPool<'tc>,
    frame_tex: Option<Texture<'tc>>,
    atlases: Vec<Texture<'tc>>,
    sprites: Option<SpriteTable>,
    batch: QuadBatch,
    mode: BatchMode,
    max_tex: (u32, u32),
    /// レンダーターゲット(層とフレームテクスチャ)が使える。
    targets: bool,
    /// SDL の論理サイズを設定している(`Target::Logical`)。
    logical: bool,
    /// 直前のフレームの描き先(`read_frame` が使う)。
    last: Option<Target>,
    pub(crate) layer_redraws: u32,
    layer_failed: bool,
    frame_failed: bool,
    #[cfg(test)]
    pub(crate) inject: Inject,
}

impl<'tc> Renderer<'tc> {
    /// レンダーターゲットが使えるかを調べる。フレームテクスチャは要るときに作る。
    pub(crate) fn new(canvas: &mut Canvas<Window>, mode: BatchMode) -> Self {
        let info = canvas.info();
        let mut r = Renderer {
            layers: LayerPool::new(),
            frame_tex: None,
            atlases: Vec::new(),
            sprites: None,
            batch: QuadBatch::new(),
            mode,
            max_tex: (
                tex_limit(info.max_texture_width),
                tex_limit(info.max_texture_height),
            ),
            targets: false,
            logical: false,
            last: None,
            layer_redraws: 0,
            layer_failed: false,
            frame_failed: false,
            #[cfg(test)]
            inject: Inject::default(),
        };
        r.probe(canvas);
        r
    }

    fn probe(&mut self, canvas: &Canvas<Window>) {
        let supported = canvas.render_target_supported();
        #[cfg(test)]
        let supported = supported && !self.inject.frame_tex;
        if !supported {
            log::warn!("レンダーターゲットに対応していないので、層をその場で描く(14 §3.3.4)");
        }
        self.targets = supported;
    }

    /// 1920×1080 のフレームテクスチャ(縮めて写すので線形補間)。作れなければ `None`。
    fn frame_texture(
        &mut self,
        tc: &'tc TextureCreator<WindowContext>,
    ) -> Option<&mut Texture<'tc>> {
        if self.frame_tex.is_none() && self.targets && !self.frame_failed {
            match tc.create_texture_target(PixelFormatEnum::RGBA32, RENDER_WIDTH, RENDER_HEIGHT) {
                Ok(mut t) => {
                    t.set_blend_mode(BlendMode::None);
                    t.set_scale_mode(ScaleMode::Linear);
                    self.frame_tex = Some(t);
                }
                Err(e) => {
                    self.frame_failed = true;
                    log::warn!("フレームテクスチャを作れないので SDL の論理サイズで縮める: {e}");
                }
            }
        }
        self.frame_tex.as_mut()
    }

    /// 論理サイズ(`Target::Logical`)を付けるか外す。
    fn set_logical(&mut self, canvas: &mut Canvas<Window>, on: bool) {
        if self.logical == on {
            return;
        }
        self.logical = on;
        let (w, h) = if on {
            (RENDER_WIDTH, RENDER_HEIGHT)
        } else {
            (0, 0)
        };
        if let Err(e) = canvas.set_logical_size(w, h) {
            log::warn!("set_logical_size: {e}");
        }
        if let Err(e) = canvas.set_integer_scale(false) {
            log::warn!("set_integer_scale: {e}");
        }
    }

    /// アトラスをテクスチャにし、スプライトの表を作る。`bundle` は検査済み。
    pub(crate) fn load(
        &mut self,
        tc: &'tc TextureCreator<WindowContext>,
        bundle: &AssetBundle,
    ) -> Result<(), String> {
        self.atlases.clear();
        for (i, page) in bundle.atlases.iter().enumerate() {
            if page.width > self.max_tex.0 || page.height > self.max_tex.1 {
                return Err(format!(
                    "アトラス {i} が大きすぎる: {}×{}",
                    page.width, page.height
                ));
            }
            let mut tex = tc
                .create_texture_static(PixelFormatEnum::RGBA32, page.width, page.height)
                .map_err(|e| e.to_string())?;
            tex.update(None, page.rgba, page.width as usize * 4)
                .map_err(|e| e.to_string())?;
            tex.set_blend_mode(BlendMode::Blend);
            self.atlases.push(tex);
        }
        self.sprites = Some(SpriteTable::new(bundle));
        Ok(())
    }

    /// レンダーターゲットが使えず、層をその場で描いている(14 §3.3.4)。
    #[cfg(test)]
    pub(crate) fn is_direct(&self) -> bool {
        !self.targets
    }

    /// 起動から層のテクスチャを大きさの変化で作り直した回数。
    pub(crate) fn layer_recreates(&self) -> u32 {
        self.layers.recreates
    }

    #[cfg(test)]
    pub(crate) fn layer_size(&self, i: usize) -> Option<(u32, u32)> {
        self.layers.size(i)
    }

    #[cfg(test)]
    pub(crate) fn set_max_tex(&mut self, max: (u32, u32)) {
        self.max_tex = max;
    }

    /// 次のフレームで全層を描き直す(`RenderTargetsReset`)。
    pub(crate) fn invalidate_layers(&mut self) {
        for slot in self.layers.slots.iter_mut().flatten() {
            slot.revision = None;
        }
    }

    /// テクスチャをすべて作り直す(`RenderDeviceReset`)。
    pub(crate) fn reset_device(
        &mut self,
        canvas: &mut Canvas<Window>,
        tc: &'tc TextureCreator<WindowContext>,
        bundle: Option<&AssetBundle>,
    ) -> Result<(), String> {
        self.layers.reset();
        self.frame_tex = None;
        self.frame_failed = false;
        self.atlases.clear();
        self.probe(canvas);
        match bundle {
            Some(b) => self.load(tc, b),
            None => Ok(()),
        }
    }

    /// 層とフレームを描き、ウィンドウへ写す(`present` はしない)。
    pub(crate) fn draw(
        &mut self,
        canvas: &mut Canvas<Window>,
        tc: &'tc TextureCreator<WindowContext>,
        list: &DrawList,
    ) -> DrawResult {
        let mut result = DrawResult {
            quads: 0,
            error: None,
            bad_sprite: false,
            bad_layer: false,
            broken_layer: false,
            layer_not_opaque: false,
        };
        let output = match canvas.output_size() {
            Ok(o) => o,
            Err(e) => {
                result.error = Some(e);
                return result;
            }
        };
        let target = match place(output.0, output.1) {
            Placement::Direct { s, x, y } => Target::Window { s, x, y },
            Placement::Scaled { dst } => match self.frame_texture(tc) {
                Some(_) => Target::Frame { dst },
                None => Target::Logical,
            },
        };
        self.set_logical(canvas, target == Target::Logical);
        self.last = Some(target);
        if self.sprites.is_none() {
            canvas.set_draw_color(BLACK);
            canvas.clear();
            return result;
        }
        let native = Area {
            x0: 0,
            y0: 0,
            x1: i64::from(RENDER_WIDTH),
            y1: i64::from(RENDER_HEIGHT),
        };
        let mut d = Drawer {
            atlases: &mut self.atlases,
            sprites: self.sprites.as_ref().expect("上で確かめた"),
            batch: &mut self.batch,
            mode: self.mode,
            clear_fill: false,
            frame: native,
            clip: None,
            xf: Xf::NATIVE,
            bounds: native,
            quads: 0,
            error: None,
            bad_sprite: false,
            bad_layer: false,
        };
        let mut issues = PlanIssues::default();
        let sections = plan(&list.cmds, &mut issues);
        let mut uses = [LayerUse::None; LAYER_COUNT];
        let active = sections.map(|s| s.as_ref().and_then(placed).is_some());
        #[cfg(not(test))]
        let fail = false;
        #[cfg(test)]
        let fail = self.inject.layer_tex;
        let k = RENDER_SCALE;
        for (i, section) in sections.iter().enumerate() {
            let Some(section) = section else { continue };
            let Some(rect) = placed(section) else {
                continue;
            };
            if !starts_opaque(&list.cmds, section) {
                result.layer_not_opaque = true;
            }
            let size = (rect.w.saturating_mul(k), rect.h.saturating_mul(k));
            let slot = if self.targets {
                self.layers.ensure(i, tc, size, self.max_tex, &active, fail)
            } else {
                Err(String::new())
            };
            uses[i] = match slot {
                Ok(slot) => {
                    if slot.revision != Some(section.revision) {
                        let content = &list.cmds[section.begin + 1..section.end];
                        d.xf = Xf::NATIVE;
                        d.bounds = Area::of(Rect {
                            x: 0,
                            y: 0,
                            w: size.0,
                            h: size.1,
                        });
                        let r = canvas.with_texture_canvas(&mut slot.tex, |c| {
                            c.set_draw_color(TRANSPARENT);
                            c.clear();
                            d.exec_content(c, content, None);
                            d.flush(c);
                        });
                        #[cfg(test)]
                        if self.inject.layer_draw {
                            d.fail("注入した層の失敗");
                        }
                        match r {
                            Ok(()) if d.error.is_none() => slot.revision = Some(section.revision),
                            Ok(()) => {}
                            Err(e) => d.fail(e),
                        }
                        self.layer_redraws += 1;
                    }
                    LayerUse::Cached(rect)
                }
                Err(e) => {
                    if !e.is_empty() && !self.layer_failed {
                        self.layer_failed = true;
                        log::warn!("層 {i} はその場描きにする: {e}");
                    }
                    LayerUse::Inline(*section)
                }
            };
        }
        canvas.set_draw_color(BLACK);
        canvas.clear();
        match target {
            Target::Window { s, x, y } => {
                let frame = Area {
                    x0: i64::from(x),
                    y0: i64::from(y),
                    x1: i64::from(x) + i64::from(RENDER_WIDTH * s),
                    y1: i64::from(y) + i64::from(RENDER_HEIGHT * s),
                };
                d.xf = Xf {
                    s: i64::from(s),
                    ox: i64::from(x),
                    oy: i64::from(y),
                };
                let bars = frame
                    != Area::of(Rect {
                        x: 0,
                        y: 0,
                        w: output.0,
                        h: output.1,
                    });
                (d.bounds, d.frame, d.clear_fill) = (frame, frame, bars);
                d.clip = bars.then(|| frame.to_rect()).flatten();
                canvas.set_clip_rect(d.clip.map(sdl_rect));
                d.exec_frame(canvas, &list.cmds, &uses, &self.layers.slots);
                canvas.set_clip_rect(None);
            }
            Target::Logical => {
                (d.xf, d.bounds, d.frame, d.clear_fill) = (Xf::NATIVE, native, native, true);
                d.clip = native.to_rect();
                canvas.set_clip_rect(d.clip.map(sdl_rect));
                d.exec_frame(canvas, &list.cmds, &uses, &self.layers.slots);
                canvas.set_clip_rect(None);
            }
            Target::Frame { dst } => {
                (d.xf, d.bounds, d.clear_fill) = (Xf::NATIVE, native, false);
                let frame = self.frame_tex.as_mut().expect("上で作った");
                let layers = &self.layers.slots;
                let r = canvas.with_texture_canvas(frame, |c| {
                    c.set_draw_color(BLACK);
                    c.clear();
                    d.exec_frame(c, &list.cmds, &uses, layers);
                });
                if let Err(e) = r {
                    d.fail(e);
                }
                #[cfg(test)]
                if self.inject.frame_draw {
                    d.fail("注入したフレームの失敗");
                }
                if let Err(e) = canvas.copy(frame, None, dst) {
                    d.fail(e);
                }
            }
        }
        result.quads = d.quads;
        result.error = d.error;
        result.bad_sprite = d.bad_sprite;
        result.bad_layer = d.bad_layer || issues.ignored || issues.broken;
        result.broken_layer = issues.broken;
        result
    }

    /// 描画の画素(RGBA、1920×1080、行優先)。ウィンドウに直接描いたときはウィンドウを読み、倍率の分を間引く。
    pub(crate) fn read_frame(&mut self, canvas: &mut Canvas<Window>) -> Result<Vec<u8>, String> {
        match self.last {
            Some(Target::Frame { .. }) => {
                let frame = self.frame_tex.as_mut().ok_or("フレームテクスチャがない")?;
                let mut pixels = Err(String::new());
                canvas
                    .with_texture_canvas(frame, |c| {
                        pixels = c.read_pixels(None, PixelFormatEnum::RGBA32)
                    })
                    .map_err(|e| e.to_string())?;
                pixels
            }
            Some(Target::Window { s, x, y }) => read_window(canvas, s, x, y),
            Some(Target::Logical) => Err("縮めて描いているので描画の解像度では読めない".to_owned()),
            None => Err("まだ描いていない".to_owned()),
        }
    }
}

fn read_window(canvas: &Canvas<Window>, s: u32, x: i32, y: i32) -> Result<Vec<u8>, String> {
    let view = SdlRect::new(x, y, RENDER_WIDTH * s, RENDER_HEIGHT * s);
    let raw = canvas.read_pixels(view, PixelFormatEnum::RGBA32)?;
    let (s, pitch) = (s as usize, view.width() as usize * 4);
    let mut out = Vec::with_capacity((RENDER_WIDTH * RENDER_HEIGHT * 4) as usize);
    for y in 0..RENDER_HEIGHT as usize {
        for x in 0..RENDER_WIDTH as usize {
            let i = y * s * pitch + x * s * 4;
            out.extend_from_slice(raw.get(i..i + 4).ok_or("read_pixels が短い")?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_max_texture_means_unlimited() {
        assert_eq!(tex_limit(0), u32::MAX);
        assert_eq!(tex_limit(2048), 2048);
    }

    #[test]
    fn outputs_of_full_hd_or_more_are_drawn_directly_at_an_integer_scale() {
        let direct = |s, x, y| Placement::Direct { s, x, y };
        assert_eq!(place(1920, 1080), direct(1, 0, 0));
        assert_eq!(place(1920, 1200), direct(1, 0, 60));
        assert_eq!(place(2560, 1440), direct(1, 320, 180));
        assert_eq!(place(3840, 2160), direct(2, 0, 0));
        assert_eq!(place(4096, 2160), direct(2, 128, 0));
    }

    #[test]
    fn smaller_outputs_are_scaled_down_keeping_the_aspect() {
        let scaled = |x, y, w, h| Placement::Scaled {
            dst: SdlRect::new(x, y, w, h),
        };
        assert_eq!(place(1280, 720), scaled(0, 0, 1280, 720));
        assert_eq!(place(640, 360), scaled(0, 0, 640, 360));
        assert_eq!(place(1366, 768), scaled(0, 0, 1365, 768));
        assert_eq!(place(1024, 768), scaled(0, 96, 1024, 576));
        assert_eq!(place(1919, 1080), scaled(0, 0, 1919, 1079));
        assert_eq!(place(1, 1), scaled(0, 0, 1, 1));
    }
}
