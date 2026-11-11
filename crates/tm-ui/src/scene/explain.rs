//! 説明の箱、止めない案内、highlight、説明中のうすい黒 [13 §3.11 の層 7・8、§3.13]。

use tm_assets::font::FontId;
use tm_assets::sprite;
use tm_assets::text::{TextArg, TextId};
use tm_assets::txt;
use tm_core::{Command, GameView, GridPos, POS_UNITS_PER_TILE};
use tm_platform_api::{Color, DrawCmd, MonoTime, Rect};
use tm_tutorial::{DirectiveKind, Tutorial, TutorialDirective, TutorialStageId, UiAnchor};

use super::build::Step;
use super::play::Gate;
use super::{DIM, PlayMode, PlayScene, SCREEN, SettingsMenu};
use crate::anchor::{Mark, Places, Toward, edge_toward, tile_rect};
use crate::anim::overlaps;
use crate::draw::palette::{self, with_alpha};
use crate::draw::{DrawBuilder, rect};
use crate::hud::decimal;
use crate::layout::{
    BOX_BOTTOM, BOX_TOP_Y, Cam, EXPLAIN_PAD, EXPLAIN_W, EXPLAIN_X, EXPLAIN_X_PANEL, MAP_H,
    MAP_SCALE, MAP_W, MAP_W_PANEL, MAP_X, MAP_Y, SCREEN_H, SCREEN_W, TILE, explain_h, tile_center,
    tile_in_view,
};
use crate::settings::Settings;
use crate::text::{DynText, TextBlock};

/// 本文の上の余白。
const TEXT_TOP: i32 = 8;
/// 箱の下の欄(ページと「つぎへ」。Body12 の 1 行)の上端の、箱の下端からの距離。
const FOOT_FROM_BOTTOM: i32 = 24;
/// ページの番号と「つぎへ」の間。
const FOOT_GAP: i32 = 12;
/// highlight の明暗と矢印のはねの周期(2 Hz。13 §3.12)。
const BLINK_MS: u64 = 500;
/// 矢印のはねの高さ。
const BOUNCE: i32 = 4;
/// highlight の不透明度のいちばん暗いときと明るいとき。
const ALPHA_LOW: u32 = 112;
const ALPHA_HIGH: u32 = 200;
/// これより上の矩形は、矢印を下から上向きに出す(上にはみ出すので)。
const ARROW_ROOM: i32 = 12;
/// 横向きの矢印と枠の間(せってい の箱の縁にかからない)。
const SIDE_GAP: i32 = 10;
/// 表示域の縁の矢印(2 倍で描く)と縁の間。
const EDGE_GAP: i32 = 2;
/// マップの上の印の矢印(8px の絵の 2 倍)と、はねる分の余地。
const MAP_ARROW_ROOM: i32 = 8 * MAP_SCALE + 2 + BOUNCE + 2;

/// 描く説明・案内の控え(`update` が作り `render` が読む)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    /// いまの directive。
    pub directive: TutorialDirective,
    /// 説明の箱を出す(見えている説明のページ)。
    pub boxed: bool,
    /// highlight。
    pub mark: Option<Mark>,
    /// 説明の箱の矩形。
    pub area: Rect,
    /// せってい の下に固定した箱(#15)。
    pub under_menu: bool,
    /// 「いまは ここを えらんでね」の間(highlight を 2 倍の明るさにする)。
    pub loud: bool,
}

/// 説明と案内の表示データ。
#[derive(Default)]
pub(crate) struct ExplainView {
    /// いま描くもの。
    pub shown: Option<Shown>,
    page: DynText,
    page_no: DynText,
    next: DynText,
    /// 止める説明(0)とせってい の中の説明(1)の、最後に見せたページ(`se_popup` を鳴らすかの判定)。
    seen: [Option<(TutorialStageId, u8)>; 2],
    /// 「作る」の下書き中に選べる駅(小さな丸を付ける)。
    picks: Vec<GridPos>,
    /// `permits` に通す仮の `CreateLine`(毎フレーム確保しないように使い回す)。
    probe: Option<Command>,
}

