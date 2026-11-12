//! マップのカメラ(縦横。ズームしない)。注目点が表示域の端から 2 マスより内側に入るように追う。

use tm_core::GridPos;

use crate::layout::{Cam, RING, TILE};

/// 注目点(カーソルなど)を、必ず置く範囲(表示域から確認ダイアログの帯を除いたもの)の端から何 px 内側に保つか。
const MARGIN: i32 = 2 * TILE;
/// 注目点を、止める説明の箱の帯から何 px 離すか(1 マス動かしたとき、箱に入る前にカメラが動き出す)。
const SOFT_MARGIN: i32 = TILE / 2;
/// 寄せる対象(案内の駅など)を、表示域の端と説明の箱から何 px 内側に入れるか(入らなければ枠の分の `RING` まで縮める)。
const HINT_MARGIN: i32 = TILE / 2;

/// カメラが見せる場面。座標は表示域の左上を原点にした px。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Viewport {
    /// 表示域の幅(パネルを開くと狭くなる)。
    pub w: i32,
    /// 表示域の高さ。
    pub h: i32,
    /// 注目点を必ず置く縦の範囲 `top..bottom`(路線の確認ダイアログの帯を除く)。
    pub top: i32,
    /// 同じく下端。
    pub bottom: i32,
    /// 横が説明の箱にかかる注目点を、箱に隠さない縦の範囲(`top..bottom` から、さらに止める説明の箱の帯を除く)。案内の対象も同じ。
    /// 必ず置く範囲の 2 マスの余白と両立しなければ、余白を縮めてこちらを守る。
    pub soft: (i32, i32),
    /// 説明の箱の横の範囲 `x0..x1`(なければ空)。マスの絵がここにかかるときだけ `soft` を使う。
    pub soft_x: (i32, i32),
    /// マップ全体の幅(層の座標)。
    pub world_w: i32,
    /// マップ全体の高さ。
    pub world_h: i32,
}

impl Viewport {
    /// 幅 `w`・高さ `h` の表示域で、大きさ `world` のマップを見る。
    pub(crate) fn new(w: i32, h: i32, world: (i32, i32)) -> Viewport {
        Viewport {
            w,
            h,
            top: 0,
            bottom: h,
            soft: (0, h),
            soft_x: (0, 0),
            world_w: world.0,
            world_h: world.1,
        }
    }

    /// 縦の帯 `y0..y1`(表示域の座標)を、注目点を必ず置く範囲(となるべく置く範囲)から除く。上下の広く残る側を使う。
    pub(crate) fn without_band(mut self, y0: i32, y1: i32) -> Viewport {
        (self.top, self.bottom) = cut((self.top, self.bottom), y0, y1);
        self.soft = cut(
            (self.soft.0.max(self.top), self.soft.1.min(self.bottom)),
            y0,
            y1,
        );
        self
    }

    /// 横 `x0..x1`・縦 `y0..y1` の止める説明の箱を、横がそこにかかる注目点を置く範囲から除く。
    pub(crate) fn without_soft_band(mut self, (x0, x1): (i32, i32), y0: i32, y1: i32) -> Viewport {
        self.soft = cut(self.soft, y0, y1);
        self.soft_x = (x0, x1);
        self
    }

    /// 表示域の座標で左端 `x` のマスを置いてよい縦の範囲(絵が説明の箱の横にかかれば `soft`)。
    fn safe_y(&self, x: i32) -> (i32, i32) {
        let (x0, x1) = self.soft_x;
        if x - RING < x1 && x0 < x + TILE + RING {
            self.soft
        } else {
            (self.top, self.bottom)
        }
    }

    fn max(&self) -> Cam {
        Cam::at(
            (self.world_w - self.w).max(0),
            (self.world_h - self.h).max(0),
        )
    }

    /// カメラをマップの範囲(0〜max(0, 全体 − 表示域))に収める。
    pub(crate) fn clamp(&self, c: Cam) -> Cam {
        let m = self.max();
        Cam::at(c.x.clamp(0, m.x), c.y.clamp(0, m.y))
    }
}

/// 範囲 `r` から帯 `y0..y1` を除き、上下の広く残る側を返す。
fn cut(r: (i32, i32), y0: i32, y1: i32) -> (i32, i32) {
    if y1 <= r.0 || y0 >= r.1 {
        r
    } else if y0 - r.0 >= r.1 - y1 {
        (r.0, y0.max(r.0))
    } else {
        (y1.min(r.1), r.1)
    }
}

