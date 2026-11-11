//! `DrawCmd` の組み立てと描画要素の予算 [13 §3.11、§9]。

pub(crate) mod map;
pub(crate) mod palette;

use tm_assets::sprite::{self, NineSlice};
use tm_platform_api::{Color, DrawCmd, LayerId, Rect, SpriteId};

use crate::layout::MAP_SCALE;
use crate::text::TextBlock;

/// UI の描画要素の上限 [UI-D-25]。キャッシュ層は中身によらず 1 個と数える。
pub(crate) const UI_DRAW_BUDGET: usize = 1500;

/// 予算を超えたときに省くまとまり。宣言順に先に省く [13 §9]。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shed {
    /// 収入のポップアップ。
    Popup,
    /// 待っている人数の札。
    PeopleTag,
    /// 紙ふぶき。
    Confetti,
    /// トースト。
    Toast,
}

const SHED_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
struct GroupStat {
    groups: u32,
    min_size: u32,
    total: u32,
}

#[derive(Clone, Copy, Debug)]
struct OpenGroup {
    kind: usize,
    start: usize,
    skip: bool,
}

/// 1 フレームの描画の集計。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DrawStats {
    /// 予算に数えた描画要素の数(省いた後)。
    pub counted: usize,
    /// 省く前の数。
    pub wanted: usize,
    /// 省いたまとまりの数(`Shed` の順)。
    pub dropped: [u32; SHED_COUNT],
    /// 省く前の、種類ごとの描画要素の数(`Shed` の順)。
    pub totals: [u32; SHED_COUNT],
}

/// 描画コマンドの書き手。数えるだけの 1 回目と、省く数を決めて書く 2 回目に使う。
pub(crate) struct DrawBuilder<'a> {
    out: Option<&'a mut Vec<DrawCmd>>,
    counted: usize,
    group: Option<OpenGroup>,
    stats: [GroupStat; SHED_COUNT],
    seen: [u32; SHED_COUNT],
    drop: [u32; SHED_COUNT],
}