impl ExplainView {
    /// 見えている説明が新しいか(stage が変わったか、page が 0 に戻った)を調べて覚える。`hidden` は せってい に隠れている。
    fn note_page(&mut self, d: &TutorialDirective, hidden: bool) -> bool {
        if d.kind != DirectiveKind::Explanation {
            self.seen = [None, None];
            return false;
        }
        if hidden {
            self.seen[1] = None;
            return false;
        }
        let slot = &mut self.seen[usize::from(!d.pauses_game)];
        let fresh = match *slot {
            None => true,
            Some((stage, page)) => stage != d.stage || (d.page == 0 && page != 0),
        };
        *slot = Some((d.stage, d.page));
        fresh
    }

    /// highlight しているマップの対象のマス(駅、路線の真ん中のマス、列車のいるマス)。カメラが一緒に見せようとする。
    /// highlight を描かないとき(せってい の下)は `None`。
    pub(crate) fn focus(&self, view: GameView<'_>, paths: &[Vec<GridPos>]) -> Option<GridPos> {
        let s = self.shown?;
        s.mark?;
        match s.directive.highlight? {
            UiAnchor::Station(id) => view.station(id).map(|s| s.pos),
            UiAnchor::Line(id) => paths
                .get(id.index())
                .and_then(|p| p.get(p.len() / 2))
                .copied(),
            UiAnchor::Train(id) => view.train(id).map(|t| {
                let u = POS_UNITS_PER_TILE as i64;
                GridPos {
                    x: (t.point.x / u) as i32,
                    y: (t.point.y / u) as i32,
                }
            }),
            _ => None,
        }
    }

    /// 「作る」の下書き中に小さな丸を付ける駅(テスト用)。
    #[cfg(test)]
    pub(crate) fn picks(&self) -> &[GridPos] {
        &self.picks
    }
}

/// 2 Hz の三角波(0..=half)。
fn wave(now: MonoTime) -> u64 {
    let t = (now.0 / 1000) % BLINK_MS;
    let half = BLINK_MS / 2;
    half - half.abs_diff(t)
}

/// highlight の色。`loud` の間は 2 倍の明るさ(速さは同じ)[01 §10.2]。
pub(crate) fn mark_color(now: MonoTime, loud: bool) -> Color {
    let a = ALPHA_LOW + (ALPHA_HIGH - ALPHA_LOW) * wave(now) as u32 / (BLINK_MS / 2) as u32;
    let a = if loud { (2 * a).min(255) } else { a };
    with_alpha(palette::HIGHLIGHT, a as u8)
}

fn bounce(now: MonoTime) -> i32 {
    (wave(now) as i32 * BOUNCE) / (BLINK_MS / 2) as i32
}

/// `r` の上に、上下にはねる下向きの矢印を出す。上に余地がなければ下から上向きに指す。
fn arrow_at(b: &mut DrawBuilder<'_>, r: Rect, now: MonoTime) {
    if r.y < ARROW_ROOM {
        b.sprite(
            sprite::FX_ARROW_UP,
            r.x + r.w as i32 / 2,
            r.y + r.h as i32 + 1 + bounce(now),
            palette::WHITE,
        );
        return;
    }
    b.sprite(
        sprite::FX_ARROW_DOWN,
        r.x + r.w as i32 / 2,
        r.y - 1 - bounce(now),
        palette::WHITE,
    );
}

/// マップの上の `r` を指す矢印を `MAP_SCALE` 倍で出す。上に表示域の余地がなければ下から上向きに指す。
fn map_arrow_at(b: &mut DrawBuilder<'_>, r: Rect, now: MonoTime) {
    let cx = r.x + r.w as i32 / 2;
    if r.y - MAP_ARROW_ROOM < MAP_Y {
        b.map_sprite(
            sprite::FX_ARROW_UP,
            cx,
            r.y + r.h as i32 + 2 + bounce(now),
            palette::WHITE,
        );
    } else {
        b.map_sprite(
            sprite::FX_ARROW_DOWN,
            cx,
            r.y - 2 - bounce(now),
            palette::WHITE,
        );
    }
}

