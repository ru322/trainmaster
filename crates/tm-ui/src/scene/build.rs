//! ろせんモード: 作る/のばす の選択、駅の選択、曲がり方、完成図、確認、送信 [13 §3.9]。

use tm_assets::font::FontId;
use tm_assets::sound::{SE_CANCEL, SE_CONFIRM, SE_CURSOR};
use tm_assets::sprite;
use tm_assets::text::{TextArg, TextId};
use tm_assets::txt;
use tm_core::{
    Bend, Command, CommandError, CommandKind, Feature, GameEvent, GameView, GridPos, LineEnd,
    LineId, Money, StationId,
};
use tm_input::{Action, ActionFrame, InputDevice};
use tm_numeric::FormatStyle;
use tm_platform_api::{Color, DrawCmd, MonoTime};
use tm_tutorial::{MenuAnchor, UiEvent};

use super::play::{Ctx, show_error, step_x, step_y};
use crate::draw::palette::{self, line_color};
use crate::draw::{DrawBuilder, rect};
use crate::hud::{Band, BandChips, decimal};
use crate::input_ctx::Guard;
use crate::layout::{
    BOX_BOTTOM, BOX_TOP_Y, BUILD_CHOICES, Cam, INFO_GAP, INFO_ROW, INFO_TEXT_W, INFO_TEXT_X,
    LINE_CONFIRM_W, LINE_CONFIRM_X, MAP_H, MAP_SCALE, MAP_Y, TILE, line_confirm_h, tile_center,
    tile_ring, tile_screen,
};
use crate::text::DynText;
use crate::text::numbers::NumText;

/// 「作る/のばす」の選択肢の箱。
const CHOOSE: tm_platform_api::Rect = BUILD_CHOICES;
/// 選択肢の 1 行の高さ。
const CHOICE_H: i32 = 20;
pub(crate) const TAG_W: u16 = 300;
/// マップの上の札(選んだ順の番号、「もう いちど」)の高さ(Body12 の 1 行)。
const TAG_H: i32 = 16;
/// 確認の「はい」「いいえ」の枠。
const BUTTON_W: i32 = 72;
const BUTTON_H: i32 = 20;

/// 作りかけの路線 [13 §3.9]。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CreateDraft {
    /// 選んだ順の駅。
    pub selected: Vec<StationId>,
    /// `bends[i]` = `selected[i]` → `selected[i+1]`。
    pub bends: Vec<Bend>,
    /// カーソルの先の駅への仮の曲がり方。
    pub hover: Option<(StationId, Bend)>,
}

/// のばす: つなぐ駅を選んでいるところ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExtendDraft {
    pub line: LineId,
    pub end: LineEnd,
    /// 延ばす端の駅。
    pub from: StationId,
    pub hover: Option<(StationId, Bend)>,
}

/// ろせんモードの段階。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// 「ろせんを 作る」(0)/「ろせんを のばす」(1)を選ぶ。
    Choose { sel: usize },
    /// 作る。
    Create(CreateDraft),
    /// のばす: 端の駅を選ぶ。
    PickEnd,
    /// のばす: 端が複数の路線の端なので路線を選ぶ。
    Which {
        station: StationId,
        cands: Vec<(LineId, LineEnd)>,
        idx: usize,
    },
    /// のばす: つなぐ駅を選ぶ。
    PickNew(ExtendDraft),
}

/// 路線の確認ダイアログ(送信中も出したまま)。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineConfirm {
    /// 「はい」を選んでいる。初期は「はい」[UI-D-07]。
    pub yes: bool,
    guard: Guard,
    /// 上(y = 36)に出す。
    pub top: bool,
    /// 「つうか する えきが あるよ」を添える。
    pub pass: bool,
    /// 結果のイベントを待っている。
    pub submitting: bool,
    /// 線路の値段(0 円なら行を出さない)。
    pub cost: Money,
    cmd: Command,
}

/// ろせんモードの状態。
pub(crate) struct BuildState {
    /// 段階。
    pub step: Step,
    /// 確認ダイアログ。
    pub confirm: Option<LineConfirm>,
    create_ok: bool,
    extend_err: Option<TextId>,
    extend_lock: Option<u8>,
    choices: [DynText; 2],
    finish: DynText,
    yes: DynText,
    no: DynText,
    title: DynText,
    pass: DynText,
    confirm_cost: DynText,
    confirm_num: NumText,
    /// 下書きの線路の値段。
    pub cost: DraftCost,
    tags: Vec<NumText>,
    preview: Vec<GridPos>,
    solid: Vec<Vec<GridPos>>,
}

/// 下書きの線路の値段 [13 §3.9 UI-D-36]。値段は `validate` の結果からだけ取る。
#[derive(Default)]
pub(crate) struct DraftCost {
    stations: Vec<StationId>,
    bends: Vec<Bend>,
    extend: Option<(LineId, LineEnd, StationId, Bend)>,
    aimed: bool,
    hover: bool,
    /// 出す値段(対象のコマンドがない・0 円なら `None`)。
    pub cost: Option<Money>,
    /// 足りない分。
    pub short: Option<Money>,
    /// 決めた駅だけで仕上げるとお金が足りない。
    pub finish_short: bool,
    label: DynText,
    num: NumText,
    short_text: DynText,
    short_num: NumText,
}

/// 値段を見るコマンド。
enum Aim<'a> {
    None,
    Create(&'a [StationId], &'a [Bend], Option<(StationId, Bend)>),
    Extend(LineId, LineEnd, StationId, Bend),
}

