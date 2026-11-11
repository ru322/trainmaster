//! `DrawCmd` の実行(1 フレーム分)[14 §3.4]。

use sdl2::pixels::Color as SdlColor;
use sdl2::rect::Rect as SdlRect;
use sdl2::render::{BlendMode, Canvas, Texture};
use sdl2::video::Window;
use tm_platform_api::{AssetBundle, Color, DrawCmd, LOGICAL_HEIGHT, LOGICAL_WIDTH, Rect, SpriteId};

use super::batch::{Area, QuadBatch, Uv, line_runs};
use super::layers::{LAYER_COUNT, LayerSlot, LayerUse, interval};
use crate::config::BatchMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpriteEntry {
    pub(crate) atlas: usize,
    pub(crate) src: Rect,
    pub(crate) uv: Uv,
}

/// スプライトのアトラス上の位置とテクスチャ座標の表。
pub(crate) struct SpriteTable {
    entries: Vec<SpriteEntry>,
    white: SpriteEntry,
}

impl SpriteTable {
    /// `validate` を通った束から作る。
    pub(crate) fn new(bundle: &AssetBundle) -> Self {
        let entries: Vec<SpriteEntry> = bundle
            .sprites
            .iter()
            .map(|r| {
                let atlas = usize::from(r.atlas);
                let page = &bundle.atlases[atlas];
                let (aw, ah) = (page.width as f32, page.height as f32);
                let (x, y, w, h) = (
                    f32::from(r.x),
                    f32::from(r.y),
                    f32::from(r.w),
                    f32::from(r.h),
                );
                SpriteEntry {
                    atlas,
                    src: Rect {
                        x: i32::from(r.x),
                        y: i32::from(r.y),
                        w: u32::from(r.w),
                        h: u32::from(r.h),
                    },
                    uv: Uv {
                        u0: x / aw,
                        v0: y / ah,
                        u1: (x + w) / aw,
                        v1: (y + h) / ah,
                    },
                }
            })
            .collect();
        let mut white = entries[usize::from(bundle.white.0)];
        let (cu, cv) = (
            (white.uv.u0 + white.uv.u1) / 2.0,
            (white.uv.v0 + white.uv.v1) / 2.0,
        );
        white.uv = Uv {
            u0: cu,
            v0: cv,
            u1: cu,
            v1: cv,
        };
        SpriteTable { entries, white }
    }

    pub(crate) fn get(&self, id: SpriteId) -> Option<&SpriteEntry> {
        self.entries.get(usize::from(id.0))
    }
}

/// 1 回の描画の間の状態。失敗は最初の 1 つを覚え、以降の描画を飛ばす。
pub(crate) struct Drawer<'a, 'tc> {
    pub(crate) atlases: &'a mut [Texture<'tc>],
    pub(crate) sprites: &'a SpriteTable,
    pub(crate) batch: &'a mut QuadBatch,
    pub(crate) mode: BatchMode,
    pub(crate) direct: bool,
    /// 描き先の範囲(フレームと直接描画モードは画面、層のテクスチャは `(0, 0, w, h)`)。
    pub(crate) bounds: Area,
    pub(crate) quads: u32,
    pub(crate) error: Option<String>,
    pub(crate) bad_sprite: bool,
    pub(crate) bad_layer: bool,
}

pub(crate) fn sdl_color(c: Color) -> SdlColor {
    SdlColor::RGBA(c.r, c.g, c.b, c.a)
}

pub(crate) fn sdl_rect(r: Rect) -> SdlRect {
    SdlRect::new(r.x, r.y, r.w, r.h)
}

impl Drawer<'_, '_> {
    pub(crate) fn fail(&mut self, e: impl ToString) {
        if self.error.is_none() {
            self.error = Some(e.to_string());
        }
    }

    pub(crate) fn flush(&mut self, c: &mut Canvas<Window>) {
        if self.batch.is_empty() {
            return;
        }
        if self.error.is_none()
            && let Some(tex) = self.atlases.get(self.batch.atlas())
            && let Err(e) =
                c.render_geometry(self.batch.vertices(), Some(tex), self.batch.indices())
        {
            self.fail(e);
        }
        self.batch.clear();
    }

    /// 直接描画モードでは `clear` が論理サイズの外の帯まで塗るので、640×360 をブレンドなしで塗る(14 §3.3.4)。
    fn clear(&mut self, c: &mut Canvas<Window>, color: Color) {
        self.flush(c);
        if self.error.is_some() {
            return;
        }
        c.set_draw_color(sdl_color(color));
        if self.direct {
            c.set_blend_mode(BlendMode::None);
            let r = c.fill_rect(SdlRect::new(0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT));
            c.set_blend_mode(BlendMode::Blend);
            if let Err(e) = r {
                self.fail(e);
            }
        } else {
            c.clear();
        }
    }

    fn fill(&mut self, c: &mut Canvas<Window>, area: Area, color: Color) {
        let Some(rect) = area.meet(self.bounds).to_rect() else {
            return;
        };
        if self.error.is_some() {
            return;
        }
        self.quads += 1;
        match self.mode {
            BatchMode::Geometry => {
                let white = self.sprites.white;
                self.push(c, white.atlas, rect, white.uv, color);
            }
            BatchMode::Copy => {
                c.set_blend_mode(BlendMode::Blend);
                c.set_draw_color(sdl_color(color));
                if let Err(e) = c.fill_rect(sdl_rect(rect)) {
                    self.fail(e);
                }
            }
        }
    }