/// `r` の横(画面の左半分にあれば右、右半分にあれば左)に、`r` を指す横向きの矢印を出す。
fn side_arrow(b: &mut DrawBuilder<'_>, r: Rect, now: MonoTime, c: Color) {
    let y = r.y + r.h as i32 / 2 - 4;
    if 2 * (r.x + r.w as i32 / 2) <= SCREEN_W {
        b.sprite(
            sprite::ICO_TRI_LEFT,
            r.x + r.w as i32 + SIDE_GAP + bounce(now),
            y,
            c,
        );
    } else {
        b.sprite(
            sprite::ICO_TRI_RIGHT,
            r.x - SIDE_GAP - 8 - bounce(now),
            y,
            c,
        );
    }
}

/// ボタンの絵も含めて灰色で描く(ガードの間の「つぎへ」)。
fn greyed(b: &mut DrawBuilder<'_>, block: &TextBlock, x: i32, y: i32) {
    for g in &block.glyphs {
        b.push(DrawCmd::Sprite {
            id: g.sprite,
            dst: rect(
                x + i32::from(g.x),
                y + i32::from(g.y),
                u32::from(g.w),
                u32::from(g.h),
            ),
            tint: palette::TEXT_DISABLED,
        });
    }
}

/// 決定ボタンの文。最後のページ(1 ページだけの説明を含む)は「とじる」。
pub(crate) fn next_label(page: u8, page_count: u8) -> TextId {
    if page + 1 >= page_count {
        txt::EXPL_CLOSE
    } else {
        txt::EXPL_NEXT
    }
}

/// 本文の行数 `lines` の説明の箱。`top` なら上(y = 36)、でなければ下(下端 316)[13 §3.5]。
pub(crate) fn page_rect(x: i32, lines: u8, top: bool) -> Rect {
    let h = explain_h(lines.max(1)) as i32;
    let y = if top { BOX_TOP_Y } else { BOX_BOTTOM - h };
    rect(x, y, EXPLAIN_W, h as u32)
}

/// 説明の箱(本文 Head16、下の欄にページと「つぎへ」)。`ready` でなければ「つぎへ」を灰色で描く。
pub(crate) fn draw_page_box(
    b: &mut DrawBuilder<'_>,
    r: Rect,
    page: &TextBlock,
    page_no: Option<&TextBlock>,
    next: &TextBlock,
    ready: bool,
) {
    b.nine_slice(&sprite::NS_UI_POPUP, r, palette::WHITE);
    b.text(page, r.x + EXPLAIN_PAD, r.y + TEXT_TOP, palette::TEXT);
    let by = r.y + r.h as i32 - FOOT_FROM_BOTTOM;
    let right = r.x + r.w as i32 - EXPLAIN_PAD;
    let nw = i32::from(next.width);
    if ready {
        b.text(next, right - nw, by, palette::TEXT);
    } else {
        greyed(b, next, right - nw, by);
    }
    if let Some(no) = page_no {
        let pw = i32::from(no.width);
        b.text(no, right - nw - FOOT_GAP - pw, by, palette::TEXT_DISABLED);
    }
}

/// 印(枠と矢印)が占める矩形。箱と重ならないかを調べる。
fn mark_area(m: Mark) -> Rect {
    let grow = |r: Rect, l: i32, t: i32, rr: i32, bb: i32| {
        rect(
            r.x - l,
            r.y - t,
            (r.w as i32 + l + rr) as u32,
            (r.h as i32 + t + bb) as u32,
        )
    };
    match m {
        Mark::Frame(r) => grow(r, 0, 14, 0, 14),
        Mark::Tile(r) | Mark::Line { mid: r, .. } => grow(r, 0, MAP_ARROW_ROOM, 0, MAP_ARROW_ROOM),
        Mark::Side(r) => grow(r, SIDE_GAP + 12, 0, SIDE_GAP + 12, 0),
        Mark::Edge {
            y,
            toward: Toward::Left | Toward::Right,
            ..
        } => rect(MAP_X, y - EDGE_ARROW / 2, MAP_W as u32, EDGE_ARROW as u32),
        Mark::Edge { x, y, .. } => rect(
            x - EDGE_ARROW,
            y - EDGE_ARROW - BOUNCE,
            2 * EDGE_ARROW as u32,
            2 * (EDGE_ARROW + BOUNCE) as u32,
        ),
    }
}