/// 足りないときの × つきのコインの幅。
pub(crate) const COIN_W: i32 = 16;

impl DraftCost {
    /// 値段を見るコマンドを決める。変わったら true。
    fn aim(&mut self, step: &Step, confirm: Option<&LineConfirm>) -> bool {
        let aim = match (confirm.map(|c| &c.cmd), step) {
            (Some(Command::CreateLine { stations, bends }), _) => {
                Aim::Create(stations, bends, None)
            }
            (
                Some(&Command::ExtendLine {
                    line,
                    end,
                    station,
                    bend,
                }),
                _,
            ) => Aim::Extend(line, end, station, bend),
            (Some(_), _) => Aim::None,
            (None, Step::Create(d)) => match d.hover {
                Some(h) if !d.selected.is_empty() => Aim::Create(&d.selected, &d.bends, Some(h)),
                _ if d.selected.len() >= 2 => Aim::Create(&d.selected, &d.bends, None),
                _ => Aim::None,
            },
            (None, Step::PickNew(d)) => match d.hover {
                Some((s, b)) => Aim::Extend(d.line, d.end, s, b),
                None => Aim::None,
            },
            (None, _) => Aim::None,
        };
        if self.same(&aim) {
            return false;
        }
        self.stations.clear();
        self.bends.clear();
        self.extend = None;
        self.hover = false;
        self.aimed = true;
        match aim {
            Aim::None => self.aimed = false,
            Aim::Create(s, b, h) => {
                self.stations.extend_from_slice(s);
                self.bends.extend_from_slice(b);
                if let Some((x, bend)) = h {
                    self.stations.push(x);
                    self.bends.push(bend);
                    self.hover = true;
                }
            }
            Aim::Extend(l, e, s, b) => self.extend = Some((l, e, s, b)),
        }
        true
    }

    fn same(&self, aim: &Aim<'_>) -> bool {
        match *aim {
            Aim::None => !self.aimed,
            Aim::Create(s, b, h) => {
                let extra = usize::from(h.is_some());
                self.aimed
                    && self.extend.is_none()
                    && self.hover == h.is_some()
                    && self.stations.len() == s.len() + extra
                    && self.bends.len() == b.len() + extra
                    && self.stations.starts_with(s)
                    && self.bends.starts_with(b)
                    && h.is_none_or(|(x, bend)| {
                        self.stations.last() == Some(&x) && self.bends.last() == Some(&bend)
                    })
            }
            Aim::Extend(l, e, s, b) => self.aimed && self.extend == Some((l, e, s, b)),
        }
    }

    fn price_create(&mut self, view: GameView<'_>) -> (Option<Money>, Option<Money>) {
        let cmd = Command::CreateLine {
            stations: std::mem::take(&mut self.stations),
            bends: std::mem::take(&mut self.bends),
        };
        let r = price(view, &cmd);
        if let Command::CreateLine { stations, bends } = cmd {
            self.stations = stations;
            self.bends = bends;
        }
        r
    }

    /// `validate` で値段を取り直し、文を作る。
    fn evaluate(&mut self, view: GameView<'_>, dev: InputDevice) {
        let (cost, short) = match (self.aimed, self.extend) {
            (false, _) => (None, None),
            (true, Some((line, end, station, bend))) => price(
                view,
                &Command::ExtendLine {
                    line,
                    end,
                    station,
                    bend,
                },
            ),
            (true, None) => self.price_create(view),
        };
        self.cost = cost.filter(|c| !c.is_zero());
        self.short = short.filter(|_| self.cost.is_some());
        self.finish_short = if self.hover && self.extend.is_none() && self.stations.len() > 2 {
            let (x, b) = (self.stations.pop(), self.bends.pop());
            let (c, s) = self.price_create(view);
            self.stations.extend(x);
            self.bends.extend(b);
            s.is_some() && c.is_some_and(|c| !c.is_zero())
        } else {
            self.short.is_some()
        };
        let body = FontId::Body12;
        if let Some(c) = self.cost {
            self.num.set(c, &FormatStyle::DEFAULT, body);
            let money = TextArg {
                name: "money",
                value: &self.num.text,
            };
            self.label
                .set(txt::BUILD_COST, &[money], body, INFO_TEXT_W, dev);
        }
        if let Some(s) = self.short {
            self.short_num.set(s, &FormatStyle::DEFAULT, body);
            let money = TextArg {
                name: "money",
                value: &self.short_num.text,
            };
            self.short_text
                .set(txt::BUILD_COST_SHORT, &[money], body, INFO_TEXT_W, dev);
        }
    }

    /// 情報の段に描く。出さないなら false。
    fn draw(&self, b: &mut DrawBuilder<'_>) -> bool {
        if self.cost.is_none() {
            return false;
        }
        b.fill(INFO_ROW, palette::INFO_BG);
        let y = INFO_ROW.y;
        b.text(&self.label.block, INFO_TEXT_X, y, palette::TEXT);
        if self.short.is_some() {
            let x = INFO_TEXT_X + i32::from(self.label.block.width) + INFO_GAP;
            b.sprite(sprite::ICO_COIN_X, x, y + 2, palette::WHITE);
            b.text(&self.short_text.block, x + COIN_W, y, palette::ERROR);
        }
        true
    }
}

