//! マップのカメラ(縦横。ズームしない)。注目点が表示域の端から 2 マスより内側に入るように追う。

use tm_core::GridPos;

use crate::layout::{Cam, TILE};

/// 注目点(カーソルなど)を、置いてよい範囲の端から何 px 内側に保つか。
const MARGIN: i32 = 2 * TILE;
/// 寄せる対象(案内の駅など)を、表示域の端から何 px 内側に入れるか。
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
    /// 注目点をなるべく置く縦の範囲(`top..bottom` から、さらに止める説明の箱の帯を除く)。注目点が箱に重ならなければよく、余白は取らない。
    /// 案内の対象と一緒に見せるためにだけ、この外(`top..bottom` の中)に出してよい。
    pub soft: (i32, i32),
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

    /// 縦の帯 `y0..y1` を、注目点をなるべく置く範囲からだけ除く(止める説明の箱)。
    pub(crate) fn without_soft_band(mut self, y0: i32, y1: i32) -> Viewport {
        self.soft = cut(self.soft, y0, y1);
        self
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

/// 2 つの範囲の共通部分。なければ `a`。
fn within(a: (i32, i32), b: (i32, i32)) -> (i32, i32) {
    let (lo, hi) = (a.0.max(b.0), a.1.min(b.1));
    if lo <= hi { (lo, hi) } else { a }
}

/// 2 つの範囲と `0..=max` の共通部分。
fn meet(a: (i32, i32), b: (i32, i32), max: i32) -> Option<(i32, i32)> {
    let lo = a.0.max(b.0).max(0);
    let hi = a.1.min(b.1).min(max);
    (lo <= hi).then_some((lo, hi))
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
    /// 注目点のマス `focus` がなるべく置く範囲の端から 2 マスより内側に入る、いまの目標にいちばん近い位置を目標にする。
    /// `hint`(案内の対象)は、`focus` を見せたまま表示域に入れられるときだけ入れる。なるべく置く範囲で入れられなければ、
    /// 必ず置く範囲(説明の箱の下も使う)で試す。
    pub(crate) fn follow(&mut self, focus: GridPos, hint: Option<GridPos>, vp: Viewport) {
        let fx = allowed(focus.x * TILE, (0, vp.w), MARGIN);
        let fy = within(
            allowed(focus.y * TILE, (vp.top, vp.bottom), MARGIN),
            allowed(focus.y * TILE, vp.soft, 0),
        );
        let mut t = Cam::at(
            self.target.x.clamp(fx.0, fx.1),
            self.target.y.clamp(fy.0, fy.1),
        );
        if let Some(h) = hint {
            // 一緒に見せるときは、注目点の余白も対象と同じ半マスまで縮める
            let max = vp.max();
            let rx = allowed(focus.x * TILE, (0, vp.w), HINT_MARGIN);
            let hx = allowed(h.x * TILE, (0, vp.w), HINT_MARGIN);
            let hy = allowed(h.y * TILE, (0, vp.h), HINT_MARGIN);
            let ry = allowed(focus.y * TILE, (vp.top, vp.bottom), HINT_MARGIN);
            let soft = within(ry, allowed(focus.y * TILE, vp.soft, 0));
            let fits = [soft, ry]
                .into_iter()
                .find_map(|ry| Some((meet(rx, hx, max.x)?, meet(ry, hy, max.y)?)));
            if let Some((ix, iy)) = fits {
                t = Cam::at(t.x.clamp(ix.0, ix.1), t.y.clamp(iy.0, iy.1));
            }
        }
        self.target = vp.clamp(t);
    }

    /// 注目点が不連続に変わったとき。注目点をなるべく置く範囲の中央に置く。
    pub(crate) fn jump(&mut self, focus: GridPos, vp: Viewport) {
        let c = Cam::at(
            focus.x * TILE + TILE / 2 - vp.w / 2,
            focus.y * TILE + TILE / 2 - (vp.soft.0 + vp.soft.1) / 2,
        );
        self.target = vp.clamp(c);
    }

    /// ゲームの画面を始めたとき。`tiles` の外接矩形の中心を表示域の中央に置き、`keep` のマスを見せたまますぐそこへ置く。
    /// `tiles` が空なら `keep` を中央に置く。
    pub(crate) fn start(
        &mut self,
        tiles: impl Iterator<Item = GridPos>,
        keep: GridPos,
        vp: Viewport,
    ) {
        let mut b: Option<(GridPos, GridPos)> = None;
        for t in tiles {
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
        let c = Cam::at(
            (lo.x + hi.x + 1) * TILE / 2 - vp.w / 2,
            (lo.y + hi.y + 1) * TILE / 2 - (vp.top + vp.bottom) / 2,
        );
        self.pos = vp.clamp(c);
        self.keep_visible(keep, vp);
        self.target = self.pos;
    }

    /// いまの位置を、マス `focus` が必ず置く範囲にすべて入るところまで寄せ、マップの範囲に収める
    /// (動きの途中やマップの端でもカーソルを欠けさせず、パネルを閉じて表示域が広がってもマップの外を見せない)。
    pub(crate) fn keep_visible(&mut self, focus: GridPos, vp: Viewport) {
        let (x0, x1) = allowed(focus.x * TILE, (0, vp.w), 0);
        let (y0, y1) = allowed(focus.y * TILE, (vp.top, vp.bottom), 0);
        self.pos = vp.clamp(Cam::at(self.pos.x.clamp(x0, x1), self.pos.y.clamp(y0, y1)));
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

    #[test]
    fn the_explanation_band_gives_way_only_to_a_hint_that_fits_beside_it() {
        let low = full().without_soft_band(MAP_H - 84, MAP_H);
        assert_eq!((low.top, low.bottom, low.soft), (0, MAP_H, (0, MAP_H - 84)));
        let start = || {
            let mut c = Camera::default();
            c.jump(g(20, 9), full());
            c.pos = c.target;
            c
        };
        let mut c = start();
        c.follow(g(20, 9), None, low);
        assert_eq!(c.target, start().target, "箱に重ならなければ動かない");
        let (_, y) = on_screen(g(20, 12), c.pos);
        assert!(y + TILE > MAP_H - 84, "前提: (20, 12) は箱の帯の中");
        c.follow(g(20, 12), None, low);
        settle(&mut c);
        let (_, y) = on_screen(g(20, 12), c.pos);
        assert_eq!(
            y + TILE,
            MAP_H - 84,
            "説明の箱に重ならないところまで(余白は取らない)"
        );
        let mut c = start();
        c.follow(g(20, 9), Some(g(20, 4)), low);
        settle(&mut c);
        let (_, y) = on_screen(g(20, 9), c.pos);
        let (_, hy) = on_screen(g(20, 4), c.pos);
        assert!(
            hy >= HINT_MARGIN && y + TILE <= MAP_H,
            "案内の駅とカーソルを両方見せる"
        );
        assert!(
            y + TILE > MAP_H - 84,
            "箱の帯の外では両方が入らないので、箱の帯を使う"
        );
        let hard = full().without_band(MAP_H - 84, MAP_H);
        let mut c = start();
        c.follow(g(20, 9), Some(g(20, 4)), hard);
        settle(&mut c);
        let (_, y) = on_screen(g(20, 9), c.pos);
        assert!(
            y + TILE <= MAP_H - 84,
            "確認ダイアログの帯には案内のためでも入れない"
        );
        let mut c = start();
        c.keep_visible(g(20, 9), low);
        assert_eq!(
            c.pos,
            start().pos,
            "欠けさせないための寄せは説明の箱の帯を見ない"
        );
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
