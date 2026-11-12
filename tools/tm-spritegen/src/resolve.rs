//! 定義(RON)から、名前の付いた画像の一覧を作る。

use crate::art::{Art, Drawing};
use crate::def::{AnimDef, Op, Part, Proc, Source, SpriteDef, SpriteFile};
use crate::draw::{self, NINE_SUFFIXES};
use crate::error::SpriteError;
use crate::image::Image;
use crate::palette::Palette;
use std::collections::{BTreeMap, BTreeSet};

/// 名前が決まった 1 枚のスプライト。
#[derive(Debug, Clone)]
pub struct Sprite {
    /// 名前(フレームは `_f0` などを含む)。
    pub name: String,
    /// 設計の大きさ(px)。`SpriteMeta` の `w`・`h`。
    pub size: (u32, u32),
    /// UI が描く倍率(設計の 1px が論理の何 px か)。
    pub scale: u32,
    /// 絵の元。
    pub drawing: Drawing,
    /// アトラスに入れる画像(設計の 1px = 描画の倍率 × `scale` 画素)。
    pub image: Image,
    /// 2 倍に描くときの大きい版(設計の 1px = 描画の倍率 × `scale` × 2 画素)。
    pub alt: Option<Image>,
    /// 基準点(設計の px)。
    pub pivot: (i32, i32),
    /// tint 前提か。
    pub tint: bool,
    /// 基準点が切り出しで決まった(範囲の検査をしない)か。
    pub free_pivot: bool,
}

/// アニメーションの定義(フレームは `<name>_f0` から連続)。
#[derive(Debug, Clone)]
pub struct Anim {
    /// 定義の名前。
    pub name: String,
    /// フレーム数。
    pub frames: usize,
    /// 設定。
    pub anim: AnimDef,
}

/// 9 分割の定義。
#[derive(Debug, Clone)]
pub struct Nine {
    /// 定義の名前。
    pub name: String,
    /// 角の大きさ。
    pub corner: u32,
}

/// 定義を解決した結果(名前順でも採番順でもない)。
#[derive(Debug, Clone, Default)]
pub struct Resolved {
    /// スプライト。
    pub sprites: Vec<Sprite>,
    /// アニメーション。
    pub anims: Vec<Anim>,
    /// 9 分割。
    pub nines: Vec<Nine>,
    /// 線路と帯の半区間(`TrackHalf`)のマスの大きさ(設計の px)。なければ `None`。
    pub track_tile: Option<u32>,
}

#[derive(Clone)]
struct Rendered {
    frames: Vec<Drawing>,
    pivot: (i32, i32),
    tint: bool,
    free: bool,
}

#[derive(Clone, Copy)]
struct Hint {
    pivot: (i32, i32),
    tint: bool,
    free: bool,
}

struct Ctx<'a> {
    pal: &'a Palette,
    defs: BTreeMap<String, (u32, u32, SpriteDef)>,
    cache: BTreeMap<String, Rendered>,
    stack: Vec<String>,
}

fn syntax(name: &str, message: &str) -> SpriteError {
    SpriteError::Syntax {
        file: name.into(),
        message: message.into(),
    }
}