impl<'a> DrawBuilder<'a> {
    /// 数えるだけの書き手。
    pub(crate) fn counting() -> DrawBuilder<'static> {
        DrawBuilder {
            out: None,
            counted: 0,
            group: None,
            stats: [GroupStat::default(); SHED_COUNT],
            seen: [0; SHED_COUNT],
            drop: [0; SHED_COUNT],
        }
    }

    /// `drop[k]` 個ずつ、種類 k のまとまりを先頭から省いて書く書き手。
    pub(crate) fn emitting(out: &'a mut Vec<DrawCmd>, drop: [u32; SHED_COUNT]) -> Self {
        DrawBuilder {
            out: Some(out),
            counted: 0,
            group: None,
            stats: [GroupStat::default(); SHED_COUNT],
            seen: [0; SHED_COUNT],
            drop,
        }
    }

    /// 数えた結果から、上限に収めるために省くまとまりの数を決める。
    pub(crate) fn plan(&self, limit: usize) -> [u32; SHED_COUNT] {
        let mut over = self.counted.saturating_sub(limit);
        let mut drop = [0; SHED_COUNT];
        for (k, st) in self.stats.iter().enumerate() {
            if over == 0 {
                break;
            }
            if st.groups == 0 {
                continue;
            }
            // 最小のまとまりの大きさで割るので、省いた後は必ず上限以下になる
            let size = st.min_size.max(1) as usize;
            let n = over.div_ceil(size).min(st.groups as usize);
            drop[k] = n as u32;
            let removed = if n == st.groups as usize {
                st.total as usize
            } else {
                n * size
            };
            over = over.saturating_sub(removed);
        }
        drop
    }

    /// 数えた描画要素の数(テスト用)。
    #[cfg(test)]
    pub(crate) fn counted(&self) -> usize {
        self.counted
    }

    /// 省いたまとまりの数。
    pub(crate) fn dropped(&self) -> [u32; SHED_COUNT] {
        std::array::from_fn(|k| self.seen[k].min(self.drop[k]))
    }

    /// 省けるまとまりを描く。予算を超えたときは種類ごとに先に描いたものから省く。
    pub(crate) fn group(&mut self, kind: Shed, f: impl FnOnce(&mut Self)) {
        debug_assert!(self.group.is_none(), "まとまりは入れ子にしない");
        let k = kind as usize;
        let skip = self.seen[k] < self.drop[k];
        self.seen[k] += 1;
        self.group = Some(OpenGroup {
            kind: k,
            start: self.counted,
            skip,
        });
        f(self);
        let g = self.group.take().expect("開いたまとまり");
        let size = (self.counted - g.start) as u32;
        let st = &mut self.stats[g.kind];
        st.min_size = if st.groups == 0 {
            size
        } else {
            st.min_size.min(size)
        };
        st.groups += 1;
        st.total += size;
    }

    /// 1 つ書く。
    pub(crate) fn push(&mut self, cmd: DrawCmd) {
        if self.group.is_some_and(|g| g.skip) {
            return;
        }
        self.counted += 1;
        if let Some(out) = self.out.as_deref_mut() {
            out.push(cmd);
        }
    }

    /// キャッシュ層を置く。中身は数えず、層を 1 個と数える(02 §9.2)。
    pub(crate) fn cached_layer(
        &mut self,
        layer: LayerId,
        revision: u64,
        at: Rect,
        content: &[DrawCmd],
    ) {
        if let Some(out) = self.out.as_deref_mut() {
            out.push(DrawCmd::BeginCachedLayer {
                layer,
                revision,
                x: at.x,
                y: at.y,
                w: at.w,
                h: at.h,
            });
            out.extend_from_slice(content);
            out.push(DrawCmd::EndCachedLayer);
        }
        self.push(DrawCmd::DrawCachedLayer { layer });
    }

    /// 全体を塗る。
    pub(crate) fn clear(&mut self, color: Color) {
        self.push(DrawCmd::Clear(color));
    }

    /// 矩形を塗る。
    pub(crate) fn fill(&mut self, rect: Rect, color: Color) {
        self.push(DrawCmd::FillRect { rect, color });
    }

    /// スプライトを基準の位置 `(x, y)` に置く(`dst = 基準 − pivot`)。
    pub(crate) fn sprite(&mut self, id: SpriteId, x: i32, y: i32, tint: Color) {
        self.push(sprite_cmd(id, x, y, tint));
    }

    /// マップの絵を `MAP_SCALE` 倍で基準の位置 `(x, y)` に置く。
    pub(crate) fn map_sprite(&mut self, id: SpriteId, x: i32, y: i32, tint: Color) {
        self.push(map_sprite_cmd(id, x, y, tint));
    }

    /// テキストのブロックを左上 `(x, y)` に置く。ボタンの絵は色を変えず、不透明度だけ合わせる。
    pub(crate) fn text(&mut self, block: &TextBlock, x: i32, y: i32, color: Color) {
        let plain = palette::with_alpha(palette::WHITE, color.a);
        for g in &block.glyphs {
            self.push(DrawCmd::Sprite {
                id: g.sprite,
                dst: Rect {
                    x: x + i32::from(g.x),
                    y: y + i32::from(g.y),
                    w: u32::from(g.w),
                    h: u32::from(g.h),
                },
                tint: if g.plain { plain } else { color },
            });
        }
    }

    /// 9 分割の枠を `rect` に描く。
    pub(crate) fn nine_slice(&mut self, ns: &NineSlice, rect: Rect, tint: Color) {
        self.nine_slice_scaled(ns, rect, tint, 1);
    }

    /// 9 分割の枠を、角と縁の太さを `scale` 倍にして `rect` に描く(マップの上の枠)。
    pub(crate) fn nine_slice_scaled(
        &mut self,
        ns: &NineSlice,
        rect: Rect,
        tint: Color,
        scale: i32,
    ) {
        let c = i32::from(ns.corner) * scale;
        let (w, h) = (rect.w as i32, rect.h as i32);
        let xs = [0, c, w - c];
        let ws = [c, w - 2 * c, c];
        let ys = [0, c, h - c];
        let hs = [c, h - 2 * c, c];
        for row in 0..3 {
            for col in 0..3 {
                if ws[col] <= 0 || hs[row] <= 0 {
                    continue;
                }
                self.push(DrawCmd::Sprite {
                    id: ns.parts[row * 3 + col],
                    dst: Rect {
                        x: rect.x + xs[col],
                        y: rect.y + ys[row],
                        w: ws[col] as u32,
                        h: hs[row] as u32,
                    },
                    tint,
                });
            }
        }
    }

    /// 集計。`counter` は 1 回目(数えるだけ)の書き手。
    pub(crate) fn stats(&self, counter: &DrawBuilder<'_>) -> DrawStats {
        DrawStats {
            counted: self.counted,
            wanted: counter.counted,
            dropped: self.dropped(),
            totals: counter.stats.map(|s| s.total),
        }
    }
}

/// スプライトを基準の位置 `(x, y)` に置くコマンド。
pub(crate) fn sprite_cmd(id: SpriteId, x: i32, y: i32, tint: Color) -> DrawCmd {
    scaled_sprite_cmd(id, x, y, tint, 1)
}

/// マップの絵を `MAP_SCALE` 倍で基準の位置 `(x, y)` に置くコマンド。
pub(crate) fn map_sprite_cmd(id: SpriteId, x: i32, y: i32, tint: Color) -> DrawCmd {
    scaled_sprite_cmd(id, x, y, tint, MAP_SCALE)
}

/// `dst = 基準 − pivot × scale`、大きさは `scale` 倍(最近傍の整数倍なのでドットはにじまない)。
fn scaled_sprite_cmd(id: SpriteId, x: i32, y: i32, tint: Color, scale: i32) -> DrawCmd {
    let m = sprite::meta(id);
    DrawCmd::Sprite {
        id,
        dst: Rect {
            x: x - i32::from(m.pivot_x) * scale,
            y: y - i32::from(m.pivot_y) * scale,
            w: u32::from(m.w) * scale as u32,
            h: u32::from(m.h) * scale as u32,
        },
        tint,
    }
}