/// 1 つの軸で、`lo..lo + TILE` のマスを `safe.0..safe.1` の余白 `margin` の内側に置けるカメラの範囲(両端を含む)。
/// 余白が取れなければ縮め、範囲そのものに入らなければ中央に置く 1 点にする。
fn allowed(lo: i32, safe: (i32, i32), margin: i32) -> (i32, i32) {
    let room = safe.1 - safe.0 - TILE;
    if room < 0 {
        let c = lo + TILE / 2 - (safe.0 + safe.1) / 2;
        return (c, c);
    }
    let m = margin.min(room / 2);
    (lo + TILE - safe.1 + m, lo - safe.0 - m)
}

/// 2 つの範囲の共通部分。なければ `b`(余白の `a` より、説明の箱に隠さない `b` を守る)。
fn within(a: (i32, i32), b: (i32, i32)) -> (i32, i32) {
    let (lo, hi) = (a.0.max(b.0), a.1.min(b.1));
    if lo <= hi { (lo, hi) } else { b }
}

/// 2 つの範囲と `0..=max` の共通部分。
fn meet(a: (i32, i32), b: (i32, i32), max: i32) -> Option<(i32, i32)> {
    let lo = a.0.max(b.0).max(0);
    let hi = a.1.min(b.1).min(max);
    (lo <= hi).then_some((lo, hi))
}

/// 横を `x` に置いたとき、マス `focus` を必ず置く範囲の端から 2 マスと、説明の箱から半マス離す縦の範囲。
fn focus_y(focus: GridPos, x: i32, vp: &Viewport) -> (i32, i32) {
    within(
        allowed(focus.y * TILE, (vp.top, vp.bottom), MARGIN),
        allowed(focus.y * TILE, vp.safe_y(focus.x * TILE - x), SOFT_MARGIN),
    )
}

/// 横を `x` の近くに置き、`focus` と案内の対象 `h` を一緒に、どちらも説明の箱に隠さずに表示域に入れる横の位置と縦の範囲。
/// 余白はまず半マス、入らなければカーソルの絵の分(`RING`)。それでも入らなければ `None`。
fn with_hint(focus: GridPos, h: GridPos, x: i32, vp: &Viewport) -> Option<(i32, (i32, i32))> {
    let max = vp.max();
    [HINT_MARGIN, RING].into_iter().find_map(|m| {
        let ix = meet(
            allowed(focus.x * TILE, (0, vp.w), m),
            allowed(h.x * TILE, (0, vp.w), m),
            max.x,
        )?;
        let x = x.clamp(ix.0, ix.1);
        let ry = allowed(focus.y * TILE, vp.safe_y(focus.x * TILE - x), m);
        let hy = allowed(h.y * TILE, vp.safe_y(h.x * TILE - x), m);
        meet(ry, hy, max.y).map(|iy| (x, iy))
    })
}

/// 目標の横が `x` のとき `follow` が選ぶ横の位置と、縦を収める範囲と、`hint` を一緒に入れたか。
fn aim(focus: GridPos, hint: Option<GridPos>, x: i32, vp: &Viewport) -> (i32, (i32, i32), bool) {
    let fx = allowed(focus.x * TILE, (0, vp.w), MARGIN);
    let x = x.clamp(fx.0, fx.1);
    match hint.and_then(|h| with_hint(focus, h, x, vp)) {
        Some((hx, iy)) => (hx, within(focus_y(focus, hx, vp), iy), true),
        None => (x, focus_y(focus, x, vp), false),
    }
}

/// いまの位置と目標の位置。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Camera {
    /// いまの位置(描くのに使う)。
    pub pos: Cam,
    /// 目標の位置。
    pub target: Cam,
}

impl Camera {
    /// 注目点のマス `focus` が、必ず置く範囲の端から 2 マスより内側で、説明の箱から半マス離れる、いまの目標にいちばん近い位置を目標にする。
    /// `hint`(案内の対象)は、`focus` と一緒に、どちらも説明の箱に隠さずに表示域に入れられるときだけ入れる(入れられなければ縁の矢印で示す)。
    /// 一緒に入れるときの余白は、どちらも半マス、入らなければ枠の分。
    pub(crate) fn follow(&mut self, focus: GridPos, hint: Option<GridPos>, vp: Viewport) {
        let (x, ry, _) = aim(focus, hint, self.target.x, &vp);
        self.target = vp.clamp(Cam::at(x, self.target.y.clamp(ry.0, ry.1)));
    }

