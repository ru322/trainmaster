//! フレームテクスチャへの描画と、ウィンドウへの整数倍の転送 [14 §3.3]。

mod batch;
mod exec;
mod layers;

use sdl2::pixels::{Color as SdlColor, PixelFormatEnum};
use sdl2::rect::Rect as SdlRect;
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};
use tm_platform_api::{AssetBundle, DrawList, LOGICAL_HEIGHT, LOGICAL_WIDTH, Rect};

use self::batch::{Area, QuadBatch, SCREEN};
use self::exec::{Drawer, SpriteTable};
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

    /// 層 `i` のテクスチャを `(w, h)` で用意する。予算を超えるときは、このフレームに出ない層(`active` が false)を先に捨てる。
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
    pub(crate) layer_redraws: u32,
    layer_failed: bool,
    #[cfg(test)]
    pub(crate) inject: Inject,
}

impl<'tc> Renderer<'tc> {
    /// フレームテクスチャを作る。作れなければ直接描画モード(14 §3.3.4)にする。
    pub(crate) fn new(
        canvas: &mut Canvas<Window>,
        tc: &'tc TextureCreator<WindowContext>,
        mode: BatchMode,
    ) -> Self {
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
            layer_redraws: 0,
            layer_failed: false,
            #[cfg(test)]
            inject: Inject::default(),
        };
        r.create_frame(canvas, tc);
        r
    }

    fn create_frame(
        &mut self,
        canvas: &mut Canvas<Window>,
        tc: &'tc TextureCreator<WindowContext>,
    ) {
        let supported = canvas.render_target_supported();
        #[cfg(test)]
        let supported = supported && !self.inject.frame_tex;
        let tex = if supported {
            tc.create_texture_target(PixelFormatEnum::RGBA32, LOGICAL_WIDTH, LOGICAL_HEIGHT)
                .map_err(|e| e.to_string())
        } else {
            Err("レンダーターゲットに対応していない".to_owned())
        };
        match tex {
            Ok(mut t) => {
                t.set_blend_mode(BlendMode::None);
                self.frame_tex = Some(t);
                if canvas.logical_size() != (0, 0) {
                    if let Err(e) = canvas.set_logical_size(0, 0) {
                        log::warn!("set_logical_size: {e}");
                    }
                    if let Err(e) = canvas.set_integer_scale(false) {
                        log::warn!("set_integer_scale: {e}");
                    }
                }
            }
            Err(e) => {
                log::warn!("フレームテクスチャを作れないので直接描画モードにする: {e}");
                if let Err(e) = canvas.set_logical_size(LOGICAL_WIDTH, LOGICAL_HEIGHT) {
                    log::warn!("set_logical_size: {e}");
                }
                if let Err(e) = canvas.set_integer_scale(true) {
                    log::warn!("set_integer_scale: {e}");
                }
            }
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

    /// フレームテクスチャがなく、ウィンドウに直接描いている(14 §3.3.4)。
    #[cfg(test)]
    pub(crate) fn is_direct(&self) -> bool {
        self.frame_tex.is_none()
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
        self.atlases.clear();
        self.create_frame(canvas, tc);
        match bundle {
            Some(b) => self.load(tc, b),
            None => Ok(()),
        }
    }

    /// 層とフレームを描き、ウィンドウへ転送する(`present` はしない)。
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
        let Some(sprites) = &self.sprites else {
            canvas.set_draw_color(BLACK);
            canvas.clear();
            return result;
        };
        let mut d = Drawer {
            atlases: &mut self.atlases,
            sprites,
            batch: &mut self.batch,
            mode: self.mode,
            direct: self.frame_tex.is_none(),
            bounds: SCREEN,
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
        for (i, section) in sections.iter().enumerate() {
            let Some(section) = section else { continue };
            let Some(rect) = placed(section) else {
                continue;
            };
            if !starts_opaque(&list.cmds, section) {
                result.layer_not_opaque = true;
            }
            let slot = if self.frame_tex.is_some() {
                self.layers
                    .ensure(i, tc, (rect.w, rect.h), self.max_tex, &active, fail)
            } else {
                Err(String::new())
            };
            uses[i] = match slot {
                Ok(slot) => {
                    if slot.revision != Some(section.revision) {
                        let content = &list.cmds[section.begin + 1..section.end];
                        d.bounds = Area::of(Rect {
                            x: 0,
                            y: 0,
                            w: rect.w,
                            h: rect.h,
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
                        d.bounds = SCREEN;
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
        match &mut self.frame_tex {
            Some(frame) => {
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
                canvas.set_draw_color(BLACK);
                canvas.clear();
                match canvas.output_size() {
                    Ok((w, h)) => {
                        if let Err(e) = canvas.copy(frame, None, letterbox(w, h)) {
                            d.fail(e);
                        }
                    }
                    Err(e) => d.fail(e),
                }
            }
            None => {
                canvas.set_draw_color(BLACK);
                canvas.clear();
                d.exec_frame(canvas, &list.cmds, &uses, &self.layers.slots);
            }
        }
        result.quads = d.quads;
        result.error = d.error;
        result.bad_sprite = d.bad_sprite;
        result.bad_layer = d.bad_layer || issues.ignored || issues.broken;
        result.broken_layer = issues.broken;
        result
    }

    /// フレームの画素(RGBA、640×360、行優先)。直接描画モードではウィンドウを読み、倍率の分を間引く。
    pub(crate) fn read_frame(&mut self, canvas: &mut Canvas<Window>) -> Result<Vec<u8>, String> {
        let Some(frame) = self.frame_tex.as_mut() else {
            return read_window(canvas);
        };
        let mut pixels = Err(String::new());
        canvas
            .with_texture_canvas(frame, |c| {
                pixels = c.read_pixels(None, PixelFormatEnum::RGBA32)
            })
            .map_err(|e| e.to_string())?;
        pixels
    }
}

fn read_window(canvas: &Canvas<Window>) -> Result<Vec<u8>, String> {
    let (w, h) = canvas.output_size()?;
    let view = letterbox(w, h);
    let s = (view.width() / LOGICAL_WIDTH) as usize;
    let raw = canvas.read_pixels(view, PixelFormatEnum::RGBA32)?;
    let pitch = view.width() as usize * 4;
    let mut out = Vec::with_capacity((LOGICAL_WIDTH * LOGICAL_HEIGHT * 4) as usize);
    for y in 0..LOGICAL_HEIGHT as usize {
        for x in 0..LOGICAL_WIDTH as usize {
            let i = y * s * pitch + x * s * 4;
            out.extend_from_slice(raw.get(i..i + 4).ok_or("read_pixels が短い")?);
        }
    }
    Ok(out)
}

/// 出力の大きさ `(w, h)` に 640×360 を整数倍で置く矩形 [14 §3.3.1]。
pub(crate) fn letterbox(w: u32, h: u32) -> SdlRect {
    let s = (w / LOGICAL_WIDTH).min(h / LOGICAL_HEIGHT).max(1);
    let (dw, dh) = (LOGICAL_WIDTH * s, LOGICAL_HEIGHT * s);
    SdlRect::new(
        (i64::from(w) - i64::from(dw)) as i32 / 2,
        (i64::from(h) - i64::from(dh)) as i32 / 2,
        dw,
        dh,
    )
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
    fn letterbox_integer_scale() {
        assert_eq!(letterbox(1920, 1080), SdlRect::new(0, 0, 1920, 1080));
        assert_eq!(letterbox(1280, 720), SdlRect::new(0, 0, 1280, 720));
        assert_eq!(letterbox(1366, 768), SdlRect::new(43, 24, 1280, 720));
        assert_eq!(letterbox(640, 360), SdlRect::new(0, 0, 640, 360));
        assert_eq!(letterbox(1920, 1200), SdlRect::new(0, 60, 1920, 1080));
        assert_eq!(letterbox(320, 200), SdlRect::new(-160, -80, 640, 360));
        assert_eq!(letterbox(1920, 720), SdlRect::new(320, 0, 1280, 720));
        assert_eq!(letterbox(1280, 1080), SdlRect::new(0, 180, 1280, 720));
    }
}