/// 矩形。
pub(crate) const fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(b: &mut DrawBuilder<'_>, popups: u32, tags: u32) {
        for _ in 0..10 {
            b.fill(rect(0, 0, 1, 1), palette::BLACK);
        }
        for i in 0..popups {
            b.group(Shed::Popup, |b| {
                for _ in 0..(3 + i % 2) {
                    b.fill(rect(1, 1, 1, 1), palette::WHITE);
                }
            });
        }
        for _ in 0..tags {
            b.group(Shed::PeopleTag, |b| {
                b.fill(rect(2, 2, 1, 1), palette::WHITE);
                b.fill(rect(2, 2, 1, 1), palette::WHITE);
            });
        }
        b.fill(rect(9, 9, 1, 1), palette::BLACK);
    }

    fn run(limit: usize, popups: u32, tags: u32) -> (Vec<DrawCmd>, DrawStats) {
        let mut c = DrawBuilder::counting();
        scene(&mut c, popups, tags);
        let drop = c.plan(limit);
        let mut out = Vec::new();
        let mut b = DrawBuilder::emitting(&mut out, drop);
        scene(&mut b, popups, tags);
        let st = b.stats(&c);
        (out, st)
    }

    #[test]
    fn within_budget_nothing_is_dropped() {
        let (out, st) = run(1000, 4, 3);
        assert_eq!(st.wanted, 11 + 14 + 6);
        assert_eq!(st.counted, out.len());
        assert_eq!(st.dropped, [0; 4]);
    }

    #[test]
    fn popups_go_first_then_people_tags() {
        let (out, st) = run(11 + 14, 4, 3);
        assert!(st.counted <= 25 && out.len() == st.counted);
        assert_eq!(st.dropped[Shed::Popup as usize], 2);
        assert_eq!(st.dropped[Shed::PeopleTag as usize], 0);
        let (_, st) = run(11 + 2, 4, 3);
        assert_eq!(st.dropped[Shed::Popup as usize], 4);
        assert_eq!(st.dropped[Shed::PeopleTag as usize], 2);
        assert!(st.counted <= 13);
        assert_eq!(
            out_last(&run(11, 4, 3).0),
            Some(rect(9, 9, 1, 1)),
            "必須の要素は省かない"
        );
    }

    fn out_last(out: &[DrawCmd]) -> Option<Rect> {
        match out.last() {
            Some(DrawCmd::FillRect { rect, .. }) => Some(*rect),
            _ => None,
        }
    }

    #[test]
    fn cached_layer_counts_as_one() {
        let content = vec![DrawCmd::Clear(palette::GROUND_A); 1840];
        let mut out = Vec::new();
        let mut b = DrawBuilder::emitting(&mut out, [0; 4]);
        b.cached_layer(LayerId(0), 7, rect(-37, 44, 512, 288), &content);
        assert_eq!(b.counted(), 1);
        assert_eq!(out.len(), 1843);
        assert!(matches!(
            out[0],
            DrawCmd::BeginCachedLayer {
                layer: LayerId(0),
                revision: 7,
                x: -37,
                y: 44,
                w: 512,
                h: 288
            }
        ));
        assert_eq!(out[1841], DrawCmd::EndCachedLayer);
        assert_eq!(out[1842], DrawCmd::DrawCachedLayer { layer: LayerId(0) });
    }

    #[test]
    fn sprite_uses_pivot() {
        let cmd = sprite_cmd(sprite::STN_BODY, 56, 84, palette::WHITE);
        let DrawCmd::Sprite { dst, .. } = cmd else {
            panic!()
        };
        assert_eq!(dst, rect(49, 77, 14, 14));
        let DrawCmd::Sprite { dst, .. } = map_sprite_cmd(sprite::STN_BODY, 56, 84, palette::WHITE)
        else {
            panic!()
        };
        assert_eq!(
            dst,
            rect(42, 70, 28, 28),
            "マップの絵は 2 倍、基準点も 2 倍"
        );
    }

    #[test]
    fn nine_slice_covers_rect() {
        let mut out = Vec::new();
        let mut b = DrawBuilder::emitting(&mut out, [0; 4]);
        b.nine_slice(&sprite::NS_UI_WIN, rect(10, 20, 100, 40), palette::WHITE);
        assert_eq!(out.len(), 9);
        let area: u32 = out
            .iter()
            .map(|c| match c {
                DrawCmd::Sprite { dst, .. } => dst.w * dst.h,
                _ => 0,
            })
            .sum();
        assert_eq!(area, 4000);
        let mut out = Vec::new();
        let mut b = DrawBuilder::emitting(&mut out, [0; 4]);
        b.nine_slice_scaled(&sprite::NS_UI_SEL, rect(10, 20, 44, 44), palette::WHITE, 2);
        let DrawCmd::Sprite { dst, .. } = out[0] else {
            panic!()
        };
        let c = u32::from(sprite::NS_UI_SEL.corner) * 2;
        assert_eq!(dst, rect(10, 20, c, c), "角は 2 倍");
    }
}
