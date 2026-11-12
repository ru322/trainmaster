//! `DrawCmd` の実行(1 フレーム分)[14 §3.4]。論理座標を描き先の画素にして描く。

use sdl2::pixels::Color as SdlColor;
use sdl2::rect::Rect as SdlRect;
use sdl2::render::{BlendMode, Canvas, Texture};
use sdl2::video::Window;
use tm_platform_api::{
    AssetBundle, Color, DrawCmd, RENDER_SCALE, Rect, SUBPX, SpriteId, SpriteRect, quad_rows,
};

use super::batch::{Area, QuadBatch, Uv, line_runs};
use super::layers::{LAYER_COUNT, LayerSlot, LayerUse, interval};
use crate::config::BatchMode;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpriteEntry {
    pub(crate) atlas: usize,
    pub(crate) rect: SpriteRect,
    pub(crate) uv: Uv,
}

impl SpriteEntry {
    fn new(bundle: &AssetBundle, r: SpriteRect) -> Self {
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
            rect: r,
            uv: Uv {
                u0: x / aw,
                v0: y / ah,
                u1: (x + w) / aw,
                v1: (y + h) / ah,
            },
        }
    }

    fn src(&self) -> SdlRect {
        SdlRect::new(
            i32::from(self.rect.x),
            i32::from(self.rect.y),
            u32::from(self.rect.w),
            u32::from(self.rect.h),
        )
    }
}

/// スプライトのアトラス上の位置とテクスチャ座標の表。大きい版(`AssetBundle::alt`)も持つ。
pub(crate) struct SpriteTable {
    entries: Vec<SpriteEntry>,
    alt: Vec<Option<SpriteEntry>>,
    pub(crate) white: SpriteEntry,
}

impl SpriteTable {
    /// `validate` を通った束から作る。
    pub(crate) fn new(bundle: &AssetBundle) -> Self {
        let entries: Vec<SpriteEntry> = bundle
            .sprites
            .iter()
            .map(|&r| SpriteEntry::new(bundle, r))
            .collect();
        let mut alt = vec![None; entries.len()];
        for a in bundle.alt {
            alt[usize::from(a.id.0)] = Some(SpriteEntry::new(bundle, a.rect));
        }
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
        SpriteTable {
            entries,
            alt,
            white,
        }
    }

    /// 描き先の大きさ `(w, h)`(画素)で使う版。規則は `AssetBundle::source_rect` と同じ。
    pub(crate) fn pick(&self, id: SpriteId, w: u64, h: u64) -> Option<&SpriteEntry> {
        let i = usize::from(id.0);
        let base = self.entries.get(i)?;
        match &self.alt[i] {
            Some(a) if base.rect.is_magnified_by(w, h) => Some(a),
            _ => Some(base),
        }
    }
}

/// 論理座標から描き先の画素への写し方。論理 1px = `RENDER_SCALE × s` 画素、細かい座標の 1 = `s` 画素、原点は `(ox, oy)`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Xf {
    pub(crate) s: i64,
    pub(crate) ox: i64,
    pub(crate) oy: i64,
}

impl Xf {
    /// 描画の解像度のまま(層とフレームテクスチャ)。
    pub(crate) const NATIVE: Xf = Xf { s: 1, ox: 0, oy: 0 };

    fn k(self) -> i64 {
        i64::from(RENDER_SCALE) * self.s
    }

    /// 論理座標の `r` を論理の `shift` だけ動かした範囲。
    pub(crate) fn area(self, r: Rect, shift: (i64, i64)) -> Area {
        let k = self.k();
        let (x, y) = (i64::from(r.x) + shift.0, i64::from(r.y) + shift.1);
        Area {
            x0: self.ox + x * k,
            y0: self.oy + y * k,
            x1: self.ox + (x + i64::from(r.w)) * k,
            y1: self.oy + (y + i64::from(r.h)) * k,
        }
    }

    /// 描き先の範囲 `b` を覆う論理座標の範囲。
    fn logical(self, b: Area) -> Area {
        let k = self.k();
        Area {
            x0: (b.x0 - self.ox).div_euclid(k),
            y0: (b.y0 - self.oy).div_euclid(k),
            x1: (b.x1 - self.ox + k - 1).div_euclid(k),
            y1: (b.y1 - self.oy + k - 1).div_euclid(k),
        }
    }
}