/// 定義ファイルの一覧(ファイル名順)を解決し、SG-E01〜E06 を検査する。`render_scale` は論理 1px の描画の画素数。
pub fn resolve(
    files: &[(String, SpriteFile)],
    pal: &Palette,
    render_scale: u32,
) -> Result<Resolved, SpriteError> {
    let mut defs = BTreeMap::new();
    for (file, f) in files {
        if f.tile < 8 || f.tile % 2 != 0 {
            return Err(syntax(file, "tile は 8 以上の偶数"));
        }
        let scale = f.scale.unwrap_or(1);
        if !(1..=4).contains(&scale) {
            return Err(syntax(file, "scale は 1〜4"));
        }
        for d in &f.sprites {
            crate::check_name(&d.name)?;
            if defs
                .insert(d.name.clone(), (f.tile, scale, d.clone()))
                .is_some()
            {
                return Err(SpriteError::Name {
                    name: d.name.clone(),
                    reason: "定義が重複している".into(),
                });
            }
        }
    }
    let mut ctx = Ctx {
        pal,
        defs,
        cache: BTreeMap::new(),
        stack: Vec::new(),
    };
    let names: Vec<String> = ctx.defs.keys().cloned().collect();
    let mut out = Resolved::default();
    for (tile, _, def) in ctx.defs.values() {
        if matches!(def.source, Some(Source::Proc(Proc::TrackHalf { .. }))) {
            match out.track_tile {
                Some(t) if t != *tile => {
                    return Err(SpriteError::Shape {
                        sprite: def.name.clone(),
                        reason: format!("TrackHalf の tile が {t} と {tile} でそろっていない"),
                    });
                }
                _ => out.track_tile = Some(*tile),
            }
        }
    }
    let mut seen = BTreeSet::new();
    for name in names {
        let (_, file_scale, def) = ctx.defs[&name].clone();
        let scale = def.scale.unwrap_or(file_scale);
        if !(1..=4).contains(&scale) {
            return Err(syntax(&name, "scale は 1〜4"));
        }
        let level = |d: &Drawing| -> (Image, Option<Image>) {
            let r = render_scale * scale;
            (
                d.render(r),
                def.alt.unwrap_or(false).then(|| d.render(2 * r)),
            )
        };
        if let (
            Some(Source::Proc(Proc::NineSlice {
                corner,
                border_w,
                fill,
                border,
                outline,
            })),
            true,
        ) = (&def.source, def.frames.is_empty())
        {
            let colors = [
                pal.by_name(fill, &name)?,
                pal.by_name(border, &name)?,
                pal.by_name(outline, &name)?,
            ];
            let parts = draw::nine_slice(*corner, *border_w, colors, &name)?;
            for (suffix, v) in NINE_SUFFIXES.iter().zip(parts) {
                let drawing = Drawing::new(Art::Vector(v));
                let (image, alt) = level(&drawing);
                add(
                    &mut out,
                    &mut seen,
                    Sprite {
                        name: format!("{name}_{suffix}"),
                        size: drawing.size(),
                        scale,
                        drawing,
                        image,
                        alt,
                        pivot: def.pivot.unwrap_or((0, 0)),
                        tint: def.tint.unwrap_or(false),
                        free_pivot: false,
                    },
                )?;
            }
            out.nines.push(Nine {
                name: name.clone(),
                corner: *corner,
            });
            continue;
        }
        let r = ctx.render_def(&name)?;
        match (r.frames.len(), def.anim) {
            (1, None) => add(
                &mut out,
                &mut seen,
                Sprite {
                    name: name.clone(),
                    size: r.frames[0].size(),
                    scale,
                    image: level(&r.frames[0]).0,
                    alt: level(&r.frames[0]).1,
                    drawing: r.frames[0].clone(),
                    pivot: r.pivot,
                    tint: r.tint,
                    free_pivot: r.free,
                },
            )?,
            (1, Some(_)) => return Err(syntax(&name, "フレームが 1 つなのに anim がある")),
            (_, None) => return Err(syntax(&name, "フレームが複数なのに anim がない")),
            (n, Some(anim)) => {
                out.anims.push(Anim {
                    name: name.clone(),
                    frames: n,
                    anim,
                });
                for (i, d) in r.frames.iter().enumerate() {
                    let (image, alt) = level(d);
                    add(
                        &mut out,
                        &mut seen,
                        Sprite {
                            name: format!("{name}_f{i}"),
                            size: d.size(),
                            scale,
                            drawing: d.clone(),
                            image,
                            alt,
                            pivot: r.pivot,
                            tint: r.tint,
                            free_pivot: r.free,
                        },
                    )?;
                }
            }
        }
    }
    Ok(out)
}