/// `validate` の結果からの値段と足りない分。
fn price(view: GameView<'_>, cmd: &Command) -> (Option<Money>, Option<Money>) {
    match view.validate(cmd) {
        Ok(p) => (Some(p.cost), None),
        Err(CommandError::InsufficientFunds {
            required,
            available,
        }) => (
            Some(required),
            Some(required.checked_sub(available).unwrap_or(Money::ZERO)),
        ),
        Err(_) => (None, None),
    }
}

fn station_at(view: GameView<'_>, pos: GridPos) -> Option<StationId> {
    view.station_at(pos)
}

impl BuildState {
    /// BuildMode を押した。初期の選択は、空き枠があれば「作る」、なければ「のばす」[UI-D-06]。
    pub(crate) fn start(cx: &mut Ctx<'_, '_>) -> BuildState {
        let (used, slots) = cx.view.line_slots();
        let create_ok = used < slots;
        let lock = cx.view.balance().unlock_level(Feature::LineExtend).0;
        let (extend_err, extend_lock) = if cx.view.lines().len() == 0 {
            (Some(txt::ERR_NO_LINE), None)
        } else if !cx.view.is_unlocked(Feature::LineExtend) {
            (Some(txt::ERR_LOCKED), Some(lock))
        } else {
            (None, None)
        };
        cx.se(SE_CONFIRM);
        cx.event(UiEvent::MenuOpened(MenuAnchor::BuildMode));
        BuildState {
            step: Step::Choose {
                sel: usize::from(!create_ok),
            },
            confirm: None,
            create_ok,
            extend_err,
            extend_lock,
            choices: Default::default(),
            finish: DynText::default(),
            yes: DynText::default(),
            no: DynText::default(),
            title: DynText::default(),
            pass: DynText::default(),
            confirm_cost: DynText::default(),
            confirm_num: NumText::default(),
            cost: DraftCost::default(),
            tags: Vec::new(),
            preview: Vec::new(),
            solid: Vec::new(),
        }
    }

    /// マップのカーソルを動かす段階か。
    pub(crate) fn uses_map_cursor(&self) -> bool {
        self.confirm.is_none()
            && matches!(
                self.step,
                Step::Create(_) | Step::PickEnd | Step::PickNew(_)
            )
    }

    /// 画面(層)の番号。
    pub(crate) fn layer_key(&self) -> u32 {
        let s = match self.step {
            Step::Choose { .. } => 0,
            Step::Create(_) => 1,
            Step::PickEnd => 2,
            Step::Which { .. } => 3,
            Step::PickNew(_) => 4,
        };
        s + 8 * u32::from(self.confirm.is_some())
    }

    fn exit(&mut self, cx: &mut Ctx<'_, '_>) -> bool {
        if self.confirm.take().is_some() {
            cx.event(UiEvent::MenuClosed(MenuAnchor::LineConfirm));
        }
        cx.event(UiEvent::MenuClosed(MenuAnchor::BuildMode));
        true
    }

    /// 入力を受ける。ろせんモードを終えるなら true。
    pub(crate) fn handle(
        &mut self,
        input: &ActionFrame,
        cursor: GridPos,
        cx: &mut Ctx<'_, '_>,
    ) -> bool {
        if self.confirm.is_some() {
            self.handle_confirm(input, cx);
            return false;
        }
        let pressed = |a| input.pressed.contains(a);
        if pressed(Action::BuildMode) {
            cx.se(SE_CANCEL);
            return self.exit(cx);
        }
        match &mut self.step {
            Step::Choose { sel } => {
                let dy = step_y(input);
                if dy != 0 {
                    let next = (*sel as i32 + dy).clamp(0, 1) as usize;
                    if next != *sel {
                        *sel = next;
                        cx.se(SE_CURSOR);
                    }
                }
                if pressed(Action::Cancel) {
                    cx.se(SE_CANCEL);
                    return self.exit(cx);
                }
                if pressed(Action::Confirm) {
                    self.choose(cx);
                }
            }
            Step::Create(draft) => {
                if pressed(Action::Cancel) {
                    cx.se(SE_CANCEL);
                    if draft.selected.pop().is_none() {
                        return self.exit(cx);
                    }
                    draft.bends.truncate(draft.selected.len().saturating_sub(1));
                    draft.hover = None;
                    return false;
                }
                update_hover_create(draft, cursor, cx.view);
                if pressed(Action::ToggleBend) {
                    if let Some((_, b)) = &mut draft.hover {
                        *b = b.reversed();
                        cx.se(SE_CURSOR);
                    } else if let Some(b) = draft.bends.last_mut() {
                        *b = b.reversed();
                        cx.se(SE_CURSOR);
                    }
                }
                if pressed(Action::Confirm) {
                    self.confirm_create(cursor, cx);
                }
            }
            Step::PickEnd => {
                if pressed(Action::Cancel) {
                    cx.se(SE_CANCEL);
                    return self.exit(cx);
                }
                if pressed(Action::Confirm) {
                    self.pick_end(cursor, cx);
                }
            }
            Step::Which {
                station,
                cands,
                idx,
            } => {
                let dx = step_x(input);
                if dx != 0 && cands.len() > 1 {
                    *idx = (*idx as i32 + dx).rem_euclid(cands.len() as i32) as usize;
                    cx.se(SE_CURSOR);
                }
                if pressed(Action::Cancel) {
                    cx.se(SE_CANCEL);
                    self.step = Step::PickEnd;
                } else if pressed(Action::Confirm) {
                    let (line, end) = cands[*idx];
                    let from = *station;
                    cx.se(SE_CONFIRM);
                    self.step = Step::PickNew(ExtendDraft {
                        line,
                        end,
                        from,
                        hover: None,
                    });
                }
            }
            Step::PickNew(draft) => {
                if pressed(Action::Cancel) {
                    cx.se(SE_CANCEL);
                    self.step = Step::PickEnd;
                    return false;
                }
                update_hover_extend(draft, cursor, cx.view);
                if pressed(Action::ToggleBend)
                    && let Some((_, b)) = &mut draft.hover
                {
                    *b = b.reversed();
                    cx.se(SE_CURSOR);
                }
                if pressed(Action::Confirm) {
                    self.confirm_extend(cursor, cx);
                }
            }
        }
        false
    }