/// 「作る」の選べる駅の丸の位置(駅の上、表示域に入らなければ下)。丸が表示域に入らなければ `None`。
pub(crate) fn pick_dot_at(p: GridPos, cam: Cam, view_w: i32) -> Option<(i32, i32)> {
    let (x, y) = tile_center(p, cam);
    let d = TILE / 2 + 4 * MAP_SCALE;
    let r = 4 * MAP_SCALE;
    [y - d, y + d].into_iter().find_map(|cy| {
        let inside =
            x - r >= MAP_X && x + r <= MAP_X + view_w && cy - r >= MAP_Y && cy + r <= MAP_Y + MAP_H;
        inside.then_some((x, cy))
    })
}

/// 表示域の縁の矢印の大きさ(8px の絵の 2 倍)。
const EDGE_ARROW: i32 = 8 * MAP_SCALE;

/// 表示域(幅 `map_w`)の縁に、外の対象を指す矢印を 2 倍で描く。
fn draw_edge(b: &mut DrawBuilder<'_>, m: Mark, map_w: i32, now: MonoTime, c: Color) {
    let Mark::Edge { x, y, toward } = m else {
        return;
    };
    let k = bounce(now);
    let (id, x, y) = match toward {
        Toward::Left => (
            sprite::ICO_TRI_LEFT,
            MAP_X + EDGE_GAP + k,
            y - EDGE_ARROW / 2,
        ),
        Toward::Right => (
            sprite::ICO_TRI_RIGHT,
            MAP_X + map_w - EDGE_GAP - EDGE_ARROW - k,
            y - EDGE_ARROW / 2,
        ),
        Toward::Up => (sprite::FX_ARROW_UP, x, MAP_Y + EDGE_GAP + k),
        Toward::Down => (sprite::FX_ARROW_DOWN, x, MAP_Y + MAP_H - EDGE_GAP - k),
    };
    b.map_sprite(id, x, y, c);
}

/// 説明の箱を下か上に置く。印が下半分なら上を先に試し、印と重なる方は避ける [13 §3.5]。
/// 印に重ならない方が、マップのカーソル(`cursor`。画面の矩形)にも重なるなら、もう一方が印にもカーソルにも重ならなければそちらにする。
fn place_box(x: i32, lines: u8, mark: Option<Mark>, cursor: Option<Rect>) -> Rect {
    let low = page_rect(x, lines, false);
    let high = page_rect(x, lines, true);
    let (first, second) = match mark {
        Some(m) if 2 * m.center_y() >= SCREEN_H => (high, low),
        _ => (low, high),
    };
    let area = mark.map(mark_area);
    let on_mark = |r: Rect| area.is_some_and(|a| overlaps(r, a));
    let on_cursor = |r: Rect| cursor.is_some_and(|c| overlaps(r, c));
    let free = |r: Rect| !on_mark(r) && !on_cursor(r);
    if (on_mark(first) && !on_mark(second)) || (!on_mark(first) && on_cursor(first) && free(second))
    {
        second
    } else {
        first
    }
}

