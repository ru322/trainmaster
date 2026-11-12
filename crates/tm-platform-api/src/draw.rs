//! 描画コマンド [10 §8.2]。

use crate::geom::{Color, Rect};

/// `AssetBundle::sprites` の添字。`0..SPRITE_COUNT` はスプライト、以降はグリフ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpriteId(pub u16);

/// キャッシュ層の番号。使えるのは `0..=3`(14 §3.5.2)。0 と 1 は静的なマップ層(マップを縦に切った 2 枚)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayerId(pub u8);

/// 描画コマンド 1 つ。座標は論理ピクセル(`Quad` だけは細かい座標)。プラットフォームは `RENDER_SCALE` 倍で描く。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrawCmd {
    /// 全体を塗る。
    Clear(Color),
    /// 矩形をアルファ合成で塗る。
    FillRect {
        /// 塗る範囲。
        rect: Rect,
        /// 色。
        color: Color,
    },
    /// 太さ 1 の線。
    Line {
        /// 始点の x。
        x0: i32,
        /// 始点の y。
        y0: i32,
        /// 終点の x。
        x1: i32,
        /// 終点の y。
        y1: i32,
        /// 色。
        color: Color,
    },
    /// スプライトを `dst` の大きさに最近傍で拡大縮小して描き、`tint` を乗算する。
    Sprite {
        /// スプライト。
        id: SpriteId,
        /// 描く範囲。
        dst: Rect,
        /// 乗算する色。
        tint: Color,
    },
    /// 凸の四角形をアルファ合成で塗る。頂点は細かい座標(論理 1px = `SUBPX`)。三角形は頂点を重ねる。
    /// 中心が内側にある画素を塗る(辺の上は左と上を含み、右と下を含まない。`quad_rows`)。
    Quad {
        /// 頂点(順は時計回りでも反時計回りでもよい)。
        pts: [(i32, i32); 4],
        /// 色。
        color: Color,
    },
    /// キャッシュ層の始まり。中身は層の座標(原点は層の左上)で書く(14 §3.5)。
    BeginCachedLayer {
        /// 層。
        layer: LayerId,
        /// 前回と同じ値で大きさも同じなら描き直さない。
        revision: u64,
        /// 層を画面に置く x。
        x: i32,
        /// 層を画面に置く y。
        y: i32,
        /// 層の幅。
        w: u32,
        /// 層の高さ。
        h: u32,
    },
    /// キャッシュ層の終わり。
    EndCachedLayer,
    /// 同じフレームの `BeginCachedLayer` の位置と大きさに層を描く。
    DrawCachedLayer {
        /// 層。
        layer: LayerId,
    },
}

/// 1 フレーム分の描画コマンド。上限は描画要素 2000 個(キャッシュ層は 1 個)[02 §9.2]。
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    /// コマンドの列。
    pub cmds: Vec<DrawCmd>,
}

/// 頂点の座標の絶対値の上限。これを超える座標は上限に寄せる(途中の積が i128 に収まる範囲)。
pub const QUAD_COORD_LIMIT: i64 = 1 << 36;

/// `Quad` が塗る画素を行ごとに `f(y, x0, x1)`(`x0..x1`)で渡す。座標は描画の画素(細かい座標と同じ)。
/// `clip`(`x0, y0, x1, y1`、端を含まない)の外は渡さない。SDL の描画と `SoftRaster` が同じ画素を塗るための共通の規則。
/// 頂点は `±QUAD_COORD_LIMIT` に寄せてから扱う(`i32` の細かい座標と層の平行移動は必ず範囲に入る)。
pub fn quad_rows(
    pts: [(i64, i64); 4],
    clip: (i64, i64, i64, i64),
    mut f: impl FnMut(i64, i64, i64),
) {
    let lim = |v: i64| i128::from(v.clamp(-QUAD_COORD_LIMIT, QUAD_COORD_LIMIT));
    let p = pts.map(|(x, y)| (2 * lim(x), 2 * lim(y)));
    let ymin = p.iter().map(|q| q.1).min().unwrap_or(0);
    let ymax = p.iter().map(|q| q.1).max().unwrap_or(0);
    let first = (ceil_div(ymin - 1, 2) as i64).max(clip.1);
    let last = (ceil_div(ymax - 1, 2) as i64).min(clip.3);
    for y in first..last {
        let yc = 2 * i128::from(y) + 1;
        let mut lo: Option<(i128, i128)> = None;
        let mut hi: Option<(i128, i128)> = None;
        for i in 0..4 {
            let (a, b) = (p[i], p[(i + 1) % 4]);
            if a.1 == b.1 || yc < a.1.min(b.1) || yc >= a.1.max(b.1) {
                continue;
            }
            let den = b.1 - a.1;
            let num = a.0 * den + (yc - a.1) * (b.0 - a.0);
            let x = if den < 0 { (-num, -den) } else { (num, den) };
            if lo.is_none_or(|l| x.0 * l.1 < l.0 * x.1) {
                lo = Some(x);
            }
            if hi.is_none_or(|h| h.0 * x.1 < x.0 * h.1) {
                hi = Some(x);
            }
        }
        let (Some(lo), Some(hi)) = (lo, hi) else {
            continue;
        };
        let start = clamp_i64(ceil_div(lo.0 - lo.1, 2 * lo.1)).max(clip.0);
        let end = clamp_i64(ceil_div(hi.0 - hi.1, 2 * hi.1)).min(clip.2);
        if start < end {
            f(y, start, end);
        }
    }
}