    fn choose(&mut self, cx: &mut Ctx<'_, '_>) {
        let Step::Choose { sel } = self.step else {
            return;
        };
        if sel == 0 {
            if !self.create_ok {
                cx.refuse(txt::ERR_NO_SLOT);
                return;
            }
            cx.se(SE_CONFIRM);
            cx.event(UiEvent::MenuOpened(MenuAnchor::BuildCreate));
            self.step = Step::Create(CreateDraft::default());
        } else {
            match self.extend_err {
                Some(txt::ERR_LOCKED) => {
                    cx.se(tm_assets::sound::SE_ERROR);
                    let mut buf = [0u8; 4];
                    cx.band.show_error(
                        cx.now,
                        txt::ERR_LOCKED,
                        &[TextArg {
                            name: "n",
                            value: decimal(u64::from(self.extend_lock.unwrap_or(0)), &mut buf),
                        }],
                        cx.dev,
                    );
                }
                Some(id) => cx.refuse(id),
                None => {
                    cx.se(SE_CONFIRM);
                    cx.event(UiEvent::MenuOpened(MenuAnchor::BuildExtend));
                    self.step = Step::PickEnd;
                }
            }
        }
    }

    fn confirm_create(&mut self, cursor: GridPos, cx: &mut Ctx<'_, '_>) {
        let Step::Create(draft) = &mut self.step else {
            return;
        };
        let Some(x) = station_at(cx.view, cursor) else {
            cx.refuse(txt::ERR_NOT_STATION);
            return;
        };
        if draft.selected.len() >= 2 && draft.selected.last() == Some(&x) {
            let cmd = Command::CreateLine {
                stations: draft.selected.clone(),
                bends: draft.bends.clone(),
            };
            match cx.view.validate(&cmd) {
                Ok(preview) => {
                    let top =
                        confirm_on_top(cx.view, &draft.selected, &draft.bends, cursor, cx.cam);
                    self.open_confirm(cmd, top, preview, cx);
                }
                Err(e) => cx.error(e),
            }
            return;
        }
        if let Err(e) = cx.view.validate_line_draft(&draft.selected, x) {
            cx.error(e);
            return;
        }
        let bend = match (draft.hover, draft.selected.last()) {
            (Some((s, b)), _) if s == x => b,
            (_, Some(&prev)) => cx.view.default_bend(prev, x),
            (_, None) => Bend::StraightFirst,
        };
        if !draft.selected.is_empty() {
            let mut stations = draft.selected.clone();
            stations.push(x);
            let mut bends = draft.bends.clone();
            bends.push(bend);
            if !cx.permitted(&Command::CreateLine { stations, bends }) {
                return;
            }
            draft.bends.push(bend);
        }
        draft.selected.push(x);
        draft.hover = None;
        cx.se(SE_CONFIRM);
    }

    fn pick_end(&mut self, cursor: GridPos, cx: &mut Ctx<'_, '_>) {
        let Some(s) = station_at(cx.view, cursor) else {
            cx.refuse(txt::ERR_NOT_END);
            return;
        };
        let mut cands = Vec::new();
        for l in cx.view.lines() {
            for (end, st) in cx.view.line_ends(l.id) {
                if st == s {
                    cands.push((l.id, end));
                }
            }
        }
        match cands.as_slice() {
            [] => cx.refuse(txt::ERR_NOT_END),
            &[(line, end)] => {
                cx.se(SE_CONFIRM);
                self.step = Step::PickNew(ExtendDraft {
                    line,
                    end,
                    from: s,
                    hover: None,
                });
            }
            _ => {
                cx.se(SE_CONFIRM);
                self.step = Step::Which {
                    station: s,
                    cands,
                    idx: 0,
                };
            }
        }
    }

    fn confirm_extend(&mut self, cursor: GridPos, cx: &mut Ctx<'_, '_>) {
        let Step::PickNew(draft) = &self.step else {
            return;
        };
        let Some(x) = station_at(cx.view, cursor) else {
            cx.refuse(txt::ERR_NOT_STATION);
            return;
        };
        let bend = match draft.hover {
            Some((s, b)) if s == x => b,
            _ => cx.view.default_bend(draft.from, x),
        };
        let cmd = Command::ExtendLine {
            line: draft.line,
            end: draft.end,
            station: x,
            bend,
        };
        match cx.view.validate(&cmd) {
            Ok(preview) => {
                let top = confirm_on_top(cx.view, &[draft.from, x], &[bend], cursor, cx.cam);
                self.open_confirm(cmd, top, preview, cx);
            }
            Err(e) => cx.error(e),
        }
    }