/// うすい黒を `hole` の外だけに 1 まい重ねる。
fn dim_around(b: &mut DrawBuilder<'_>, hole: Option<Rect>) {
    let Some(h) = hole else {
        b.fill(SCREEN, DIM);
        return;
    };
    let x0 = h.x.clamp(0, SCREEN_W);
    let y0 = h.y.clamp(0, SCREEN_H);
    let x1 = (h.x + h.w as i32).clamp(x0, SCREEN_W);
    let y1 = (h.y + h.h as i32).clamp(y0, SCREEN_H);
    let parts = [
        rect(0, 0, SCREEN_W as u32, y0 as u32),
        rect(0, y1, SCREEN_W as u32, (SCREEN_H - y1) as u32),
        rect(0, y0, x0 as u32, (y1 - y0) as u32),
        rect(x1, y0, (SCREEN_W - x1) as u32, (y1 - y0) as u32),
    ];
    for r in parts.into_iter().filter(|r| r.w > 0 && r.h > 0) {
        b.fill(r, DIM);
    }
}

impl PlayScene {
    /// マップの表示域の幅(パネルを開いていれば 416)。
    pub(crate) fn map_w(&self) -> i32 {
        match self.mode {
            PlayMode::Panel(_) => MAP_W_PANEL,
            PlayMode::Map | PlayMode::Build(_) => MAP_W,
        }
    }

    fn places<'a>(&'a self, view: GameView<'a>) -> Places<'a> {
        let (panel, build) = match &self.mode {
            PlayMode::Map => (None, None),
            PlayMode::Panel(p) => (Some(&**p), None),
            PlayMode::Build(b) => (None, Some(&**b)),
        };
        Places {
            view,
            cam: self.camera.pos,
            map_w: self.map_w(),
            paths: self.map.as_ref().map_or(&[][..], |m| &m.paths[..]),
            panel,
            build,
            menu: self.settings_menu.as_ref(),
            band: &self.band,
            hud: &self.hud,
        }
    }

    /// 説明・案内の表示データを作る(`update` の最後。出来事を渡した後の directive で)。
    /// 新しい説明を見せ始めたら true(`se_popup` を鳴らす候補)。
    pub(crate) fn prepare_explain(
        &mut self,
        now: MonoTime,
        view: GameView<'_>,
        tutorial: Option<&Tutorial>,
    ) -> bool {
        let dev = self.dev;
        let menu_open = self.settings_menu.is_some();
        let Some(d) = self.directive(tutorial) else {
            if let PlayMode::Panel(p) = &mut self.mode {
                p.explaining = false;
            }
            self.input.watch_page(now, None);
            self.explain.seen = [None, None];
            self.explain.shown = None;
            self.explain.picks.clear();
            self.band.set_guide(None, dev);
            return false;
        };
        if let PlayMode::Panel(p) = &mut self.mode {
            p.explaining = d.kind == DirectiveKind::Explanation && d.pauses_game;
        }
        let menu_top = self.settings_menu.as_ref().is_none_or(SettingsMenu::is_top);
        let hidden = menu_open && (d.pauses_game || !menu_top);
        let fresh = self.explain.note_page(&d, hidden);
        let boxed = d.kind == DirectiveKind::Explanation && !hidden;
        // 見え始めたフレームからガードを数える(次のフレームの振り分けでも同じページなので始め直さない)
        self.input.watch_page(now, boxed.then_some(&d));
        let guide = d.kind == DirectiveKind::Guide;
        self.band.set_guide(guide.then_some(d.message), dev);
        let visible = boxed || (guide && !menu_open);
        let mark = if visible {
            d.highlight.and_then(|a| self.places(view).resolve(a))
        } else {
            None
        };
        let page_w = (EXPLAIN_W as i32 - 2 * EXPLAIN_PAD) as u16;
        if boxed {
            self.explain
                .page
                .set(d.message, &[], FontId::Head16, page_w, dev);
        }
        let lines = self.explain.page.block.lines;
        // #15 は せってい の下に固定する(highlight の反対側の規則を使わない。UI-D-35)
        let cursor = Some(tile_rect(self.cursor, self.camera.pos));
        let area = if menu_open {
            page_rect(EXPLAIN_X, lines, false)
        } else if matches!(self.mode, PlayMode::Panel(_)) {
            place_box(EXPLAIN_X_PANEL, lines, mark, cursor)
        } else {
            place_box(EXPLAIN_X, lines, mark, cursor)
        };
        let loud = self.band.notice_text() == Some(txt::ERR_TUTORIAL);
        self.explain.shown = Some(Shown {
            directive: d,
            boxed,
            mark,
            area,
            under_menu: menu_open,
            loud,
        });
        self.prepare_picks(view, tutorial, guide && !menu_open && d.highlight.is_none());
        if boxed {
            let ex = &mut self.explain;
            let (mut a, mut b) = ([0u8; 4], [0u8; 4]);
            ex.page_no.set(
                txt::EXPL_PAGE,
                &[
                    TextArg {
                        name: "n",
                        value: decimal(u64::from(d.page) + 1, &mut a),
                    },
                    TextArg {
                        name: "m",
                        value: decimal(u64::from(d.page_count), &mut b),
                    },
                ],
                FontId::Body12,
                page_w,
                dev,
            );
            ex.next.set(
                next_label(d.page, d.page_count),
                &[],
                FontId::Body12,
                page_w,
                dev,
            );
        }
        fresh
    }