/// 1 回の描画の間の状態。失敗は最初の 1 つを覚え、以降の描画を飛ばす。
pub(crate) struct Drawer<'a, 'tc> {
    pub(crate) atlases: &'a mut [Texture<'tc>],
    pub(crate) sprites: &'a SpriteTable,
    pub(crate) batch: &'a mut QuadBatch,
    pub(crate) mode: BatchMode,
    /// `Clear` を描き先の `frame` の矩形の塗りにする(ウィンドウに直接描くとき。帯を塗らない)。
    pub(crate) clear_fill: bool,
    /// 描き先の切り取り(黒い帯のあるウィンドウではフレームの範囲)。層をその場で描いた後はここへ戻す。
    pub(crate) clip: Option<Rect>,
    /// ウィンドウの中のフレームの範囲(`clear_fill` のとき)。
    pub(crate) frame: Area,
    pub(crate) xf: Xf,
    /// 描き先の範囲(画素)。
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

    /// 黒い帯のあるウィンドウでは `clear` が帯まで塗るので、フレームの範囲をブレンドなしで塗る(14 §3.3.4)。帯がなければ `clear`。
    fn clear(&mut self, c: &mut Canvas<Window>, color: Color) {
        self.flush(c);
        if self.error.is_some() {
            return;
        }
        c.set_draw_color(sdl_color(color));
        if self.clear_fill {
            let Some(r) = self.frame.to_rect() else {
                return;
            };
            c.set_blend_mode(BlendMode::None);
            let r = c.fill_rect(sdl_rect(r));
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
        if dst.meet(self.bounds).to_rect().is_none() || self.error.is_some() {
            return;
        }
        let Some(dst) = dst.to_rect() else {
            return;
        };
        let entry = self.sprites.pick(id, u64::from(dst.w), u64::from(dst.h));
        debug_assert!(entry.is_some(), "範囲外のスプライト {id:?}");
        let Some(&entry) = entry else {
            self.bad_sprite = true;
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
                if let Err(e) = c.copy(tex, entry.src(), sdl_rect(dst)) {
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

    /// 論理座標の線。論理の画素の列を、描き先の画素の矩形にして描く。
    fn line(&mut self, c: &mut Canvas<Window>, from: (i64, i64), to: (i64, i64), color: Color) {
        let (xf, lb) = (self.xf, self.xf.logical(self.bounds));
        line_runs(from, to, lb, |r| self.fill(c, xf.area(r, (0, 0)), color));
    }

    /// 細かい座標の四角形。`quad_rows` の行を、同じ幅の続く行ごとに 1 つの矩形にして描く。
    fn quad(&mut self, c: &mut Canvas<Window>, pts: [(i64, i64); 4], color: Color) {
        let xf = self.xf;
        let pts = pts.map(|(x, y)| (xf.ox + x * xf.s, xf.oy + y * xf.s));
        let b = self.bounds;
        let mut run: Option<(i64, i64, i64, i64)> = None;
        let mut rows = Vec::new();
        quad_rows(pts, (b.x0, b.y0, b.x1, b.y1), |y, x0, x1| {
            run = match run {
                Some((a0, a1, top, bottom)) if a0 == x0 && a1 == x1 && bottom == y => {
                    Some((a0, a1, top, y + 1))
                }
                other => {
                    rows.extend(other);
                    Some((x0, x1, y, y + 1))
                }
            };
        });
        rows.extend(run);
        for (x0, x1, y0, y1) in rows {
            self.fill(c, Area { x0, y0, x1, y1 }, color);
        }
    }

    /// 層の区間の中身を描く。`place` があればその場描き(平行移動し、`Clear` を矩形の塗りにする)。
    pub(crate) fn exec_content(
        &mut self,
        c: &mut Canvas<Window>,
        cmds: &[DrawCmd],
        place: Option<Rect>,
    ) {
        let shift = place.map_or((0, 0), |r| (i64::from(r.x), i64::from(r.y)));
        let at = |x: i32, y: i32| (i64::from(x) + shift.0, i64::from(y) + shift.1);
        let sub = i64::from(SUBPX);
        for cmd in cmds {
            match *cmd {
                DrawCmd::Clear(color) => match place {
                    Some(r) => self.fill(c, self.xf.area(r, (0, 0)), color),
                    None => self.clear(c, color),
                },
                DrawCmd::FillRect { rect, color } => self.fill(c, self.xf.area(rect, shift), color),
                DrawCmd::Line {
                    x0,
                    y0,
                    x1,
                    y1,
                    color,
                } => self.line(c, at(x0, y0), at(x1, y1), color),
                DrawCmd::Sprite { id, dst, tint } => {
                    self.sprite(c, id, self.xf.area(dst, shift), tint)
                }
                DrawCmd::Quad { pts, color } => {
                    let pts = pts
                        .map(|(x, y)| (i64::from(x) + shift.0 * sub, i64::from(y) + shift.1 * sub));
                    self.quad(c, pts, color);
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
                            let dst = self.xf.area(*rect, (0, 0)).to_rect();
                            if let (Some(slot), Some(dst)) = (&slots[usize::from(layer.0)], dst)
                                && self.error.is_none()
                                && let Err(e) = c.copy(&slot.tex, None, sdl_rect(dst))
                            {
                                self.fail(e);
                            }
                        }
                        Some(LayerUse::Inline(section)) => {
                            let outer = self.bounds;
                            let area = self.xf.area(section.rect, (0, 0));
                            if let Some(clip) = area.meet(outer).to_rect() {
                                self.bounds = Area::of(clip);
                                c.set_clip_rect(sdl_rect(clip));
                                self.exec_content(
                                    c,
                                    &cmds[section.begin + 1..section.end],
                                    Some(section.rect),
                                );
                                self.flush(c);
                                c.set_clip_rect(self.clip.map(sdl_rect));
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

#[cfg(test)]
mod tests {
    use super::*;
    use tm_platform_api::{AtlasImage, SpriteAlt};

    static PX: [u8; 64 * 32 * 4] = [255; 64 * 32 * 4];
    static ATLAS: [AtlasImage; 1] = [AtlasImage {
        width: 64,
        height: 32,
        rgba: &PX,
    }];
    static RECTS: [SpriteRect; 2] = [
        SpriteRect {
            atlas: 0,
            x: 0,
            y: 0,
            w: 2,
            h: 2,
        },
        SpriteRect {
            atlas: 0,
            x: 16,
            y: 8,
            w: 6,
            h: 3,
        },
    ];
    static ALT: [SpriteAlt; 1] = [SpriteAlt {
        id: SpriteId(1),
        rect: SpriteRect {
            atlas: 0,
            x: 32,
            y: 16,
            w: 12,
            h: 6,
        },
    }];
    static BUNDLE: AssetBundle = AssetBundle {
        atlases: &ATLAS,
        sprites: &RECTS,
        alt: &ALT,
        sounds: &[],
        white: SpriteId(0),
    };

    #[test]
    fn uvs_are_the_texel_corners_of_the_rect() {
        let t = SpriteTable::new(&BUNDLE);
        let uv = |e: &SpriteEntry| (e.uv.u0, e.uv.v0, e.uv.u1, e.uv.v1);
        assert_eq!(
            uv(t.pick(SpriteId(1), 6, 3).unwrap()),
            (0.25, 0.25, 0.34375, 0.34375)
        );
        assert_eq!(
            uv(t.pick(SpriteId(1), 8, 4).unwrap()),
            (0.25, 0.25, 0.34375, 0.34375)
        );
        assert_eq!(
            uv(t.pick(SpriteId(1), 12, 6).unwrap()),
            (0.5, 0.5, 0.6875, 0.6875),
            "大きい版"
        );
        let w = t.white;
        assert_eq!(
            (w.uv.u0, w.uv.v0, w.uv.u1, w.uv.v1),
            (1.0 / 64.0, 1.0 / 32.0, 1.0 / 64.0, 1.0 / 32.0)
        );
        assert_eq!(t.pick(SpriteId(2), 1, 1), None);
    }

    /// SDL のソフトウェアの描画は `(int)(uv × 大きさ)` で画素に戻すので、アトラスの幅と高さは、どの行・列でも f32 で元の値に戻る大きさにする
    /// (2048・1536・1280。1408 などは戻らない行があり、1 行ずれた絵になる)。
    #[test]
    fn uv_round_trips_through_f32_for_every_row_of_the_atlas() {
        let exact =
            |size: u32| (0..=size).all(|v| ((v as f32 / size as f32) * size as f32) as u32 == v);
        for size in [2048, 1536, 1280] {
            assert!(exact(size), "{size}");
        }
        assert!(
            !exact(1408) && !exact(1344),
            "2 の冪の倍数でない高さはずれる行がある"
        );
        for page in tm_assets::bundle().unwrap().atlases {
            assert!(exact(page.width) && exact(page.height), "{page:?}");
        }
    }
}