    fn open_confirm(
        &mut self,
        cmd: Command,
        top: bool,
        preview: tm_core::CommandPreview,
        cx: &mut Ctx<'_, '_>,
    ) {
        cx.se(SE_CONFIRM);
        cx.event(UiEvent::MenuOpened(MenuAnchor::LineConfirm));
        self.confirm = Some(LineConfirm {
            yes: true,
            guard: Guard::from(cx.now),
            top,
            pass: preview.new_pass_through > 0,
            submitting: false,
            cost: preview.cost,
            cmd,
        });
    }

    fn handle_confirm(&mut self, input: &ActionFrame, cx: &mut Ctx<'_, '_>) {
        let Some(c) = &mut self.confirm else { return };
        if c.submitting {
            return;
        }
        let dx = step_x(input);
        if dx != 0 && c.yes != (dx > 0) {
            c.yes = dx > 0;
            cx.se(SE_CURSOR);
        }
        if input.pressed.contains(Action::Cancel) {
            cx.se(SE_CANCEL);
            self.close_confirm(cx);
            return;
        }
        if !input.pressed.contains(Action::Confirm) || !c.guard.open(cx.now) {
            return;
        }
        if !c.yes {
            cx.se(SE_CONFIRM);
            self.close_confirm(cx);
            return;
        }
        if cx.submit(c.cmd.clone()) {
            c.submitting = true;
        } else {
            self.close_confirm(cx);
        }
    }

    fn close_confirm(&mut self, cx: &mut Ctx<'_, '_>) {
        self.confirm = None;
        cx.event(UiEvent::MenuClosed(MenuAnchor::LineConfirm));
    }

    /// 送信の結果を受ける [13 §3.9]。成功ならろせんモードを終える(true)。
    pub(crate) fn observe(
        &mut self,
        ev: &GameEvent,
        now: MonoTime,
        band: &mut Band,
        dev: InputDevice,
        events: &mut Vec<UiEvent>,
    ) -> bool {
        let Some(c) = &self.confirm else { return false };
        if !c.submitting {
            return false;
        }
        let ours = match (&c.cmd, *ev) {
            (
                Command::CreateLine { stations, .. },
                GameEvent::LineCreated { station_count, .. },
            ) => stations.len() == station_count as usize,
            (
                &Command::ExtendLine {
                    line, end, station, ..
                },
                GameEvent::LineExtended {
                    line: l,
                    end: e,
                    station: s,
                    ..
                },
            ) => (line, end, station) == (l, e, s),
            _ => false,
        };
        match *ev {
            GameEvent::LineCreated { .. } | GameEvent::LineExtended { .. } if ours => {
                self.confirm = None;
                events.push(UiEvent::MenuClosed(MenuAnchor::LineConfirm));
                events.push(UiEvent::MenuClosed(MenuAnchor::BuildMode));
                true
            }
            GameEvent::CommandRejected {
                kind: CommandKind::CreateLine | CommandKind::ExtendLine,
                reason,
            } => {
                self.confirm = None;
                events.push(UiEvent::MenuClosed(MenuAnchor::LineConfirm));
                show_error(band, now, reason, dev);
                false
            }
            _ => false,
        }
    }

    /// 下の帯の左の文と右の操作のヒント。
    pub(crate) fn band_mode(&self) -> (Option<TextId>, BandChips) {
        if self.confirm.is_some() {
            return (None, BandChips::Hint(txt::BUILD_CHOOSE_HINT));
        }
        let (t, c) = match &self.step {
            Step::Choose { .. } => (txt::BUILD_CHOOSE_TITLE, txt::BUILD_CHOOSE_HINT),
            Step::Create(d) if d.selected.is_empty() => (txt::BUILD_CREATE_FIRST, txt::BUILD_HINT),
            Step::Create(_) => (txt::BUILD_CREATE_NEXT, txt::BUILD_HINT),
            Step::PickEnd => (txt::BUILD_EXTEND_END, txt::BUILD_END_HINT),
            Step::Which { .. } => (txt::BUILD_EXTEND_WHICH, txt::BUILD_WHICH_HINT),
            Step::PickNew(_) => (txt::BUILD_EXTEND_NEW, txt::BUILD_HINT),
        };
        (Some(t), BandChips::Hint(c))
    }

    /// 描画の材料を作る(毎フレーム。文は変わったときだけ並べ直す)。`refreshed` は HUD の 10 Hz。
    pub(crate) fn prepare(
        &mut self,
        view: GameView<'_>,
        cursor: GridPos,
        refreshed: bool,
        dev: InputDevice,
    ) {
        let body = FontId::Body12;
        self.choices[0].set(txt::BUILD_CHOOSE_CREATE, &[], body, TAG_W, dev);
        self.choices[1].set(txt::BUILD_CHOOSE_EXTEND, &[], body, TAG_W, dev);
        self.finish
            .set(txt::BUILD_CREATE_FINISH, &[], body, TAG_W, dev);
        self.yes.set(txt::COMMON_YES, &[], body, TAG_W, dev);
        self.no.set(txt::COMMON_NO, &[], body, TAG_W, dev);
        let inner = LINE_CONFIRM_W as u16 - 32;
        self.title
            .set(txt::BUILD_CONFIRM_TITLE, &[], body, inner, dev);
        self.pass
            .set(txt::BUILD_CONFIRM_PASS, &[], body, inner, dev);
        if let Some(c) = &self.confirm
            && !c.cost.is_zero()
        {
            self.confirm_num.set(c.cost, &FormatStyle::DEFAULT, body);
            self.confirm_cost.set(
                txt::BUILD_COST,
                &[TextArg {
                    name: "money",
                    value: &self.confirm_num.text,
                }],
                body,
                inner,
                dev,
            );
        }
        self.preview.clear();
        let mut used = 0;
        let mut seg =
            |from: StationId, to: StationId, bend: Bend, solid: &mut Vec<Vec<GridPos>>| {
                if solid.len() <= used {
                    solid.push(Vec::new());
                }
                view.preview_track(from, to, bend, &mut solid[used]);
                used += 1;
            };
        match &mut self.step {
            Step::Create(d) => {
                update_hover_create(d, cursor, view);
                for (w, &b) in d.selected.windows(2).zip(&d.bends) {
                    seg(w[0], w[1], b, &mut self.solid);
                }
                if let (Some(&last), Some((s, b))) = (d.selected.last(), d.hover)
                    && self.confirm.is_none()
                {
                    view.preview_track(last, s, b, &mut self.preview);
                }
                while self.tags.len() < d.selected.len() {
                    let mut t = NumText::default();
                    t.set_people(self.tags.len() as u64 + 1, body);
                    self.tags.push(t);
                }
            }
            Step::PickNew(d) => {
                update_hover_extend(d, cursor, view);
                if let Some((s, b)) = d.hover {
                    if self.confirm.is_some() {
                        seg(d.from, s, b, &mut self.solid);
                    } else {
                        view.preview_track(d.from, s, b, &mut self.preview);
                    }
                }
            }
            _ => {}
        }
        self.solid.truncate(used);
        if self.cost.aim(&self.step, self.confirm.as_ref()) || refreshed {
            self.cost.evaluate(view, dev);
        }
    }