    /// 「作る」の下書き中で highlight がないとき、選べる駅(と、2 駅目からは仕上げの最後の駅)を集める [13 §6.5]。
    fn prepare_picks(&mut self, view: GameView<'_>, tutorial: Option<&Tutorial>, want: bool) {
        self.explain.picks.clear();
        let PlayMode::Build(build) = &self.mode else {
            return;
        };
        let (true, Step::Create(draft), None) = (want, &build.step, &build.confirm) else {
            return;
        };
        let gate = Gate::new(tutorial);
        #[cfg(test)]
        let gate = gate.with_forbid(self.forbid);
        let ex = &mut self.explain;
        let mut probe = ex.probe.take().unwrap_or(Command::CreateLine {
            stations: Vec::new(),
            bends: Vec::new(),
        });
        for st in view.stations() {
            let finish = draft.selected.len() >= 2 && draft.selected.last() == Some(&st.id);
            let ok = finish
                || (view.validate_line_draft(&draft.selected, st.id).is_ok()
                    && draft.selected.last().is_none_or(|&prev| {
                        if let Command::CreateLine { stations, bends } = &mut probe {
                            stations.clone_from(&draft.selected);
                            stations.push(st.id);
                            bends.clone_from(&draft.bends);
                            bends.push(view.default_bend(prev, st.id));
                        }
                        gate.permits(&probe)
                    }));
            if ok {
                ex.picks.push(st.pos);
            }
        }
        ex.probe = Some(probe);
    }

    /// 層 7・8: うすい黒(止める説明か せってい。重なっても 1 まい)と、その上のもの [13 §3.11]。
    /// せってい が開いていれば、帯の 2 段目、せってい と下位の画面、#15 の箱、#15 の highlight の順。
    /// 閉じていれば、選べる駅の丸、highlight、説明の箱の順。
    pub(super) fn draw_explain(&self, b: &mut DrawBuilder<'_>, now: MonoTime, settings: &Settings) {
        let shown = self.explain.shown;
        if let Some(menu) = &self.settings_menu {
            b.fill(SCREEN, DIM);
            self.band.draw_over_menu(b);
            menu.draw(b, settings);
            if let Some(s) = shown {
                if s.boxed {
                    self.draw_box(b, s.area, s.directive.page_count > 1, now);
                }
                self.draw_mark(b, s, now);
            }
            return;
        }
        if shown.is_some_and(|s| s.boxed && s.directive.pauses_game) {
            dim_around(b, shown.and_then(|s| s.mark).and_then(Mark::hole));
        }
        let Some(s) = shown else { return };
        let color = mark_color(now, s.loud);
        let (cam, view_w) = (self.camera.pos, self.map_w());
        let mut seen = false;
        for &p in &self.explain.picks {
            if let Some((x, y)) = pick_dot_at(p, cam, view_w) {
                b.map_sprite(sprite::FX_DOT, x, y, color);
                seen = true;
            }
        }
        // 選べる駅が 1 つも見えなければ、カーソルにいちばん近い駅のほうへ縁の矢印
        if !seen
            && let Some(&p) = self
                .explain
                .picks
                .iter()
                .min_by_key(|p| p.chebyshev(self.cursor))
        {
            draw_edge(
                b,
                edge_toward(tile_rect(p, cam), view_w),
                view_w,
                now,
                color,
            );
        }
        self.draw_mark(b, s, now);
        if s.boxed {
            self.draw_box(b, s.area, s.directive.page_count > 1, now);
        }
    }