    /// 説明の箱を新しく置いたとき。目標を横に `reach` px、縦に 3 マス以内で動かして、`stations` のうちいま全部見えているものを
    /// 全部見せたまま、横が箱にかかるものを箱(`soft` の外)から出す。一部だけ見えているものは、見えている部分を減らさない。
    /// `follow` が `focus` を追ってそのまま止まる位置(`focus` は箱の外。`hint` を一緒に見せられるなら見せる位置)だけを探す。
    /// 見つかれば、動かす量(横と縦の和)がいちばん小さい位置へ目標を移して true。
    pub(crate) fn clear_box(
        &mut self,
        stations: &[GridPos],
        focus: GridPos,
        hint: Option<GridPos>,
        reach: i32,
        vp: Viewport,
    ) -> bool {
        let t = self.target;
        let max = vp.max();
        let screen = |p: GridPos, c: Cam| (p.x * TILE - c.x, p.y * TILE - c.y);
        let inside = |(x, y): (i32, i32)| x >= 0 && x + TILE <= vp.w && y >= 0 && y + TILE <= vp.h;
        let cut = |(x, y): (i32, i32)| x < vp.w && x + TILE > 0 && y < vp.h && y + TILE > 0;
        let (mut full, mut part) = (Vec::new(), Vec::new());
        for &p in stations {
            let s = screen(p, t);
            if inside(s) {
                full.push(p);
            } else if cut(s) {
                part.push(s);
            }
        }
        // 一部だけ見えているマスは、はみ出している側へカメラを動かさない(片側の範囲)
        let keep = |lo: i32, size: i32, view: i32| {
            if lo < 0 {
                (i32::MIN, 0)
            } else if lo + size > view {
                (0, i32::MAX)
            } else {
                (i32::MIN, i32::MAX)
            }
        };
        let (mut kx, mut ky) = ((i32::MIN, i32::MAX), (i32::MIN, i32::MAX));
        for &(x, y) in &part {
            let (a, b) = (keep(x, TILE, vp.w), keep(y, TILE, vp.h));
            kx = (kx.0.max(a.0), kx.1.min(a.1));
            ky = (ky.0.max(b.0), ky.1.min(b.1));
        }
        let hinted = aim(focus, hint, t.x, &vp).2;
        let mut best: Option<(i32, Cam)> = None;
        for d in (0..=reach).flat_map(|d| [-d, d]) {
            if best.is_some_and(|(cost, _)| d.abs() > cost) {
                break;
            }
            let x = t.x + d;
            if !(0..=max.x).contains(&x) || !(kx.0..=kx.1).contains(&d) {
                continue;
            }
            // follow が横を動かさず、案内の対象を見せていたら見せたままの位置だけ
            let (ax, ry, h) = aim(focus, hint, x, &vp);
            if ax.clamp(0, max.x) != x || hinted && !h {
                continue;
            }
            let ry = (ry.0.clamp(0, max.y), ry.1.clamp(0, max.y));
            let mut lo = ry.0.max(t.y - 3 * TILE).max(t.y.saturating_add(ky.0));
            let mut hi = ry.1.min(t.y + 3 * TILE).min(t.y.saturating_add(ky.1));
            for &p in &full {
                let (sx, _) = screen(p, Cam::at(x, 0));
                if sx < 0 || sx + TILE > vp.w {
                    lo = i32::MAX;
                    break;
                }
                let (top, bottom) = vp.safe_y(sx);
                lo = lo.max(p.y * TILE + TILE - bottom);
                hi = hi.min(p.y * TILE - top);
            }
            if lo > hi {
                continue;
            }
            let y = t.y.clamp(lo, hi);
            let cost = d.abs() + (y - t.y).abs();
            if best.is_none_or(|(c, _)| cost < c) {
                best = Some((cost, Cam::at(x, y)));
            }
        }
        if let Some((_, c)) = best {
            self.target = c;
        }
        best.is_some()
    }

    /// 注目点が不連続に変わったとき。注目点をなるべく置く範囲の中央に置く。
    pub(crate) fn jump(&mut self, focus: GridPos, vp: Viewport) {
        let c = Cam::at(
            focus.x * TILE + TILE / 2 - vp.w / 2,
            focus.y * TILE + TILE / 2 - (vp.soft.0 + vp.soft.1) / 2,
        );
        self.target = vp.clamp(c);
    }

    /// ゲームの画面を始めたとき。`tiles` の外接矩形の中心を表示域の中央に置き(入りきらなければ `tiles` の 1 つ目をすべて見せ)、
    /// `keep` のマスを追ったときに止まる位置へすぐ置く(始めの数フレームでカメラが動かない)。`tiles` が空なら `keep` を中央に置く。
    pub(crate) fn start(
        &mut self,
        tiles: impl Iterator<Item = GridPos>,
        keep: GridPos,
        vp: Viewport,
    ) {
        let mut b: Option<(GridPos, GridPos)> = None;
        let mut first = None;
        for t in tiles {
            first.get_or_insert(t);
            b = Some(b.map_or((t, t), |(lo, hi)| {
                (
                    GridPos {
                        x: lo.x.min(t.x),
                        y: lo.y.min(t.y),
                    },
                    GridPos {
                        x: hi.x.max(t.x),
                        y: hi.y.max(t.y),
                    },
                )
            }));
        }
        let (lo, hi) = b.unwrap_or((keep, keep));
        let mut c = Cam::at(
            (lo.x + hi.x + 1) * TILE / 2 - vp.w / 2,
            (lo.y + hi.y + 1) * TILE / 2 - (vp.top + vp.bottom) / 2,
        );
        if let Some(f) = first {
            let (x0, x1) = allowed(f.x * TILE, (0, vp.w), 0);
            let (y0, y1) = allowed(f.y * TILE, (vp.top, vp.bottom), 0);
            c = Cam::at(c.x.clamp(x0, x1), c.y.clamp(y0, y1));
        }
        self.target = vp.clamp(c);
        self.follow(keep, None, vp);
        self.pos = self.target;
        self.keep_visible(keep, vp);
        self.target = self.pos;
    }

