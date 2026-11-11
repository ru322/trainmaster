//! チュートリアルの状態機械(13 §6.2、§6.5〜§6.8)。

use tm_assets::text::TextId;
use tm_core::{
    BuyMode, Command, CommandError, Feature, GameEvent, GameView, ParamKey, ParamSet, StationId,
    TrainId,
};

use crate::anchor::{MenuAnchor, UiAnchor};
use crate::directive::{DirectiveKind, TutorialDirective, TutorialStageId};
use crate::location::{BuildKind, BuildLoc, UiLocation};
use crate::save::{SavedLatches, SavedSide, SavedStep, TutorialSave};
use crate::stages::{
    Already, DoneCond, GuideDef, GuideTarget, Hl, MAIN_STAGES, SETTINGS_STAGE, SkipCond, StageDef,
    Trigger, find_stage, page_of,
};
use crate::ui_event::UiEvent;

/// `FanfareFinished` が来ないときに、`observe_view` の回数で待つのをやめる上限(13 §6.6)。保険が働いてもログは出さない。
pub(crate) const FANFARE_FRAME_LIMIT: u32 = 3600;

/// この駅数以上の路線を作ったら #5 を飛ばす(10 §8.4 申し送り (b))。
const SKIP_EXTEND_STATION_COUNT: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Wait,
    Explain { page: u8 },
    Guide { sub: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SideState {
    NotYet,
    Showing { page: u8 },
    Done,
}

/// 一度成り立ったら戻らない条件。
#[derive(Clone, Copy, Debug, Default)]
struct Latches {
    first_income: bool,
    purchases: u32,
    bought_max: bool,
    bought: ParamSet,
    lines_created: u32,
    max_line_station_count: u32,
    extended: bool,
    max_gauge_percent: u8,
    company_seen: u8,
    goal: bool,
    newest_station: Option<StationId>,
}

/// 最後の `observe_view` で見た値。
#[derive(Clone, Copy, Debug, Default)]
struct Seen {
    lines: u32,
    free_slot: bool,
    all_connected: bool,
    upgraded: ParamSet,
    can_afford: bool,
    unconnected: Option<StationId>,
    last_station: Option<StationId>,
    first_train: Option<TrainId>,
}

/// チュートリアルの状態機械。段階を進め、UI に出す説明・案内と、許すコマンドを決める。
#[derive(Debug)]
pub struct Tutorial {
    show: bool,
    stage_idx: usize,
    step: Step,
    unshown: bool,
    replay_pending: bool,
    latch: Latches,
    fanfare_pending: bool,
    fanfare_frames: u32,
    loc: UiLocation,
    side: SideState,
    seen: Seen,
    shown: Vec<TextId>,
    shown_stages: Vec<TutorialStageId>,
    current: Option<TutorialDirective>,
}

impl Tutorial {
    /// 「せつめいを 出す」の設定を受けて、#1 から始める。
    pub fn new(show_explanations: bool) -> Self {
        let mut t = Tutorial {
            show: show_explanations,
            stage_idx: 0,
            step: Step::Wait,
            unshown: false,
            replay_pending: false,
            latch: Latches::default(),
            fanfare_pending: false,
            fanfare_frames: 0,
            loc: UiLocation::new(),
            side: SideState::NotYet,
            seen: Seen::default(),
            shown: Vec::new(),
            shown_stages: Vec::new(),
            current: None,
        };
        t.advance();
        t
    }

    /// 「せつめいを 出す」を変える(13 §6.7)。
    pub fn set_show_explanations(&mut self, on: bool) {
        self.apply_show(on);
        self.advance();
    }

    fn apply_show(&mut self, on: bool) {
        self.show = on;
        if on {
            if matches!(self.step, Step::Guide { .. }) && self.unshown {
                self.replay_pending = true;
            }
        } else {
            self.replay_pending = false;
            if matches!(self.step, Step::Explain { .. }) {
                self.step = Step::Guide { sub: 0 };
                self.unshown = true;
            }
            if matches!(self.side, SideState::Showing { .. }) {
                self.side = SideState::Done;
            }
        }
    }

    /// これまでに見せた説明のページの文(見せた順、重複なし)。
    pub fn shown_messages(&self) -> &[TextId] {
        &self.shown
    }

    /// コアのイベントを受ける。
    pub fn observe_game(&mut self, ev: &GameEvent) {
        match *ev {
            GameEvent::TrainArrived { income, .. } => {
                self.latch.first_income |= !income.is_zero();
            }
            GameEvent::Purchased { param, mode, .. } => {
                self.latch.purchases += 1;
                self.latch.bought.insert(param);
                let max = mode == BuyMode::Max;
                self.latch.bought_max |= max;
                self.complete_guides(|d| {
                    d == DoneCond::Bought(param) || (max && d == DoneCond::BoughtMax)
                });
            }
            GameEvent::LineCreated { station_count, .. } => {
                self.latch.lines_created += 1;
                self.latch.max_line_station_count =
                    self.latch.max_line_station_count.max(station_count);
                self.complete_guides(|d| d == DoneCond::LineCreated);
            }
            GameEvent::LineExtended { .. } => {
                self.latch.extended = true;
                self.complete_guides(|d| d == DoneCond::LineExtended);
            }
            GameEvent::CompanyLevelUp { new_level, .. } => {
                self.latch.company_seen = self.latch.company_seen.max(new_level.0);
                self.start_fanfare();
            }
            GameEvent::StationSpawned { id, .. } => self.latch.newest_station = Some(id),
            GameEvent::GoalReached { .. } => {
                self.latch.goal = true;
                self.start_fanfare();
            }
            _ => {}
        }
        self.advance();
    }

    /// UI のイベントを受ける。
    pub fn observe_ui(&mut self, ev: &UiEvent) {
        self.loc.observe(ev);
        match *ev {
            UiEvent::MessageAcknowledged => self.acknowledge(),
            UiEvent::CursorOn(UiAnchor::Station(_)) => {
                self.complete_guides(|d| d == DoneCond::CursorOnStation);
            }
            UiEvent::MenuOpened(MenuAnchor::SettingsMenu) if self.side == SideState::NotYet => {
                if self.show {
                    self.side = SideState::Showing { page: 0 };
                    self.record_stage(SETTINGS_STAGE.id);
                    self.record_page(SETTINGS_STAGE.pages[0]);
                } else {
                    self.side = SideState::Done;
                }
            }
            UiEvent::MenuClosed(MenuAnchor::SettingsMenu) => {
                if matches!(self.side, SideState::Showing { .. }) {
                    self.side = SideState::Done;
                }
            }
            UiEvent::FanfareFinished => self.fanfare_pending = false,
            _ => {}
        }
        self.advance();
    }

    /// 毎フレーム 1 回、UI の update の前に呼ぶ。しきい値の条件と、動く highlight の対象を調べる。
    pub fn observe_view(&mut self, view: GameView<'_>) {
        if self.fanfare_pending {
            self.fanfare_frames += 1;
            if self.fanfare_frames >= FANFARE_FRAME_LIMIT {
                self.fanfare_pending = false;
            }
        }
        let company = view.company();
        self.latch.max_gauge_percent = self
            .latch
            .max_gauge_percent
            .max(company.gauge.display_percent);
        let (used, slots) = view.line_slots();
        let mut upgraded = ParamSet::default();
        for p in ParamKey::ALL {
            if view.is_unlocked(Feature::Param(p)) && !view.is_new(p) {
                upgraded.insert(p);
            }
        }
        let can_afford = match MAIN_STAGES.get(self.stage_idx).map(|s| s.trigger) {
            Some(Trigger::CanAfford(param, scope)) if self.step == Step::Wait => !matches!(
                view.validate(&Command::LevelUp {
                    param,
                    scope,
                    mode: BuyMode::One,
                }),
                Err(CommandError::InsufficientFunds { .. })
            ),
            _ => false,
        };
        self.seen = Seen {
            lines: used,
            free_slot: used < slots,
            all_connected: view.stations().all(|s| s.connected),
            upgraded,
            can_afford,
            unconnected: view.stations().find(|s| !s.connected).map(|s| s.id),
            last_station: view.stations().last().map(|s| s.id),
            first_train: view.trains().next().map(|t| t.id),
        };
        self.advance();
    }

    /// いま出す説明か案内。せってい を開いている間は #15(止めない)を返しうる。「しない」の間は `None`。
    pub fn directive(&self) -> Option<&TutorialDirective> {
        self.current.as_ref()
    }

    /// このコマンドを実行してよいか(13 §6.4)。止める説明の間はすべて false。
    pub fn permits(&self, cmd: &Command) -> bool {
        if !self.show {
            return true;
        }
        let Some(s) = MAIN_STAGES.get(self.stage_idx) else {
            return true;
        };
        let rule = match self.step {
            Step::Explain { .. } => return false,
            Step::Wait => s.permit_wait,
            Step::Guide { .. } => s.permit_guide,
        };
        rule.allows(cmd, self.latch.lines_created)
    }

    /// 説明を見せた段階(見せた順、重複なし)。
    pub fn shown_stages(&self) -> &[TutorialStageId] {
        &self.shown_stages
    }

    /// #14 まで終わったか。
    pub fn is_finished(&self) -> bool {
        self.stage_idx == MAIN_STAGES.len()
    }

    /// セーブに入れる進み(13 §6.8)。
    pub fn save(&self) -> TutorialSave {
        let l = &self.latch;
        TutorialSave {
            stage_idx: self.stage_idx as u8,
            step: match self.step {
                Step::Wait => SavedStep::Wait,
                Step::Explain { page } => SavedStep::Explain { page },
                Step::Guide { sub } => SavedStep::Guide { sub },
            },
            unshown: self.unshown,
            replay_pending: self.replay_pending,
            latch: SavedLatches {
                first_income: l.first_income,
                purchases: l.purchases,
                bought_max: l.bought_max,
                bought: l.bought.0,
                lines_created: l.lines_created,
                max_line_station_count: l.max_line_station_count,
                extended: l.extended,
                max_gauge_percent: l.max_gauge_percent,
                company_seen: l.company_seen,
                goal: l.goal,
                newest_station: l.newest_station.map(|s| s.0),
            },
            side: match self.side {
                SideState::NotYet => SavedSide::NotYet,
                SideState::Showing { page } => SavedSide::Showing { page },
                SideState::Done => SavedSide::Done,
            },
            shown: self
                .shown
                .iter()
                .filter_map(|&t| page_of(t))
                .map(|(s, p)| (s.0, p))
                .collect(),
            shown_stages: self.shown_stages.iter().map(|s| s.0).collect(),
        }
    }

    /// セーブから戻す(13 §6.8 UI-D-37)。説明の途中なら 1 ページ目から、#15 の途中なら未表示に戻し、
    /// ファンファーレ待ちは解く。「せつめいを 出す」は引数が正。directive は最初の `observe_view` で作る。
    pub fn restore(save: TutorialSave, show_explanations: bool) -> Self {
        let stage_idx = usize::from(save.stage_idx).min(MAIN_STAGES.len());
        let guides = MAIN_STAGES.get(stage_idx).map_or(0, |s| s.guides.len());
        let step = match save.step {
            _ if stage_idx == MAIN_STAGES.len() => Step::Wait,
            SavedStep::Wait => Step::Wait,
            SavedStep::Explain { .. } => Step::Explain { page: 0 },
            SavedStep::Guide { sub } => Step::Guide {
                sub: sub.min(guides as u8),
            },
        };
        let l = save.latch;
        let mut shown = Vec::new();
        for (stage, page) in save.shown {
            let text = find_stage(TutorialStageId(stage))
                .and_then(|s| s.pages.get(usize::from(page)).copied());
            if let Some(text) = text
                && !shown.contains(&text)
            {
                shown.push(text);
            }
        }
        let mut shown_stages = Vec::new();
        for id in save.shown_stages.into_iter().map(TutorialStageId) {
            if find_stage(id).is_some() && !shown_stages.contains(&id) {
                shown_stages.push(id);
            }
        }
        let mut t = Tutorial {
            show: show_explanations,
            stage_idx,
            step,
            unshown: save.unshown,
            replay_pending: save.replay_pending,
            latch: Latches {
                first_income: l.first_income,
                purchases: l.purchases,
                bought_max: l.bought_max,
                bought: ParamSet(l.bought),
                lines_created: l.lines_created,
                max_line_station_count: l.max_line_station_count,
                extended: l.extended,
                max_gauge_percent: l.max_gauge_percent,
                company_seen: l.company_seen,
                goal: l.goal,
                newest_station: l.newest_station.map(StationId),
            },
            fanfare_pending: false,
            fanfare_frames: 0,
            loc: UiLocation::new(),
            side: match save.side {
                SavedSide::Done => SideState::Done,
                SavedSide::NotYet | SavedSide::Showing { .. } => SideState::NotYet,
            },
            seen: Seen::default(),
            shown,
            shown_stages,
            current: None,
        };
        t.apply_show(show_explanations);
        t
    }

    fn start_fanfare(&mut self) {
        self.fanfare_pending = true;
        self.fanfare_frames = 0;
    }

    fn acknowledge(&mut self) {
        if let SideState::Showing { page } = self.side {
            let next = page + 1;
            if let Some(&text) = SETTINGS_STAGE.pages.get(usize::from(next)) {
                self.side = SideState::Showing { page: next };
                self.record_page(text);
            } else {
                self.side = SideState::Done;
            }
            return;
        }
        let (Step::Explain { page }, Some(s)) = (self.step, MAIN_STAGES.get(self.stage_idx)) else {
            return;
        };
        let next = page + 1;
        if let Some(&text) = s.pages.get(usize::from(next)) {
            self.step = Step::Explain { page: next };
            self.record_page(text);
        } else {
            self.step = Step::Guide { sub: 0 };
        }
    }

    /// 案内の `done` を満たすイベントを受けた。後の案内の `done` が先に起きたら、そこまで飛ぶ。
    fn complete_guides(&mut self, hit: impl Fn(DoneCond) -> bool) {
        let (Step::Guide { sub }, Some(s)) = (self.step, MAIN_STAGES.get(self.stage_idx)) else {
            return;
        };
        if let Some(j) = s.guides.iter().rposition(|g| hit(g.done))
            && j >= usize::from(sub)
        {
            self.step = Step::Guide { sub: j as u8 + 1 };
        }
    }

    fn advance(&mut self) {
        while let Some(s) = MAIN_STAGES.get(self.stage_idx) {
            if self.replay_pending && !self.fanfare_pending {
                self.replay_pending = false;
                self.begin_explain(s);
                break;
            }
            match self.step {
                Step::Wait => {
                    // UI-D-29: 飛ばす条件は開始条件とファンファーレより先に調べる
                    if self.skip_met(s.skip) {
                        self.next_stage();
                        continue;
                    }
                    if !self.trigger_met(s.trigger) || self.fanfare_pending {
                        break;
                    }
                    if self.show {
                        self.begin_explain(s);
                        break;
                    } else {
                        self.step = Step::Guide { sub: 0 };
                        self.unshown = true;
                    }
                }
                Step::Explain { .. } => break,
                Step::Guide { sub } => {
                    let Some(g) = s.guides.get(usize::from(sub)) else {
                        self.next_stage();
                        continue;
                    };
                    if self.skip_met(s.skip) {
                        self.next_stage();
                    } else if self.guide_done_already(g) || (!self.show && s.supersedable) {
                        self.step = Step::Guide { sub: sub + 1 };
                    } else if s.supersedable && self.next_trigger_met() {
                        self.next_stage();
                    } else {
                        break;
                    }
                }
            }
        }
        self.refresh();
    }

    fn next_stage(&mut self) {
        self.stage_idx += 1;
        self.step = Step::Wait;
        self.unshown = false;
        self.replay_pending = false;
    }

    fn begin_explain(&mut self, s: &StageDef) {
        self.step = Step::Explain { page: 0 };
        self.unshown = false;
        self.record_stage(s.id);
        self.record_page(s.pages[0]);
    }

    fn record_stage(&mut self, id: TutorialStageId) {
        if !self.shown_stages.contains(&id) {
            self.shown_stages.push(id);
        }
    }

    fn record_page(&mut self, text: TextId) {
        if !self.shown.contains(&text) {
            self.shown.push(text);
        }
    }

    fn trigger_met(&self, t: Trigger) -> bool {
        let l = &self.latch;
        match t {
            Trigger::Start | Trigger::AfterPrev => true,
            Trigger::FirstIncome => l.first_income,
            Trigger::CanAfford(..) => self.seen.can_afford,
            Trigger::PurchaseCount(n) => l.purchases >= n,
            Trigger::GaugeOrCompany { percent, level } => {
                l.max_gauge_percent >= percent || l.company_seen >= level
            }
            Trigger::CompanyLevel(n) => l.company_seen >= n,
            Trigger::Goal => l.goal,
        }
    }

    /// 打ち切りの条件: 次の段階の trigger と、ファンファーレが終わっていること。
    fn next_trigger_met(&self) -> bool {
        !self.fanfare_pending
            && MAIN_STAGES
                .get(self.stage_idx + 1)
                .is_some_and(|n| self.trigger_met(n.trigger))
    }

    fn bought(&self, p: ParamKey) -> bool {
        self.latch.bought.contains(p) || self.seen.upgraded.contains(p)
    }

    fn skip_met(&self, c: SkipCond) -> bool {
        match c {
            SkipCond::Never => false,
            SkipCond::AnyLine => self.seen.lines >= 1 || self.latch.lines_created >= 1,
            SkipCond::AllStationsConnected => {
                self.seen.all_connected
                    || self.latch.max_line_station_count >= SKIP_EXTEND_STATION_COUNT
            }
            SkipCond::AlreadyBought(p) => self.bought(p),
        }
    }

    fn guide_done_already(&self, g: &GuideDef) -> bool {
        let done = match g.done {
            DoneCond::Bought(p) => self.bought(p),
            DoneCond::BoughtMax => self.latch.bought_max,
            DoneCond::LineExtended => self.latch.extended,
            DoneCond::CursorOnStation | DoneCond::LineCreated | DoneCond::Never => false,
        };
        done || match g.already {
            Already::Never => false,
            Already::FreeLineSlot => self.seen.free_slot,
            Already::LinesAtLeast(n) => self.seen.lines >= n,
        }
    }

    fn refresh(&mut self) {
        self.current = if !self.show {
            None
        } else if let (true, SideState::Showing { page }) = (self.loc.settings_open, self.side) {
            Some(self.explanation(&SETTINGS_STAGE, page, false))
        } else {
            self.main_directive()
        };
    }

    fn main_directive(&self) -> Option<TutorialDirective> {
        let s = MAIN_STAGES.get(self.stage_idx)?;
        match self.step {
            Step::Explain { page } => Some(self.explanation(s, page, true)),
            Step::Guide { sub } => s.guides.get(usize::from(sub)).map(|g| self.guide(s, g)),
            Step::Wait => s.pre_guide.as_ref().map(|g| self.guide(s, g)),
        }
    }

    fn explanation(&self, s: &StageDef, page: u8, pauses_game: bool) -> TutorialDirective {
        let i = usize::from(page);
        TutorialDirective {
            stage: s.id,
            kind: DirectiveKind::Explanation,
            message: s.pages[i],
            page,
            page_count: s.pages.len() as u8,
            highlight: self.resolve(s.highlights[i]),
            pauses_game,
        }
    }

    fn guide(&self, s: &StageDef, g: &GuideDef) -> TutorialDirective {
        TutorialDirective {
            stage: s.id,
            kind: DirectiveKind::Guide,
            message: g.text,
            page: 0,
            page_count: 1,
            highlight: self.target(g.target),
            pauses_game: false,
        }
    }

    fn resolve(&self, h: Hl) -> Option<UiAnchor> {
        match h {
            Hl::None => None,
            Hl::Fixed(a) => Some(a),
            Hl::UnconnectedStation => self.seen.unconnected.map(UiAnchor::Station),
            Hl::NewestStation => self
                .latch
                .newest_station
                .or(self.seen.last_station)
                .map(UiAnchor::Station),
            Hl::FirstTrain => self.seen.first_train.map(UiAnchor::Train),
            Hl::FirstStation => Some(UiAnchor::Station(StationId(0))),
        }
    }

    fn target(&self, t: GuideTarget) -> Option<UiAnchor> {
        let loc = &self.loc;
        let m = match t {
            GuideTarget::Hl(h) => return self.resolve(h),
            GuideTarget::Buy(_) if !loc.panel_open => MenuAnchor::UpgradePanel,
            GuideTarget::Buy(p) if loc.tab != Some(p.category()) => {
                MenuAnchor::UpgradeTab(p.category())
            }
            GuideTarget::Buy(p) => MenuAnchor::UpgradeItem(p),
            GuideTarget::BuyMax if !loc.panel_open => MenuAnchor::UpgradePanel,
            GuideTarget::BuyMax => MenuAnchor::BuyMaxHint,
            GuideTarget::Build(k) => match loc.build {
                BuildLoc::Closed => MenuAnchor::BuildMode,
                BuildLoc::Choose => match k {
                    BuildKind::Create => MenuAnchor::BuildCreate,
                    BuildKind::Extend => MenuAnchor::BuildExtend,
                },
                BuildLoc::Draft(BuildKind::Create) => return None,
                BuildLoc::Draft(BuildKind::Extend) => {
                    return self.resolve(Hl::UnconnectedStation);
                }
                BuildLoc::Confirm(_) => MenuAnchor::LineConfirm,
            },
        };
        Some(UiAnchor::Menu(m))
    }
}

#[cfg(test)]
impl Tutorial {
    /// テスト用の入口: 本線の `idx` 番目の段階の待つ間にする。
    pub(crate) fn jump_to_wait(&mut self, idx: usize) {
        self.stage_idx = idx;
        self.step = Step::Wait;
        self.unshown = false;
        self.replay_pending = false;
        self.refresh();
    }

    pub(crate) fn stage(&self) -> Option<TutorialStageId> {
        MAIN_STAGES.get(self.stage_idx).map(|s| s.id)
    }

    pub(crate) fn fanfare_pending(&self) -> bool {
        self.fanfare_pending
    }
}

#[cfg(test)]
mod tests;