    fn draw_mark(&self, b: &mut DrawBuilder<'_>, s: Shown, now: MonoTime) {
        let color = mark_color(now, s.loud);
        match s.mark {
            Some(Mark::Frame(r)) => {
                b.nine_slice(&sprite::NS_UI_SEL, r, color);
                arrow_at(b, r, now);
            }
            Some(Mark::Tile(r)) => {
                b.nine_slice_scaled(&sprite::NS_UI_SEL, r, color, MAP_SCALE);
                map_arrow_at(b, r, now);
            }
            Some(Mark::Side(r)) => {
                b.nine_slice(&sprite::NS_UI_SEL, r, color);
                side_arrow(b, r, now, color);
            }
            Some(Mark::Line { line, mid }) => {
                let tiles = self.map.as_ref().and_then(|m| m.paths.get(line.index()));
                let (o, i) = (4 * MAP_SCALE, 3 * MAP_SCALE);
                for &t in tiles.into_iter().flatten() {
                    if tile_in_view(t, self.camera.pos, self.map_w()) {
                        let (x, y) = tile_center(t, self.camera.pos);
                        b.fill(
                            rect(x - o, y - o, 2 * o as u32, 2 * o as u32),
                            palette::BLACK,
                        );
                        b.fill(rect(x - i, y - i, 2 * i as u32, 2 * i as u32), color);
                    }
                }
                map_arrow_at(b, mid, now);
            }
            Some(m @ Mark::Edge { .. }) => draw_edge(b, m, self.map_w(), now, color),
            None => {}
        }
    }