fn add(out: &mut Resolved, seen: &mut BTreeSet<String>, sprite: Sprite) -> Result<(), SpriteError> {
    crate::check_name(&sprite.name)?;
    let (w, h) = sprite.size;
    let (px, py) = sprite.pivot;
    if !sprite.free_pivot && (px < 0 || py < 0 || px > w as i32 || py > h as i32) {
        return Err(SpriteError::Pivot {
            sprite: sprite.name,
            pivot: sprite.pivot,
            size: (w, h),
        });
    }
    if !seen.insert(sprite.name.clone()) {
        return Err(SpriteError::Name {
            name: sprite.name,
            reason: "名前が重複している".into(),
        });
    }
    out.sprites.push(sprite);
    Ok(())
}

impl Ctx<'_> {
    fn render_def(&mut self, name: &str) -> Result<Rendered, SpriteError> {
        if let Some(r) = self.cache.get(name) {
            return Ok(r.clone());
        }
        if self.stack.iter().any(|s| s == name) {
            return Err(SpriteError::Derive {
                sprite: name.into(),
                reason: "Derive が循環している".into(),
            });
        }
        let (tile, _, def) = self.defs[name].clone();
        self.stack.push(name.to_string());
        let r = self.render_inner(tile, &def);
        self.stack.pop();
        let r = r?;
        self.cache.insert(name.to_string(), r.clone());
        Ok(r)
    }

    fn render_inner(&mut self, tile: u32, def: &SpriteDef) -> Result<Rendered, SpriteError> {
        let name = &def.name;
        let (frames, inherited) = match (&def.source, def.frames.is_empty()) {
            (Some(src), true) => self.render_source(tile, name, src)?,
            (None, false) => {
                let mut frames = Vec::new();
                for src in &def.frames {
                    let (mut imgs, _) = self.render_source(tile, name, src)?;
                    if imgs.len() != 1 {
                        return Err(SpriteError::Shape {
                            sprite: name.clone(),
                            reason: "frames の各要素は 1 枚でなければならない".into(),
                        });
                    }
                    frames.append(&mut imgs);
                }
                (frames, None)
            }
            _ => return Err(syntax(name, "source と frames のどちらか一方だけを書く")),
        };
        let size = frames[0].size();
        if frames.iter().any(|f| f.size() != size) {
            return Err(SpriteError::Shape {
                sprite: name.clone(),
                reason: "フレームの寸法がそろっていない".into(),
            });
        }
        let hint = inherited.unwrap_or(Hint {
            pivot: (0, 0),
            tint: false,
            free: false,
        });
        Ok(Rendered {
            frames,
            pivot: def.pivot.unwrap_or(hint.pivot),
            tint: def.tint.unwrap_or(hint.tint),
            free: hint.free && def.pivot.is_none(),
        })
    }

    fn render_source(
        &mut self,
        tile: u32,
        name: &str,
        src: &Source,
    ) -> Result<(Vec<Drawing>, Option<Hint>), SpriteError> {
        match src {
            Source::Pixels(rows) => Ok((
                vec![Drawing::new(Art::Grid(self.pixels(name, rows)?))],
                None,
            )),
            Source::Derive { from, op } => {
                if !self.defs.contains_key(from) {
                    return Err(SpriteError::Derive {
                        sprite: name.into(),
                        reason: format!("元 `{from}` がない"),
                    });
                }
                if matches!(
                    self.defs[from].2.source,
                    Some(Source::Proc(Proc::NineSlice { .. }))
                ) {
                    return Err(SpriteError::Derive {
                        sprite: name.into(),
                        reason: format!("元 `{from}` は 9 分割で、1 枚ではない"),
                    });
                }
                let base = self.render_def(from)?;
                let (w, h) = base.frames[0].size();
                let (w, h) = (w as i32, h as i32);
                let (px, py) = base.pivot;
                let flip = |x, y| base.frames.iter().map(|f| f.flipped(x, y)).collect();
                let (images, pivot) = match op {
                    Op::FlipX => (flip(true, false), (w - px, py)),
                    Op::FlipY => (flip(false, true), (px, h - py)),
                    Op::Rot180 => (flip(true, true), (w - px, h - py)),
                    Op::Rotate(n) => {
                        if w != h {
                            return Err(SpriteError::Derive {
                                sprite: name.into(),
                                reason: "回す絵は正方形でなければならない".into(),
                            });
                        }
                        let mut out = Vec::new();
                        for f in &base.frames {
                            out.push(f.rotated(*n).map_err(|reason| SpriteError::Derive {
                                sprite: name.into(),
                                reason: reason.into(),
                            })?);
                        }
                        let mut p = (px, py);
                        if n % 2 == 1 && (2 * px != w || 2 * py != h) {
                            return Err(SpriteError::Derive {
                                sprite: name.into(),
                                reason: "45 度で回す絵の基準点は中心でなければならない".into(),
                            });
                        }
                        for _ in 0..(n / 2) % 4 {
                            p = (p.1, w - p.0);
                        }
                        (out, p)
                    }
                    Op::Recolor(pairs) => {
                        let mut map = Vec::new();
                        for &(a, b) in pairs {
                            map.push((self.pal.by_symbol(a, name)?, self.pal.by_symbol(b, name)?));
                        }
                        (
                            base.frames.iter().map(|f| f.recolor(&map)).collect(),
                            (px, py),
                        )
                    }
                };
                Ok((
                    images,
                    Some(Hint {
                        pivot,
                        tint: base.tint,
                        free: base.free,
                    }),
                ))
            }
            Source::Proc(Proc::TrackHalf { dir, part }) => {
                let (v, pivot) = draw::track_half(tile, *dir, *part, self.pal, name)?;
                let tint = matches!(part, Part::BandFill { .. });
                Ok((
                    vec![Drawing::new(Art::Vector(v))],
                    Some(Hint {
                        pivot,
                        tint,
                        free: true,
                    }),
                ))
            }
            Source::Proc(p) => Ok((
                vec![Drawing::new(Art::Vector(self.proc(tile, name, p)?))],
                None,
            )),
            Source::Shapes { w, h, paint } => Ok((
                vec![Drawing::new(Art::Vector(draw::shapes(
                    *w, *h, paint, self.pal, name,
                )?))],
                None,
            )),
        }
    }

    fn pixels(&self, name: &str, rows: &[String]) -> Result<Image, SpriteError> {
        let w = rows.first().map_or(0, |r| r.chars().count());
        if w == 0 {
            return Err(SpriteError::Shape {
                sprite: name.into(),
                reason: "ドット絵が空".into(),
            });
        }
        let mut img = Image::new(w as u32, rows.len() as u32);
        for (y, row) in rows.iter().enumerate() {
            if row.chars().count() != w {
                return Err(SpriteError::Shape {
                    sprite: name.into(),
                    reason: format!("{} 行目の長さが {w} でない", y + 1),
                });
            }
            for (x, ch) in row.chars().enumerate() {
                img.px[y * w + x] = self.pal.by_symbol(ch, name)?;
            }
        }
        Ok(img)
    }

    fn proc(&self, tile: u32, name: &str, p: &Proc) -> Result<crate::vector::Vector, SpriteError> {
        match p {
            Proc::TrackHalf { .. } => Err(syntax(name, "TrackHalf は単独の source にだけ書ける")),
            Proc::CornerFrame => draw::corner_frame(tile, self.pal, name),
            Proc::Fill { w, h, color } => {
                if *w == 0 || *h == 0 {
                    return Err(syntax(name, "Fill の寸法が 0"));
                }
                Ok(draw::fill(*w, *h, self.pal.by_name(color, name)?))
            }
            Proc::NineSlice { .. } => Err(syntax(name, "NineSlice は単独の source にだけ書ける")),
        }
    }
}