    /// 情報の段に線路の値段を描く(層 5)。出さないなら false(ふつうの情報の段を描く)。
    pub(crate) fn draw_cost(&self, b: &mut DrawBuilder<'_>) -> bool {
        self.cost.draw(b)
    }

    fn draft_color(&self, view: GameView<'_>) -> Color {
        match &self.step {
            Step::PickNew(d) => line_color(d.line),
            Step::Which { cands, idx, .. } => line_color(cands[*idx].0),
            _ => line_color(LineId::from_index(view.lines().len())),
        }
    }

    /// 確認ダイアログの矩形(出ていなければ `None`)。カメラがカーソルをこの下に入れないために使う。
    pub(crate) fn dialog_area(&self) -> Option<tm_platform_api::Rect> {
        self.confirm.as_ref().map(dialog_rect)
    }

    /// マップの上の完成図と印(層 3)。線と点は `MAP_SCALE` 倍の太さで描く。
    pub(crate) fn draw_map(
        &self,
        b: &mut DrawBuilder<'_>,
        view: GameView<'_>,
        now: MonoTime,
        cam: Cam,
    ) {
        let color = self.draft_color(view);
        for path in &self.solid {
            draw_solid(b, path, color, cam);
        }
        for path in &self.solid {
            for t in [path.first(), path.last()].into_iter().flatten() {
                let (x, y) = tile_center(*t, cam);
                b.map_sprite(sprite::STN_BODY, x, y, palette::WHITE);
            }
        }
        if self.preview.len() > 1 {
            let (o, i) = (3 * MAP_SCALE, 2 * MAP_SCALE);
            for &t in &self.preview[1..self.preview.len() - 1] {
                let (x, y) = tile_center(t, cam);
                b.fill(
                    rect(x - o, y - o, 2 * o as u32, 2 * o as u32),
                    palette::BLACK,
                );
                b.fill(rect(x - i, y - i, 2 * i as u32, 2 * i as u32), color);
            }
        }
        let ring = |b: &mut DrawBuilder<'_>, s: StationId, c: Color| {
            if let Some(st) = view.station(s) {
                b.nine_slice_scaled(&sprite::NS_UI_SEL, tile_ring(st.pos, cam), c, MAP_SCALE);
            }
        };
        match &self.step {
            Step::Create(d) => {
                for (i, &s) in d.selected.iter().enumerate() {
                    let Some(st) = view.station(s) else { continue };
                    let (x, y) = tile_screen(st.pos, cam);
                    let blk = &self.tags[i].block;
                    let w = i32::from(blk.width);
                    let ty = y + (TILE - TAG_H) / 2;
                    b.fill(
                        rect(x - w - 4, ty, (w + 4) as u32, TAG_H as u32),
                        palette::TAG_BG,
                    );
                    b.text(blk, x - w - 2, ty, palette::TEXT);
                }
                if d.selected.len() >= 2
                    && self.confirm.is_none()
                    && let Some(st) = d.selected.last().and_then(|&s| view.station(s))
                {
                    let (cx, cy) = tile_center(st.pos, cam);
                    let blk = &self.finish.block;
                    let w = i32::from(blk.width);
                    let x = crate::anim::clamp_to_map(cx - w / 2, w, crate::layout::MAP_W);
                    let y = if cy - TILE - TAG_H < MAP_Y {
                        cy + TILE
                    } else {
                        cy - TILE - TAG_H
                    };
                    b.fill(
                        rect(x - 4, y, (w + 8) as u32, TAG_H as u32),
                        palette::TAG_BG,
                    );
                    let c = if self.cost.finish_short {
                        palette::TEXT_DISABLED
                    } else {
                        crate::draw::map::pulse(now, 1000, 176)
                    };
                    b.text(blk, x, y, c);
                }
            }
            Step::PickEnd => {
                for l in view.lines() {
                    for (_, s) in view.line_ends(l.id) {
                        ring(b, s, line_color(l.id));
                    }
                }
            }
            Step::Which { cands, idx, .. } => {
                if let Some(line) = view.line(cands[*idx].0) {
                    for &s in line.stations {
                        ring(b, s, palette::WHITE);
                    }
                }
            }
            Step::PickNew(d) => ring(b, d.from, palette::WHITE),
            Step::Choose { .. } => {}
        }
    }

    /// 選択肢の箱(下の帯の上)。
    pub(crate) fn draw_ui(&self, b: &mut DrawBuilder<'_>) {
        let Step::Choose { sel } = self.step else {
            return;
        };
        b.nine_slice(&sprite::NS_UI_WIN, CHOOSE, palette::WHITE);
        let ok = [self.create_ok, self.extend_err.is_none()];
        for (i, t) in self.choices.iter().enumerate() {
            let r = choice_rect(i as i32);
            if i == sel {
                b.nine_slice(&sprite::NS_UI_SEL, r, palette::WHITE);
                b.sprite(
                    sprite::UI_MENU_CURSOR,
                    r.x + 4,
                    r.y + CHOICE_H / 2,
                    palette::WHITE,
                );
            }
            let c = if ok[i] {
                palette::TEXT
            } else {
                palette::TEXT_DISABLED
            };
            b.text(&t.block, r.x + 16, r.y + 2, c);
        }
    }

    /// highlight を置く部分(選択肢の箱と行、確認の「はい」)。見えていなければ `None` [13 §3.13]。
    pub(crate) fn part_rect(&self, part: MenuAnchor) -> Option<tm_platform_api::Rect> {
        if let Some(c) = &self.confirm {
            let r = dialog_rect(c);
            return (part == MenuAnchor::LineConfirm)
                .then(|| rect(yes_x(r), button_y(r), BUTTON_W as u32, BUTTON_H as u32));
        }
        let Step::Choose { .. } = self.step else {
            return None;
        };
        match part {
            MenuAnchor::BuildMode => Some(CHOOSE),
            MenuAnchor::BuildCreate => Some(choice_rect(0)),
            MenuAnchor::BuildExtend => Some(choice_rect(1)),
            _ => None,
        }
    }

    /// 確認ダイアログ(層 8)。
    pub(crate) fn draw_dialog(&self, b: &mut DrawBuilder<'_>) {
        let Some(c) = &self.confirm else { return };
        let r = dialog_rect(c);
        b.nine_slice(&sprite::NS_UI_WIN, r, palette::WHITE);
        let line = i32::from(self.title.block.line_height);
        b.text(&self.title.block, r.x + 16, r.y + 6, palette::TEXT);
        let mut y = r.y + 6 + line;
        if !c.cost.is_zero() {
            b.text(&self.confirm_cost.block, r.x + 16, y, palette::TEXT);
            y += line;
        }
        if c.pass {
            b.text(&self.pass.block, r.x + 16, y, palette::MONEY);
        }
        let by = button_y(r);
        for (blk, x, on) in [
            (&self.no.block, no_x(r), !c.yes),
            (&self.yes.block, yes_x(r), c.yes),
        ] {
            if on {
                b.nine_slice(
                    &sprite::NS_UI_SEL,
                    rect(x, by, BUTTON_W as u32, BUTTON_H as u32),
                    palette::WHITE,
                );
                b.sprite(
                    sprite::UI_MENU_CURSOR,
                    x + 4,
                    by + BUTTON_H / 2,
                    palette::WHITE,
                );
            }
            b.text(
                blk,
                x + (BUTTON_W - i32::from(blk.width)) / 2,
                by + 2,
                palette::TEXT,
            );
        }
    }
}