    fn sprite(&mut self, c: &mut Canvas<Window>, id: SpriteId, dst: Area, tint: Color) {
        let entry = self.sprites.get(id);
        debug_assert!(entry.is_some(), "範囲外のスプライト {id:?}");
        let Some(&entry) = entry else {
            self.bad_sprite = true;
            return;
        };
        if dst.meet(self.bounds).to_rect().is_none() || self.error.is_some() {
            return;
        }
        let Some(dst) = dst.to_rect() else {
            return;
        };
        self.quads += 1;
        match self.mode {
            BatchMode::Geometry => self.push(c, entry.atlas, dst, entry.uv, tint),
            BatchMode::Copy => {
                let Some(tex) = self.atlases.get_mut(entry.atlas) else {
                    return;
                };
                tex.set_color_mod(tint.r, tint.g, tint.b);
                tex.set_alpha_mod(tint.a);
                if let Err(e) = c.copy(tex, sdl_rect(entry.src), sdl_rect(dst)) {
                    self.fail(e);
                }
            }
        }
    }

    fn push(&mut self, c: &mut Canvas<Window>, atlas: usize, dst: Rect, uv: Uv, color: Color) {
        if self.batch.needs_flush(atlas) {
            self.flush(c);
        }
        self.batch.push(atlas, dst, uv, color);
    }

    fn line(&mut self, c: &mut Canvas<Window>, from: (i64, i64), to: (i64, i64), color: Color) {
        line_runs(from, to, self.bounds, |r| self.fill(c, Area::of(r), color));
    }

    /// 層の区間の中身を描く。`place` があればその場描き(平行移動し、`Clear` を矩形の塗りにする)。
    pub(crate) fn exec_content(
        &mut self,
        c: &mut Canvas<Window>,
        cmds: &[DrawCmd],
        place: Option<Rect>,
    ) {
        let (dx, dy) = place.map_or((0, 0), |r| (i64::from(r.x), i64::from(r.y)));
        let at = |x: i32, y: i32| (i64::from(x) + dx, i64::from(y) + dy);
        for cmd in cmds {
            match *cmd {
                DrawCmd::Clear(color) => match place {
                    Some(r) => self.fill(c, Area::of(r), color),
                    None => self.clear(c, color),
                },
                DrawCmd::FillRect { rect, color } => {
                    self.fill(c, Area::of(rect).shifted(dx, dy), color)
                }
                DrawCmd::Line {
                    x0,
                    y0,
                    x1,
                    y1,
                    color,
                } => self.line(c, at(x0, y0), at(x1, y1), color),
                DrawCmd::Sprite { id, dst, tint } => {
                    self.sprite(c, id, Area::of(dst).shifted(dx, dy), tint)
                }
                DrawCmd::BeginCachedLayer { .. }
                | DrawCmd::EndCachedLayer
                | DrawCmd::DrawCachedLayer { .. } => self.bad_layer = true,
            }
        }
    }

    /// フレームの描画。層の区間は読み飛ばし、`DrawCachedLayer` で `uses` のとおりに描く。
    pub(crate) fn exec_frame(
        &mut self,
        c: &mut Canvas<Window>,
        cmds: &[DrawCmd],
        uses: &[LayerUse; LAYER_COUNT],
        slots: &[Option<LayerSlot<'_>>; LAYER_COUNT],
    ) {
        if self.mode == BatchMode::Copy {
            c.set_blend_mode(BlendMode::Blend);
        }
        let mut i = 0;
        while i < cmds.len() {
            match cmds[i] {
                DrawCmd::BeginCachedLayer { .. } => i = interval(cmds, i).0,
                DrawCmd::EndCachedLayer => self.bad_layer = true,
                DrawCmd::DrawCachedLayer { layer } => {
                    self.flush(c);
                    match uses.get(usize::from(layer.0)) {
                        Some(LayerUse::Cached(rect)) => {
                            if let Some(slot) = &slots[usize::from(layer.0)]
                                && self.error.is_none()
                                && let Err(e) = c.copy(&slot.tex, None, sdl_rect(*rect))
                            {
                                self.fail(e);
                            }
                        }
                        Some(LayerUse::Inline(section)) => {
                            let outer = self.bounds;
                            if let Some(clip) = Area::of(section.rect).meet(outer).to_rect() {
                                self.bounds = Area::of(clip);
                                c.set_clip_rect(sdl_rect(clip));
                                self.exec_content(
                                    c,
                                    &cmds[section.begin + 1..section.end],
                                    Some(section.rect),
                                );
                                self.flush(c);
                                c.set_clip_rect(None);
                                self.bounds = outer;
                            }
                        }
                        Some(LayerUse::None) | None => {}
                    }
                }
                _ => self.exec_content(c, &cmds[i..=i], None),
            }
            i += 1;
        }
        self.flush(c);
    }
}
