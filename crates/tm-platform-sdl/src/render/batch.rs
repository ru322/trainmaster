//! `render_geometry` に渡す頂点の束 [14 §3.3.2]。

use sdl2::pixels::Color as SdlColor;
use sdl2::rect::FPoint;
use sdl2::render::Vertex;
use tm_platform_api::{Color, Rect};
#[cfg(test)]
use tm_platform_api::{LOGICAL_HEIGHT, LOGICAL_WIDTH};

/// `u16` の添字で描ける四角形の数。
pub(crate) const MAX_QUADS: usize = 16_383;

/// アトラス上の範囲(テクスチャ座標)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Uv {
    pub(crate) u0: f32,
    pub(crate) v0: f32,
    pub(crate) u1: f32,
    pub(crate) v1: f32,
}

/// 同じアトラスのテクスチャ付きの四角形をためる。
pub(crate) struct QuadBatch {
    verts: Vec<Vertex>,
    indices: Vec<u16>,
    atlas: usize,
}

impl QuadBatch {
    pub(crate) fn new() -> Self {
        QuadBatch {
            verts: Vec::with_capacity(2048 * 4),
            indices: Vec::with_capacity(2048 * 6),
            atlas: 0,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }

    /// `atlas` の四角形を足す前にフラッシュが要るか。
    pub(crate) fn needs_flush(&self, atlas: usize) -> bool {
        !self.is_empty() && (self.atlas != atlas || self.verts.len() / 4 >= MAX_QUADS)
    }

    pub(crate) fn push(&mut self, atlas: usize, dst: Rect, uv: Uv, color: Color) {
        debug_assert!(!self.needs_flush(atlas));
        self.atlas = atlas;
        let base = self.verts.len() as u16;
        let (x0, y0) = (dst.x as f32, dst.y as f32);
        let (x1, y1) = (x0 + dst.w as f32, y0 + dst.h as f32);
        let color = SdlColor::RGBA(color.r, color.g, color.b, color.a);
        for (x, y, u, v) in [
            (x0, y0, uv.u0, uv.v0),
            (x1, y0, uv.u1, uv.v0),
            (x1, y1, uv.u1, uv.v1),
            (x0, y1, uv.u0, uv.v1),
        ] {
            self.verts.push(Vertex {
                position: FPoint::new(x, y),
                color,
                tex_coord: FPoint::new(u, v),
            });
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub(crate) fn atlas(&self) -> usize {
        self.atlas
    }

    pub(crate) fn vertices(&self) -> &[Vertex] {
        &self.verts
    }

    pub(crate) fn indices(&self) -> &[u16] {
        &self.indices
    }

    pub(crate) fn clear(&mut self) {
        self.verts.clear();
        self.indices.clear();
    }
}

/// 矩形(端を含まない。描き先の画素か論理座標)。平行移動や画面外の大きな値で溢れないよう i64 で持つ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Area {
    pub(crate) x0: i64,
    pub(crate) y0: i64,
    pub(crate) x1: i64,
    pub(crate) y1: i64,
}

/// 論理の画面(640×360)。
#[cfg(test)]
pub(crate) const SCREEN: Area = Area {
    x0: 0,
    y0: 0,
    x1: LOGICAL_WIDTH as i64,
    y1: LOGICAL_HEIGHT as i64,
};

impl Area {
    pub(crate) fn of(r: Rect) -> Area {
        let (x, y) = (i64::from(r.x), i64::from(r.y));
        Area {
            x0: x,
            y0: y,
            x1: x + i64::from(r.w),
            y1: y + i64::from(r.h),
        }
    }

    #[cfg(test)]
    pub(crate) fn shifted(self, dx: i64, dy: i64) -> Area {
        Area {
            x0: self.x0 + dx,
            y0: self.y0 + dy,
            x1: self.x1 + dx,
            y1: self.y1 + dy,
        }
    }

    pub(crate) fn meet(self, o: Area) -> Area {
        Area {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        }
    }

    /// 空でなく `Rect` で表せるなら `Rect`。
    pub(crate) fn to_rect(self) -> Option<Rect> {
        if self.x1 <= self.x0 || self.y1 <= self.y0 {
            return None;
        }
        Some(Rect {
            x: i32::try_from(self.x0).ok()?,
            y: i32::try_from(self.y0).ok()?,
            w: u32::try_from(self.x1 - self.x0).ok()?,
            h: u32::try_from(self.y1 - self.y0).ok()?,
        })
    }
}

/// 両端を含むブレゼンハムの線のうち `bounds` の中の画素を、同じ行・列に続くものごとの矩形にして `f` に渡す [14 §3.4]。
/// 主軸の i 番目の画素の副軸は `⌊(2·i·副 + 主) / (2·主)⌋`(SoftRaster と同じ式)。主軸の見える範囲だけを数える。
pub(crate) fn line_runs(from: (i64, i64), to: (i64, i64), bounds: Area, mut f: impl FnMut(Rect)) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let x_major = dx.abs() >= dy.abs();
    let (m0, k0, dm, dk, lo, hi, klo, khi) = if x_major {
        (
            from.0, from.1, dx, dy, bounds.x0, bounds.x1, bounds.y0, bounds.y1,
        )
    } else {
        (
            from.1, from.0, dy, dx, bounds.y0, bounds.y1, bounds.x0, bounds.x1,
        )
    };
    let (n, amin) = (dm.abs(), dk.abs());
    let (sm, sk) = (if dm < 0 { -1 } else { 1 }, if dk < 0 { -1 } else { 1 });
    let (first, last) = if sm > 0 {
        (lo - m0, hi - 1 - m0)
    } else {
        (m0 - hi + 1, m0 - lo)
    };
    let mut run: Option<(i64, i64, i64)> = None;
    let mut emit = |run: Option<(i64, i64, i64)>| {
        if let Some((m, len, k)) = run {
            let (a, b) = (if sm > 0 { m } else { m - len + 1 }, k);
            let (x, y, w, h) = if x_major {
                (a, b, len, 1)
            } else {
                (b, a, 1, len)
            };
            if let Some(r) = (Area {
                x0: x,
                y0: y,
                x1: x + w,
                y1: y + h,
            })
            .to_rect()
            {
                f(r);
            }
        }
    };
    for i in first.max(0)..=last.min(n) {
        let k = if n == 0 {
            k0
        } else {
            let q = (2 * i128::from(i) * i128::from(amin) + i128::from(n)) / (2 * i128::from(n));
            k0 + sk * q as i64
        };
        let m = m0 + sm * i;
        if !(klo..khi).contains(&k) {
            emit(run.take());
            continue;
        }
        run = match run {
            Some((start, len, rk)) if rk == k => Some((start, len + 1, rk)),
            other => {
                emit(other);
                Some((m, 1, k))
            }
        };
    }
    emit(run);
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: Area = Area {
        x0: -1000,
        y0: -1000,
        x1: 1000,
        y1: 1000,
    };

    fn runs(x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<Rect> {
        let mut v = Vec::new();
        line_runs((x0, y0), (x1, y1), WIDE, |r| v.push(r));
        v
    }

    fn pixels(rs: &[Rect]) -> Vec<(i32, i32)> {
        let mut p: Vec<(i32, i32)> = rs
            .iter()
            .flat_map(|r| {
                (0..r.h as i32).flat_map(move |j| (0..r.w as i32).map(move |i| (r.x + i, r.y + j)))
            })
            .collect();
        p.sort_unstable();
        p
    }

    #[test]
    fn horizontal_and_vertical_are_one_rect() {
        assert_eq!(
            runs(2, 3, 9, 3),
            [Rect {
                x: 2,
                y: 3,
                w: 8,
                h: 1
            }]
        );
        assert_eq!(
            runs(9, 3, 2, 3),
            [Rect {
                x: 2,
                y: 3,
                w: 8,
                h: 1
            }]
        );
        assert_eq!(
            runs(4, 1, 4, 6),
            [Rect {
                x: 4,
                y: 1,
                w: 1,
                h: 6
            }]
        );
        assert_eq!(
            runs(4, 6, 4, 1),
            [Rect {
                x: 4,
                y: 1,
                w: 1,
                h: 6
            }]
        );
        assert_eq!(
            runs(5, 5, 5, 5),
            [Rect {
                x: 5,
                y: 5,
                w: 1,
                h: 1
            }]
        );
    }

    #[test]
    fn diagonal_is_single_pixels() {
        let r = runs(0, 0, 3, 3);
        assert_eq!(r.len(), 4);
        assert_eq!(pixels(&r), [(0, 0), (1, 1), (2, 2), (3, 3)]);
    }

    #[test]
    fn shallow_line_merges_rows_and_includes_ends() {
        let r = runs(0, 0, 7, 2);
        assert_eq!(
            pixels(&r),
            [
                (0, 0),
                (1, 0),
                (2, 1),
                (3, 1),
                (4, 1),
                (5, 1),
                (6, 2),
                (7, 2)
            ]
        );
        assert_eq!(r.len(), 3);
        let back = runs(7, 2, 0, 0);
        assert_eq!(pixels(&back).len(), 8);
        assert_eq!(back.len(), 3);
    }

    #[test]
    fn eight_directions_cover_endpoints() {
        for (dx, dy) in [
            (5, 0),
            (5, 2),
            (5, 5),
            (2, 5),
            (0, 5),
            (-2, 5),
            (-5, 5),
            (-5, -2),
        ] {
            let p = pixels(&runs(10, 10, 10 + i64::from(dx), 10 + i64::from(dy)));
            assert!(
                p.contains(&(10, 10)) && p.contains(&(10 + dx, 10 + dy)),
                "{dx},{dy}"
            );
            assert_eq!(p.len() as i32, dx.abs().max(dy.abs()) + 1, "{dx},{dy}");
        }
    }

    #[test]
    fn matches_the_stepping_bresenham() {
        fn stepping(x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<(i32, i32)> {
            let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
            let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
            let (mut x, mut y, mut err) = (x0, y0, dx + dy);
            let mut out = vec![(x as i32, y as i32)];
            while (x, y) != (x1, y1) {
                let e2 = 2 * err;
                if e2 >= dy {
                    err += dy;
                    x += sx;
                }
                if e2 <= dx {
                    err += dx;
                    y += sy;
                }
                out.push((x as i32, y as i32));
            }
            out.sort_unstable();
            out
        }
        for dx in -23..=23 {
            for dy in -23..=23 {
                assert_eq!(
                    pixels(&runs(3, -4, 3 + dx, dy - 4)),
                    stepping(3, -4, 3 + dx, dy - 4)
                );
            }
        }
    }

    #[test]
    fn only_visible_pixels_of_long_lines_are_visited() {
        let screen = |a: (i64, i64), b: (i64, i64)| {
            let mut v = Vec::new();
            line_runs(a, b, SCREEN, |r| v.push(r));
            pixels(&v)
        };
        let far = i64::from(i32::MAX) * 2;
        let p = screen((-far, 10), (far, 10));
        assert_eq!(p.len(), 640);
        assert!(p.iter().all(|&(_, y)| y == 10));
        let p = screen((10, far), (12, -far));
        assert_eq!(p.len(), 360);
        assert!(screen((-far, -far), (-far + 5, far)).is_empty());
        let p = screen((-5, -5), (5, 5));
        assert_eq!(p, (0..=5).map(|i| (i, i)).collect::<Vec<_>>());
    }

    #[test]
    fn area_to_rect_rejects_empty_and_overflow() {
        let a = Area::of(Rect {
            x: -3,
            y: 4,
            w: 10,
            h: 2,
        });
        assert_eq!(
            a.meet(SCREEN).to_rect(),
            Some(Rect {
                x: 0,
                y: 4,
                w: 7,
                h: 2
            })
        );
        assert_eq!(a.shifted(-7, 0).meet(SCREEN).to_rect(), None);
        assert_eq!(a.shifted(i64::from(i32::MAX) + 10, 0).to_rect(), None);
    }

    #[test]
    fn batch_builds_quads() {
        let mut b = QuadBatch::new();
        let uv = Uv {
            u0: 0.25,
            v0: 0.5,
            u1: 0.75,
            v1: 1.0,
        };
        let c = Color {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        };
        b.push(
            1,
            Rect {
                x: 10,
                y: 20,
                w: 3,
                h: 4,
            },
            uv,
            c,
        );
        b.push(
            1,
            Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            uv,
            c,
        );
        assert_eq!(b.indices(), [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
        let v = b.vertices();
        assert_eq!((v[0].position.x, v[0].position.y), (10.0, 20.0));
        assert_eq!((v[2].position.x, v[2].position.y), (13.0, 24.0));
        assert_eq!((v[1].tex_coord.x, v[1].tex_coord.y), (0.75, 0.5));
        assert_eq!((v[3].tex_coord.x, v[3].tex_coord.y), (0.25, 1.0));
        assert_eq!(v[3].color, SdlColor::RGBA(1, 2, 3, 4));
        assert!(!b.needs_flush(1));
        assert!(b.needs_flush(0));
        b.clear();
        assert!(!b.needs_flush(0));
    }

    #[test]
    fn batch_is_flushed_before_u16_indices_overflow() {
        let mut b = QuadBatch::new();
        let uv = Uv {
            u0: 0.0,
            v0: 0.0,
            u1: 1.0,
            v1: 1.0,
        };
        let c = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        let one = Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        };
        for _ in 0..MAX_QUADS - 1 {
            b.push(0, one, uv, c);
        }
        assert!(!b.needs_flush(0));
        b.push(0, one, uv, c);
        assert!(b.needs_flush(0));
        assert_eq!(b.vertices().len(), MAX_QUADS * 4);
        assert_eq!(*b.indices().last().unwrap(), (MAX_QUADS * 4 - 1) as u16);
        assert!(MAX_QUADS * 4 - 1 <= usize::from(u16::MAX));
    }
}