/// 選択肢の `i` 行目。
fn choice_rect(i: i32) -> tm_platform_api::Rect {
    rect(
        CHOOSE.x + 4,
        CHOOSE.y + 4 + CHOICE_H * i,
        CHOOSE.w - 8,
        CHOICE_H as u32,
    )
}

/// 路線の確認の箱。高さは文の行数(「あとで消せないよ」、値段、「通過する駅」)から [13 §3.5]。
fn dialog_rect(c: &LineConfirm) -> tm_platform_api::Rect {
    let h = line_confirm_h(1 + u8::from(!c.cost.is_zero()) + u8::from(c.pass));
    let y = if c.top {
        BOX_TOP_Y
    } else {
        BOX_BOTTOM - h as i32
    };
    rect(LINE_CONFIRM_X, y, LINE_CONFIRM_W, h)
}

fn button_y(r: tm_platform_api::Rect) -> i32 {
    r.y + r.h as i32 - 6 - BUTTON_H
}

fn no_x(r: tm_platform_api::Rect) -> i32 {
    r.x + r.w as i32 / 2 - 24 - BUTTON_W
}

fn yes_x(r: tm_platform_api::Rect) -> i32 {
    r.x + r.w as i32 / 2 + 24
}

fn update_hover_create(d: &mut CreateDraft, cursor: GridPos, view: GameView<'_>) {
    let Some(&last) = d.selected.last() else {
        d.hover = None;
        return;
    };
    let target = view
        .station_at(cursor)
        .filter(|&s| s != last && view.validate_line_draft(&d.selected, s).is_ok());
    match (target, d.hover) {
        (Some(s), Some((h, _))) if s == h => {}
        (Some(s), _) => d.hover = Some((s, view.default_bend(last, s))),
        (None, _) => d.hover = None,
    }
}