fn ceil_div(n: i128, d: i128) -> i128 {
    n.div_euclid(d) + i128::from(n.rem_euclid(d) != 0)
}

fn clamp_i64(v: i128) -> i64 {
    v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: (i64, i64, i64, i64) = (-1000, -1000, 1000, 1000);

    fn rows(pts: [(i64, i64); 4]) -> Vec<(i64, i64, i64)> {
        let mut v = Vec::new();
        quad_rows(pts, ALL, |y, a, b| v.push((y, a, b)));
        v
    }

    #[test]
    fn axis_aligned_rect_covers_exactly_its_pixels() {
        assert_eq!(
            rows([(2, 3), (5, 3), (5, 5), (2, 5)]),
            [(3, 2, 5), (4, 2, 5)]
        );
        assert_eq!(rows([(5, 5), (2, 5), (2, 3), (5, 3)]).len(), 2);
        assert!(rows([(2, 3), (2, 3), (2, 3), (2, 3)]).is_empty());
    }

    #[test]
    fn neighbours_sharing_an_edge_do_not_overlap_or_leave_gaps() {
        let a = [(0, 0), (10, 4), (10, 12), (0, 8)];
        let b = [(10, 4), (20, 0), (20, 8), (10, 12)];
        let c = [(0, 8), (10, 12), (10, 20), (0, 16)];
        let mut seen = std::collections::BTreeMap::new();
        for q in [a, b, c] {
            for (y, x0, x1) in rows(q) {
                for x in x0..x1 {
                    *seen.entry((x, y)).or_insert(0) += 1;
                }
            }
        }
        assert!(seen.values().all(|&n| n == 1));
        let (ra, rb) = (rows(a), rows(b));
        for &(y, _, end) in ra.iter().filter(|r| (4..12).contains(&r.0)) {
            if let Some(&(_, start, _)) = rb.iter().find(|r| r.0 == y) {
                assert_eq!(end, start, "y = {y} の継ぎ目");
            }
        }
        for y in 8..12 {
            assert!(seen.contains_key(&(0, y)) && seen.contains_key(&(9, y)));
        }
    }

    #[test]
    fn diagonal_band_has_one_row_per_pixel_and_constant_width() {
        let r = rows([(0, 0), (6, 0), (36, 30), (30, 30)]);
        assert_eq!(r.len(), 30);
        assert!(r.iter().all(|&(_, a, b)| b - a == 6));
        assert!(r.windows(2).all(|w| w[1].1 == w[0].1 + 1));
    }

    #[test]
    fn clip_limits_rows_and_columns() {
        let mut v = Vec::new();
        quad_rows(
            [(0, 0), (100, 0), (100, 100), (0, 100)],
            (10, 20, 15, 22),
            |y, a, b| v.push((y, a, b)),
        );
        assert_eq!(v, [(20, 10, 15), (21, 10, 15)]);
        let far = i64::from(i32::MAX) * 3;
        let mut huge = Vec::new();
        quad_rows(
            [
                (i64::MIN, i64::MIN),
                (i64::MAX, i64::MIN),
                (i64::MAX, i64::MAX),
                (i64::MIN, i64::MAX),
            ],
            (0, 0, 4, 2),
            |y, a, b| huge.push((y, a, b)),
        );
        assert_eq!(
            huge,
            [(0, 0, 4), (1, 0, 4)],
            "範囲外の座標は寄せて扱い、溢れない"
        );
        let mut n = 0;
        quad_rows(
            [(-far, -far), (far, -far), (far, far), (-far, far)],
            (0, 0, 4, 3),
            |_, a, b| n += b - a,
        );
        assert_eq!(n, 12);
    }
}