    fn draw_box(&self, b: &mut DrawBuilder<'_>, r: Rect, pages: bool, now: MonoTime) {
        let ex = &self.explain;
        draw_page_box(
            b,
            r,
            &ex.page.block,
            pages.then_some(&ex.page_no.block),
            &ex.next.block,
            self.input.page_ready(now),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{BAND, EXPLAIN_LINES};

    fn arrow(r: Rect) -> DrawCmd {
        let mut out = Vec::new();
        let mut b = DrawBuilder::emitting(&mut out, Default::default());
        arrow_at(&mut b, r, MonoTime(0));
        out[0]
    }

    /// 縁の矢印 1 つの絵と描く矩形(はねる量は時刻 0 で 0)。
    fn edge(x: i32, y: i32, toward: Toward, map_w: i32) -> (tm_platform_api::SpriteId, Rect) {
        let mut out = Vec::new();
        let mut b = DrawBuilder::emitting(&mut out, Default::default());
        draw_edge(
            &mut b,
            Mark::Edge { x, y, toward },
            map_w,
            MonoTime(0),
            palette::WHITE,
        );
        assert_eq!(out.len(), 1);
        let DrawCmd::Sprite { id, dst, .. } = out[0] else {
            panic!("絵")
        };
        (id, dst)
    }

    #[test]
    fn edge_arrows_point_outward_from_each_side_of_the_view() {
        assert_eq!(bounce(MonoTime(0)), 0);
        for map_w in [MAP_W, MAP_W_PANEL] {
            let right = MAP_X + map_w;
            let bottom = MAP_Y + MAP_H;
            assert_eq!(
                edge(MAP_X, 100, Toward::Left, map_w),
                (sprite::ICO_TRI_LEFT, rect(MAP_X + 2, 92, 16, 16)),
                "左: 左向きの三角を左の縁の 2px 内側、対象の高さに"
            );
            assert_eq!(
                edge(right, 200, Toward::Right, map_w),
                (sprite::ICO_TRI_RIGHT, rect(right - 2 - 16, 192, 16, 16)),
                "右: 右向きの三角を右の縁(パネルを開けば 416)の 2px 内側に"
            );
            assert_eq!(
                edge(300, MAP_Y, Toward::Up, map_w),
                (sprite::FX_ARROW_UP, rect(300 - 10, MAP_Y + 2, 22, 16)),
                "上: 上向きの矢印の先を上の縁の 2px 内側に"
            );
            assert_eq!(
                edge(300, bottom, Toward::Down, map_w),
                (
                    sprite::FX_ARROW_DOWN,
                    rect(300 - 10, bottom - 2 - 16, 22, 16)
                ),
                "下: 下向きの矢印の先を下の縁の 2px 内側に"
            );
        }
        let (_, moving) = {
            let mut out = Vec::new();
            let mut b = DrawBuilder::emitting(&mut out, Default::default());
            let m = Mark::Edge {
                x: MAP_X,
                y: 100,
                toward: Toward::Left,
            };
            draw_edge(
                &mut b,
                m,
                MAP_W,
                MonoTime(BLINK_MS * 1000 / 2),
                palette::WHITE,
            );
            let DrawCmd::Sprite { id, dst, .. } = out[0] else {
                panic!()
            };
            (id, dst)
        };
        assert_eq!(moving.x, MAP_X + 2 + BOUNCE, "4px はねる");
    }

    #[test]
    fn box_moves_to_the_other_side_when_the_first_choice_covers_the_mark() {
        let low = place_box(EXPLAIN_X, EXPLAIN_LINES, None, None);
        let high = page_rect(EXPLAIN_X, EXPLAIN_LINES, true);
        let top = high.y + high.h as i32 + 14;
        let m = Mark::Frame(rect(
            EXPLAIN_X + 20,
            top,
            20,
            (2 * (SCREEN_H / 2 - top) - 2) as u32,
        ));
        assert!(2 * m.center_y() < SCREEN_H, "前提: 下を先に試す印");
        assert!(overlaps(low, mark_area(m)), "前提: 下は印と重なる");
        assert!(!overlaps(high, mark_area(m)), "前提: 上は重ならない");
        assert_eq!(
            place_box(EXPLAIN_X, EXPLAIN_LINES, Some(m), None),
            high,
            "重ならない上へ"
        );
        let small = Mark::Frame(rect(EXPLAIN_X + 20, 60, 20, 20));
        assert_eq!(
            place_box(EXPLAIN_X, EXPLAIN_LINES, Some(small), None),
            low,
            "上半分の印なら下"
        );
    }

    #[test]
    fn box_size_follows_the_lines_and_sits_above_the_band() {
        for lines in 1..=EXPLAIN_LINES {
            let low = page_rect(EXPLAIN_X, lines, false);
            let high = page_rect(EXPLAIN_X, lines, true);
            assert_eq!((low.w, low.h), (EXPLAIN_W, explain_h(lines)));
            assert_eq!(low.y + low.h as i32, BAND.y - 4, "下は帯の 4px 上");
            assert_eq!(high.y, BOX_TOP_Y);
        }
        assert_eq!(
            page_rect(EXPLAIN_X, 0, false),
            page_rect(EXPLAIN_X, 1, false)
        );
        assert_eq!((explain_h(1), explain_h(2)), (56, 76));
    }

    #[test]
    fn arrow_points_down_from_above_or_up_from_below() {
        let DrawCmd::Sprite { id, dst, .. } = arrow(rect(100, 60, 20, 20)) else {
            panic!("絵")
        };
        assert_eq!(id, sprite::FX_ARROW_DOWN);
        assert!(dst.y + dst.h as i32 <= 60, "枠の上");
        let DrawCmd::Sprite { id, dst, .. } = arrow(rect(100, 2, 20, 20)) else {
            panic!("絵")
        };
        assert_eq!(id, sprite::FX_ARROW_UP, "上に余地がなければ下から");
        assert!(dst.y >= 22, "枠の下");
    }
}