fn update_hover_extend(d: &mut ExtendDraft, cursor: GridPos, view: GameView<'_>) {
    let target = view
        .station_at(cursor)
        .filter(|&s| s != d.from && view.line(d.line).is_some_and(|l| !l.stations.contains(&s)));
    match (target, d.hover) {
        (Some(s), Some((h, _))) if s == h => {}
        (Some(s), _) => d.hover = Some((s, view.default_bend(d.from, s))),
        (None, _) => d.hover = None,
    }
}

/// 確認ダイアログを上に出すか。完成図のマス(作りかけの全駅と線路)の画面の y の平均が表示域の下半分なら上 [13 §3.9]。
/// ただしカーソル(最後に選んだ駅)に重なる側には出さない(いちばん高い 3 行の箱で調べる)。`cam` はカメラの目標の位置。
fn confirm_on_top(
    view: GameView<'_>,
    stations: &[StationId],
    bends: &[Bend],
    cursor: GridPos,
    cam: Cam,
) -> bool {
    let mut tiles = Vec::new();
    let mut seg = Vec::new();
    for (w, &b) in stations.windows(2).zip(bends) {
        view.preview_track(w[0], w[1], b, &mut seg);
        tiles.extend_from_slice(&seg);
    }
    tiles.sort_by_key(|t| (t.y, t.x));
    tiles.dedup();
    let n = tiles.len() as i32;
    let sum: i32 = tiles.iter().map(|&t| tile_center(t, cam).1 - MAP_Y).sum();
    let top = n > 0 && 2 * sum >= MAP_H * n;
    let h = line_confirm_h(3) as i32;
    let (_, cy) = tile_screen(cursor, cam);
    let covers = |y0: i32| cy < y0 + h && y0 < cy + TILE;
    match top {
        true if covers(BOX_TOP_Y) => false,
        false if covers(BOX_BOTTOM - h) => true,
        _ => top,
    }
}

fn draw_solid(b: &mut DrawBuilder<'_>, path: &[GridPos], c: Color, cam: Cam) {
    let half = 2 * MAP_SCALE;
    for w in path.windows(2) {
        let (x0, y0) = tile_center(w[0], cam);
        let (x1, y1) = tile_center(w[1], cam);
        if x0 == x1 || y0 == y1 {
            b.fill(
                rect(
                    x0.min(x1) - half,
                    y0.min(y1) - half,
                    (x0 - x1).unsigned_abs() + 2 * half as u32,
                    (y0 - y1).unsigned_abs() + 2 * half as u32,
                ),
                c,
            );
        } else {
            for d in -half..half {
                b.push(DrawCmd::Line {
                    x0: x0 + d,
                    y0,
                    x1: x1 + d,
                    y1,
                    color: c,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tm_core::{BalanceTable, Game, GameConfig};

    use super::*;

    fn game(first: [GridPos; 3]) -> Game {
        let mut plan = crate::tests::plan();
        plan[..3].copy_from_slice(&first);
        Game::with_station_plan(
            GameConfig {
                seed: 1,
                balance: Arc::new(BalanceTable::builtin()),
            },
            plan,
        )
    }

    #[test]
    fn dialog_goes_to_the_side_without_the_draft_on_screen() {
        let g = game([
            GridPos { x: 4, y: 12 },
            GridPos { x: 12, y: 12 },
            GridPos { x: 12, y: 5 },
        ]);
        let s = [StationId(0), StationId(1), StationId(2)];
        let b = [Bend::StraightFirst; 2];
        let at = |x, y| GridPos { x, y };
        let cam = Cam::at(0, 4 * TILE);
        assert!(
            confirm_on_top(g.view(), &s[..2], &b[..1], at(12, 12), cam),
            "表示域の下半分の完成図なら上に出す"
        );
        assert!(
            !confirm_on_top(
                g.view(),
                &s[..2],
                &b[..1],
                at(12, 12),
                Cam::at(0, 12 * TILE)
            ),
            "同じ完成図でも、カメラが下にあって画面の上半分に見えていれば下に出す"
        );
        assert!(
            !confirm_on_top(g.view(), &s, &b, at(12, 5), cam),
            "完成図は下半分でも、上の箱がカーソルに重なるなら下に出す"
        );
        assert!(
            confirm_on_top(g.view(), &s, &b, at(12, 5), Cam::at(0, 0)),
            "カーソルが箱にかからなければ完成図の反対側"
        );
    }

    #[test]
    fn cost_aim_changes_with_the_draft_hover_and_bend() {
        let mut c = DraftCost::default();
        let mut d = CreateDraft {
            selected: vec![StationId(0)],
            bends: Vec::new(),
            hover: None,
        };
        assert!(!c.aim(&Step::Create(d.clone()), None), "対象なし");
        d.hover = Some((StationId(1), Bend::StraightFirst));
        assert!(c.aim(&Step::Create(d.clone()), None));
        assert!(
            !c.aim(&Step::Create(d.clone()), None),
            "同じなら作り直さない"
        );
        d.hover = Some((StationId(1), Bend::DiagonalFirst));
        assert!(c.aim(&Step::Create(d.clone()), None), "曲がり方");
        d.hover = Some((StationId(2), Bend::DiagonalFirst));
        assert!(c.aim(&Step::Create(d.clone()), None), "カーソルの先の駅");
        d.selected.push(StationId(2));
        d.bends.push(Bend::DiagonalFirst);
        d.hover = None;
        assert!(c.aim(&Step::Create(d.clone()), None), "決めた駅だけ");
        assert!(c.aim(&Step::PickEnd, None));
        assert!(!c.aim(&Step::PickEnd, None));
    }
}