    /// いまの位置を、マス `focus` が説明の箱に隠れない範囲(`soft`)にすべて入るところまで寄せ、マップの範囲に収める
    /// (動きの途中やマップの端でもカーソルを欠けさせず、パネルを閉じて表示域が広がってもマップの外を見せない)。
    pub(crate) fn keep_visible(&mut self, focus: GridPos, vp: Viewport) {
        let (x0, x1) = allowed(focus.x * TILE, (0, vp.w), 0);
        let x = self.pos.x.clamp(x0, x1);
        let (y0, y1) = allowed(focus.y * TILE, vp.safe_y(focus.x * TILE - x), 0);
        self.pos = vp.clamp(Cam::at(x, self.pos.y.clamp(y0, y1)));
    }

    /// 1 フレーム進める。軸ごとに `pos += (target − pos) / 4`(最低 1px)。
    pub(crate) fn step(&mut self) {
        let ease = |p: &mut i32, t: i32| {
            let d = t - *p;
            if d != 0 {
                let s = d / 4;
                *p += if s == 0 { d.signum() } else { s };
            }
        };
        ease(&mut self.pos.x, self.target.x);
        ease(&mut self.pos.y, self.target.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{MAP_H, MAP_W, MAP_W_PANEL, WORLD_H, WORLD_W};

    fn g(x: i32, y: i32) -> GridPos {
        GridPos { x, y }
    }

    fn full() -> Viewport {
        Viewport::new(MAP_W, MAP_H, (WORLD_W, WORLD_H))
    }

    fn panel() -> Viewport {
        Viewport::new(MAP_W_PANEL, MAP_H, (WORLD_W, WORLD_H))
    }

    fn settle(c: &mut Camera) -> u32 {
        let mut n = 0;
        while c.pos != c.target {
            c.step();
            n += 1;
            assert!(n < 100);
        }
        n
    }

    /// 注目点のマスの、表示域の中の左上(カメラ `c`)。
    fn on_screen(p: GridPos, c: Cam) -> (i32, i32) {
        (p.x * TILE - c.x, p.y * TILE - c.y)
    }

    fn inside_margin(p: GridPos, c: Cam, vp: Viewport) -> bool {
        let (x, y) = on_screen(p, c);
        x >= MARGIN
            && x + TILE <= vp.w - MARGIN
            && y >= vp.top + MARGIN
            && y + TILE <= vp.bottom - MARGIN
    }

    #[test]
    fn follows_on_both_axes_with_two_tiles_of_margin() {
        let vp = full();
        let mut c = Camera::default();
        let f = g(30, 14);
        c.follow(f, None, vp);
        assert_eq!(
            c.target,
            Cam::at(
                (f.x + 1) * TILE + MARGIN - vp.w,
                (f.y + 1) * TILE + MARGIN - vp.h
            ),
            "右下へ動くと、右と下に 2 マスの余白"
        );
        assert_eq!(c.target, Cam::at(416, 256), "2 マス = 64px");
        settle(&mut c);
        assert!(inside_margin(f, c.pos, vp));
        let t = c.target;
        c.follow(g(f.x - 1, f.y - 1), None, vp);
        assert_eq!(c.target, t, "余白の内側で動いてもカメラは動かない");
        c.follow(g(10, 3), None, vp);
        assert_eq!(
            c.target,
            Cam::at(10 * TILE - MARGIN, 3 * TILE - MARGIN),
            "左上へ"
        );
        c.follow(g(0, 0), None, vp);
        assert_eq!(c.target, Cam::default(), "端では 0 で止まる");
    }

    #[test]
    fn range_is_0_to_the_hidden_size() {
        let mut c = Camera::default();
        c.follow(g(39, 17), None, full());
        assert_eq!(
            c.target,
            Cam::at(WORLD_W - MAP_W, WORLD_H - MAP_H),
            "右下の端"
        );
        assert_eq!(c.target, Cam::at(640, 288));
        c.follow(g(39, 17), None, panel());
        assert_eq!(
            c.target.x,
            WORLD_W - MAP_W_PANEL,
            "パネルを開くと右へ 864 まで"
        );
        c.jump(g(0, 0), panel());
        assert_eq!(c.target, Cam::default());
        c.jump(g(20, 9), full());
        assert_eq!(
            c.target,
            Cam::at(
                20 * TILE + TILE / 2 - MAP_W / 2,
                9 * TILE + TILE / 2 - MAP_H / 2
            ),
            "中央に置く"
        );
        let small = Viewport::new(MAP_W, MAP_H, (16 * TILE, 8 * TILE));
        c.follow(g(15, 7), None, small);
        assert_eq!(
            c.target,
            Cam::default(),
            "表示域より小さいマップでは動かない"
        );
    }

    #[test]
    fn opening_and_closing_the_panel_keeps_the_focus_visible() {
        let mut c = Camera::default();
        let f = g(18, 9);
        c.follow(f, None, full());
        settle(&mut c);
        let (x, _) = on_screen(f, c.pos);
        assert!(x + TILE > MAP_W_PANEL, "前提: パネルの下に隠れる位置");
        c.follow(f, None, panel());
        settle(&mut c);
        assert!(
            inside_margin(f, c.pos, panel()),
            "パネルを開くとカーソルを出す"
        );
        c.follow(g(39, 9), None, panel());
        settle(&mut c);
        assert_eq!(c.pos.x, WORLD_W - MAP_W_PANEL);
        c.keep_visible(g(39, 9), full());
        assert_eq!(c.pos.x, WORLD_W - MAP_W, "閉じたらすぐマップの範囲に収める");
        c.follow(g(39, 9), None, full());
        assert_eq!(c.target.x, WORLD_W - MAP_W);
    }

    #[test]
    fn a_dialog_band_pushes_the_focus_out_from_under_it() {
        let vp = full().without_band(0, 84);
        assert_eq!((vp.top, vp.bottom), (84, MAP_H), "上の帯");
        let mut c = Camera::default();
        c.jump(g(20, 9), full());
        c.pos = c.target;
        let f = g(20, 6);
        c.follow(f, None, vp);
        settle(&mut c);
        let (_, y) = on_screen(f, c.pos);
        assert!(y >= 84 + MARGIN, "帯の下に 2 マス: {y}");
        let low = full().without_band(MAP_H - 84, MAP_H);
        assert_eq!((low.top, low.bottom), (0, MAP_H - 84), "下の帯");
        assert_eq!(
            full().without_band(-10, -1),
            full(),
            "表示域の外の帯は除かない"
        );
        c.follow(g(20, 0), None, vp);
        assert_eq!(
            c.target.y, 0,
            "マップの上端では帯の下に出せないが範囲は守る"
        );
    }

    /// 横 `x0..x1`、下 84px の説明の箱。
    fn low_box(x0: i32, x1: i32) -> Viewport {
        full().without_soft_band((x0, x1), MAP_H - 84, MAP_H)
    }

    #[test]
    fn the_explanation_box_keeps_the_cursor_out_from_under_it() {
        let lowb = low_box(0, MAP_W);
        assert_eq!(
            (lowb.top, lowb.bottom, lowb.soft),
            (0, MAP_H, (0, MAP_H - 84))
        );
        let start = || {
            let mut c = Camera::default();
            c.jump(g(20, 9), full());
            c.pos = c.target;
            c
        };
        let mut c = start();
        c.follow(g(20, 9), None, lowb);
        assert_eq!(
            c.target,
            start().target,
            "箱から半マス以上離れていれば動かない"
        );
        let (_, y) = on_screen(g(20, 12), c.pos);
        assert!(y + TILE > MAP_H - 84, "前提: (20, 12) は箱の帯の中");
        c.follow(g(20, 12), None, lowb);
        settle(&mut c);
        let (_, y) = on_screen(g(20, 12), c.pos);
        assert_eq!(y + TILE, 188, "箱(表示域の 204px から)から半マス(16px)離す");

        let (x, y) = on_screen(g(20, 11), start().pos);
        assert!(
            y + TILE > MAP_H - 84 && y + TILE <= MAP_H - MARGIN,
            "前提: (20, 11) は 2 マスの余白の内側だが箱の帯の中"
        );
        let beside = low_box(0, x - RING);
        let mut c = start();
        c.follow(g(20, 11), None, beside);
        assert_eq!(c.target, start().target, "横が箱にかからなければ動かない");
        let touching = low_box(0, x - RING + 1);
        let mut c = start();
        c.follow(g(20, 11), None, touching);
        assert_ne!(
            c.target,
            start().target,
            "カーソルの絵が 1px でもかかれば動く"
        );

        let mut c = start();
        c.keep_visible(g(20, 11), lowb);
        let (_, y) = on_screen(g(20, 11), c.pos);
        assert_eq!(
            y + TILE,
            MAP_H - 84,
            "動きの途中でも箱に隠さない(すぐ寄せる)"
        );
    }

    #[test]
    fn a_hint_that_does_not_fit_beside_the_box_is_left_to_the_edge_arrow() {
        let lowb = low_box(0, MAP_W);
        let start = || {
            let mut c = Camera::default();
            c.jump(g(20, 9), full());
            c.pos = c.target;
            c
        };
        let mut alone = start();
        alone.follow(g(20, 9), None, lowb);
        let mut c = start();
        c.follow(g(20, 9), Some(g(20, 3)), lowb);
        assert_eq!(
            c.target, alone.target,
            "箱の外に一緒に入らなければ案内の駅に寄せない"
        );
        let (_, y) = on_screen(g(20, 9), c.target);
        assert!(y + TILE <= MAP_H - 84, "カーソルは箱に隠さない");
        let mut c = start();
        c.follow(g(20, 9), Some(g(20, 6)), lowb);
        settle(&mut c);
        let (_, y) = on_screen(g(20, 9), c.pos);
        let (_, hy) = on_screen(g(20, 6), c.pos);
        assert!(
            hy >= HINT_MARGIN && y + TILE <= MAP_H - 84,
            "入るなら両方を箱の外に"
        );
        let mut c = start();
        c.follow(g(20, 9), Some(g(20, 13)), lowb);
        settle(&mut c);
        let (_, hy) = on_screen(g(20, 13), c.pos);
        assert!(
            hy + TILE <= MAP_H - 84 || hy + TILE > MAP_H,
            "案内の駅も箱の下には入れない: {hy}"
        );
    }

    #[test]
    fn when_the_margin_and_the_box_clash_the_box_wins() {
        let tight = full().without_soft_band((0, MAP_W), 100, MAP_H);
        let mut c = Camera::default();
        c.jump(g(20, 9), full());
        c.pos = c.target;
        c.follow(g(20, 9), None, tight);
        settle(&mut c);
        let (_, y) = on_screen(g(20, 9), c.pos);
        assert!(
            y >= 0 && y + TILE <= 100,
            "2 マスの余白より箱に隠さないことを守る: {y}"
        );
    }

    #[test]
    fn the_dialog_band_is_never_used_even_for_a_hint() {
        let hard = full().without_band(MAP_H - 84, MAP_H);
        let mut c = Camera::default();
        c.jump(g(20, 9), full());
        c.pos = c.target;
        c.follow(g(20, 9), Some(g(20, 4)), hard);
        settle(&mut c);
        let (_, y) = on_screen(g(20, 9), c.pos);
        assert!(y + TILE <= MAP_H - 84, "確認ダイアログの帯には入れない");
    }

    fn inside_half_tile(p: GridPos, c: Cam, vp: Viewport) -> bool {
        let (x, y) = on_screen(p, c);
        x >= HINT_MARGIN
            && x + TILE <= vp.w - HINT_MARGIN
            && y >= HINT_MARGIN
            && y + TILE <= vp.h - HINT_MARGIN
    }

    #[test]
    fn a_hint_is_shown_only_when_the_focus_stays_in_view() {
        let vp = full();
        let start = || {
            let mut c = Camera::default();
            c.jump(g(20, 9), vp);
            c.pos = c.target;
            c
        };
        let mut c = start();
        assert!(
            !inside_half_tile(g(5, 3), c.pos, vp),
            "前提: 案内の駅は見えていない"
        );
        c.follow(g(20, 9), Some(g(5, 3)), vp);
        settle(&mut c);
        assert!(
            inside_half_tile(g(20, 9), c.pos, vp),
            "カーソルは見えたまま"
        );
        assert!(inside_half_tile(g(5, 3), c.pos, vp), "案内の駅も見える");
        assert_eq!(on_screen(g(5, 3), c.pos), (16, 16), "左上から半マス(16px)");
        let t = c.target;
        c.follow(g(20, 9), Some(g(5, 3)), vp);
        assert_eq!(c.target, t, "寄せた後は動かない");
        c.follow(g(20, 9), None, vp);
        assert!(
            inside_margin(g(20, 9), c.target, vp),
            "案内が消えたら余白を戻す"
        );

        let mut alone = start();
        alone.follow(g(20, 9), None, vp);
        for far in [g(39, 0), g(0, 17)] {
            let mut c = start();
            c.follow(g(20, 9), Some(far), vp);
            assert_eq!(
                c.target, alone.target,
                "一緒に見せられない {far:?} には寄せない(端の矢印で示す)"
            );
        }
    }

    #[test]
    fn keep_visible_moves_only_as_far_as_needed() {
        let vp = full();
        let mut c = Camera::default();
        c.follow(g(39, 17), None, vp);
        c.step();
        let (x, y) = on_screen(g(39, 17), c.pos);
        assert!(
            x + TILE > vp.w && y + TILE > vp.h,
            "前提: 動きの途中で欠ける"
        );
        c.keep_visible(g(39, 17), vp);
        assert_eq!(
            on_screen(g(39, 17), c.pos),
            (vp.w - TILE, vp.h - TILE),
            "ちょうど見える所まで"
        );
        let before = c.pos;
        c.keep_visible(g(30, 14), vp);
        assert_eq!(c.pos, before, "見えていれば動かさない");
        c.step();
        assert_eq!(c.pos, c.target, "目標へはそのまま進む");
    }

    /// 横 `x0..x1`、縦 `y0` から下の端までの説明の箱。
    fn box_from(x0: i32, x1: i32, y0: i32) -> Viewport {
        full().without_soft_band((x0, x1), y0, MAP_H)
    }

    fn still(x: i32, y: i32) -> Camera {
        Camera {
            pos: Cam::at(x, y),
            target: Cam::at(x, y),
        }
    }

    #[test]
    fn clear_box_takes_a_station_out_from_under_the_box_with_the_least_move() {
        let wide = box_from(0, MAP_W, MAP_H - 84);
        let mut c = still(320, 144);
        assert_eq!(on_screen(g(20, 11), c.pos), (320, 208), "前提: 箱の帯の中");
        assert!(c.clear_box(&[g(20, 11)], g(20, 8), None, 3 * TILE, wide));
        assert_eq!(c.target, Cam::at(320, 180), "縦に 36px(1 マスより多く)");
        assert_eq!(c.pos, Cam::at(320, 144), "動かすのは目標だけ");

        let narrow = box_from(200, 440, MAP_H - 84);
        let mut c = still(320, 144);
        assert!(c.clear_box(&[g(23, 11)], g(20, 8), None, 3 * TILE, narrow));
        assert_eq!(
            c.target,
            Cam::at(290, 144),
            "縦の 36px より横の 30px が近い(箱の右へ出す)"
        );
        let mut c = still(320, 144);
        assert!(c.clear_box(&[g(23, 11)], g(20, 8), None, 0, narrow));
        assert_eq!(c.target, Cam::at(320, 180), "横に動かせなければ縦");

        let tall = box_from(200, MAP_W, 150);
        let mut c = still(320, 96);
        assert_eq!(on_screen(g(20, 10), c.pos), (320, 224));
        assert!(
            !c.clear_box(&[g(20, 10)], g(12, 9), None, 3 * TILE, tall),
            "縦に 106px(3 マスより多く)動かさないと出せない"
        );
        assert_eq!(c.target, Cam::at(320, 96), "出せなければ動かさない");

        let high = full().without_soft_band((0, MAP_W), 0, 84);
        let mut c = still(320, 144);
        assert_eq!(
            on_screen(g(20, 6), c.pos),
            (320, 48),
            "前提: 上の箱の帯の中"
        );
        assert!(c.clear_box(&[g(20, 6)], g(20, 9), None, 3 * TILE, high));
        assert_eq!(c.target, Cam::at(320, 108), "上の箱なら上へ 36px");
    }

    #[test]
    fn clear_box_does_not_push_a_half_seen_station_out_sideways_or_downward() {
        let narrow = box_from(200, 440, MAP_H - 84);
        let mut c = still(330, 144);
        assert_eq!(
            on_screen(g(10, 6), c.pos),
            (-10, 48),
            "前提: 左が欠けて見える"
        );
        assert_eq!(
            on_screen(g(16, 11), c.pos),
            (182, 208),
            "前提: 箱の左の端の下"
        );
        let mut alone = c;
        assert!(alone.clear_box(&[g(16, 11)], g(20, 8), None, 3 * TILE, narrow));
        assert_eq!(
            alone.target,
            Cam::at(350, 144),
            "前提: その駅がなければ右へ 20px"
        );
        assert!(c.clear_box(&[g(10, 6), g(16, 11)], g(20, 8), None, 3 * TILE, narrow));
        assert_eq!(
            c.target,
            Cam::at(330, 180),
            "左が欠けた駅を押し出さず、下へ"
        );

        let high = full().without_soft_band((0, MAP_W), 0, 84);
        let mut c = still(320, 144);
        assert_eq!(
            on_screen(g(25, 13), c.pos),
            (480, 272),
            "前提: 下が欠けて見える"
        );
        assert!(
            !c.clear_box(&[g(20, 6), g(25, 13)], g(20, 9), None, 3 * TILE, high),
            "上へ動かすと下が欠けた駅をさらに押し出す"
        );
        assert_eq!(c.target, Cam::at(320, 144));
    }

    #[test]
    fn clear_box_keeps_the_cursor_and_half_seen_stations_in_view() {
        let wide = box_from(0, MAP_W, MAP_H - 84);
        let mut c = still(320, 150);
        assert_eq!(
            on_screen(g(15, 4), c.pos),
            (160, -22),
            "前提: 上が欠けて見える"
        );
        assert!(
            !c.clear_box(&[g(15, 4), g(20, 11)], g(20, 8), None, 3 * TILE, wide),
            "一部だけ見えている駅をさらに外へ押し出さない"
        );
        assert_eq!(c.target, Cam::at(320, 150));
        assert!(c.clear_box(&[g(20, 11)], g(20, 8), None, 3 * TILE, wide));
        assert_eq!(
            c.target,
            Cam::at(320, 180),
            "前提: その駅がなければ下へ 30px"
        );

        let mut c = still(320, 150);
        assert!(
            !c.clear_box(&[g(20, 11)], g(20, 6), None, 3 * TILE, wide),
            "カーソルを 2 マスの余白から出すなら動かさない"
        );
        let mut c = still(320, 144);
        assert!(c.clear_box(&[g(20, 8)], g(20, 11), None, 3 * TILE, wide));
        let (_, y) = on_screen(g(20, 11), c.target);
        assert!(
            y + TILE <= MAP_H - 84 - SOFT_MARGIN,
            "カーソルも箱から出す: {y}"
        );
    }

    #[test]
    fn clear_box_does_not_lose_the_hint_that_follow_shows() {
        let vp = box_from(0, MAP_W, 194);
        let (hint, station, cursor) = (g(15, 4), g(20, 9), g(20, 7));
        let mut c = still(320, 112);
        c.follow(cursor, Some(hint), vp);
        assert_eq!(
            c.target,
            Cam::at(320, 112),
            "前提: 案内の駅を一緒に見せている"
        );
        assert_eq!(on_screen(station, c.pos), (320, 176), "前提: 箱の帯の中");
        assert!(
            !c.clear_box(&[hint, station], cursor, Some(hint), 3 * TILE, vp),
            "下へ 14px 動かすと案内の駅が余白(6px)から出る"
        );
        assert_eq!(c.target, Cam::at(320, 112));
        assert!(c.clear_box(&[hint, station], cursor, None, 3 * TILE, vp));
        assert_eq!(c.target, Cam::at(320, 126), "前提: 案内がなければ動かせる");
    }

    #[test]
    fn with_a_hint_the_cursor_keeps_its_margin_when_both_fit() {
        let mut c = still(320, 60);
        assert_eq!(
            on_screen(g(20, 9), c.pos),
            (320, 228),
            "前提: カーソルは下の余白の中"
        );
        c.follow(g(20, 9), Some(g(20, 5)), full());
        assert_eq!(
            c.target.y, 96,
            "案内の駅を入れたまま、カーソルを下から 2 マスまで上げる"
        );
        let (_, hy) = on_screen(g(20, 5), c.target);
        assert!(hy >= HINT_MARGIN, "案内の駅も見える: {hy}");
    }

    #[test]
    fn a_hint_two_pixels_short_of_the_half_tile_is_shown_with_the_ring_margin() {
        let vp = box_from(0, MAP_W, 222);
        let mut c = still(320, 134);
        let (cursor, hint) = (g(20, 9), g(18, 4));
        assert_eq!(on_screen(hint, c.pos), (256, -6), "前提: 上が 6px 欠ける");
        c.follow(cursor, Some(hint), vp);
        assert_eq!(c.target.y, 122, "案内の駅を枠の分(6px)だけ内側に入れる");
        let (_, y) = on_screen(cursor, c.target);
        assert!(y + TILE <= 222, "カーソルは箱に隠さない: {y}");
        let mut alone = still(320, 134);
        alone.follow(cursor, None, vp);
        assert_eq!(alone.target.y, 134, "前提: 案内がなければ動かない");
    }

    #[test]
    fn eases_in_about_a_third_of_a_second_and_is_deterministic() {
        let run = || {
            let mut c = Camera::default();
            c.follow(g(39, 17), None, full());
            let mut trace = Vec::new();
            for _ in 0..30 {
                c.step();
                trace.push(c.pos);
            }
            trace
        };
        let a = run();
        assert_eq!(a, run(), "同じ入力なら同じ動き");
        assert_eq!(a[0], Cam::at(640 / 4, 288 / 4));
        let n = a.iter().position(|&p| p == Cam::at(640, 288)).unwrap() + 1;
        assert!(n <= 24, "{n} フレーム");
        let mut c = Camera::default();
        c.follow(g(17, 4), None, full());
        assert_eq!(c.target, Cam::default(), "前提: 右の余白のちょうど内側");
        c.follow(g(18, 4), None, full());
        assert_eq!(c.target, Cam::at(TILE, 0), "1 マス出たら 1 マス動く");
        let n = settle(&mut c);
        assert!(n <= 16, "1 マスの追従は {n} フレーム");
    }
}
