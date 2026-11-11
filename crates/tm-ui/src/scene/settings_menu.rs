//! せってい、読み直し、リセット確認、終わる確認 [13 §2.8、§3.3、01 §11.3]。

use tm_assets::font::FontId;
use tm_assets::sound::{SE_CANCEL, SE_CONFIRM, SE_CURSOR, SE_RESET};
use tm_assets::sprite;
use tm_assets::text::{TextArg, TextId};
use tm_assets::txt;
use tm_input::{Action, ActionFrame, InputDevice};
use tm_platform_api::{AudioBus, MonoTime, SoundId};
use tm_tutorial::{
    MenuAnchor, SettingsItem, TutorialStageId, UiAnchor, UiEvent, stage_pages, stage_title,
};

use super::explain::{draw_page_box, page_rect};
use super::play::{Ctx, UiSound, step_x, step_y};
use crate::draw::palette;
use crate::draw::{DrawBuilder, rect};
use crate::hud::{BandChips, decimal};
use crate::input_ctx::Guard;
use crate::layout::{
    EXPLAIN_PAD, EXPLAIN_W, EXPLAIN_X, SETTINGS, SETTINGS_ROW_H, SETTINGS_ROWS, SUB_CONFIRM,
    bottom, right,
};
use crate::request::AppRequest;
use crate::settings::{Settings, VOLUME_MAX};
use crate::text::DynText;
use crate::text::numbers::NumText;

/// 項目の順(01 §11.3)。「ゲームを終わる」はリセットの 1 つ上、リセットは一番下 [K-02、UW-13]。
pub(crate) const ITEMS: [SettingsItem; 7] = [
    SettingsItem::Continue,
    SettingsItem::MusicVolume,
    SettingsItem::SoundVolume,
    SettingsItem::ReplayExplanations,
    SettingsItem::ShowExplanations,
    SettingsItem::Quit,
    SettingsItem::Reset,
];
/// 区切り線の下に置く最初の項目(戻れない操作。13 §3.10)。
const SEP_AT: usize = 5;
/// 見出しの上の余白。
const PAD: i32 = 8;
/// 1 行目の上端(見出し 20 と間 4 の後)。
const ROWS_Y: i32 = SETTINGS.y + PAD + 20 + 4;
/// 区切り線の高さ。
const SEP_H: i32 = 8;
/// 読み直しの一覧の一度に見せる行の数。
const LIST_ROWS: usize = SETTINGS_ROWS as usize;
const INNER_W: u16 = SETTINGS.w as u16 - 16;
/// 行の左端(選んでいる枠)と文の左端。
const ROW_X: i32 = SETTINGS.x + 6;
const LABEL_X: i32 = SETTINGS.x + 22;
/// 音量と「する/しない」の ◀ の左端。
const CTRL_X: i32 = SETTINGS.x + 120;
/// 確認の「はい」「いいえ」の枠。
const BUTTON_W: i32 = 72;
const BUTTON_H: i32 = 20;

/// せってい の中の画面。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    /// いちばん上。
    Top,
    /// 読み直しの一覧。
    Replay { sel: usize, first: usize },
    /// 読み直しのページ。
    Read {
        sel: usize,
        first: usize,
        stage: TutorialStageId,
        page: usize,
    },
    /// リセット確認(`step` は 1 か 2)。初期は「いいえ」[02 D17]。
    Reset { step: u8, yes: bool, guard: Guard },
    /// 終わる確認(`step` は 1 か 2)。初期は「いいえ」[UI-D-33]。
    Quit { step: u8, yes: bool, guard: Guard },
}

/// 確認の箱への答え。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// まだ。
    None,
    /// Cancel。
    Cancel,
    /// 「いいえ」で決めた。
    No,
    /// 「はい」で決めた。
    Yes,
}

/// 確認の箱の入力: 左右で「はい/いいえ」(変わったら `se_cursor`)、Cancel、ガードの後の決定。
pub(crate) fn answer(
    input: &ActionFrame,
    yes: &mut bool,
    guard: Guard,
    now: MonoTime,
    sounds: &mut Vec<SoundId>,
) -> Answer {
    let dx = step_x(input);
    if dx != 0 && *yes != (dx > 0) {
        *yes = dx > 0;
        sounds.push(SE_CURSOR);
    }
    if input.pressed.contains(Action::Cancel) {
        Answer::Cancel
    } else if input.pressed.contains(Action::Confirm) && guard.open(now) {
        if *yes { Answer::Yes } else { Answer::No }
    } else {
        Answer::None
    }
}

/// 「はい/いいえ」の確認の箱(リセット確認・終わる確認・タイトルの確認)。
#[derive(Default)]
pub(crate) struct ConfirmBox {
    msg: DynText,
    yes: DynText,
    no: DynText,
}

impl ConfirmBox {
    /// 文を並べる(変わったときだけ)。
    pub(crate) fn prepare(&mut self, id: TextId, dev: InputDevice) {
        let body = FontId::Body12;
        self.msg.set(id, &[], body, SUB_CONFIRM.w as u16 - 32, dev);
        self.yes.set(txt::COMMON_YES, &[], body, INNER_W, dev);
        self.no.set(txt::COMMON_NO, &[], body, INNER_W, dev);
    }

    /// いまの文(テスト用)。
    #[cfg(test)]
    pub(crate) fn text(&self) -> Option<TextId> {
        self.msg.id()
    }

    /// `SUB_CONFIRM` に描く。選んでいる方に枠と ▶。
    pub(crate) fn draw(&self, b: &mut DrawBuilder<'_>, yes: bool) {
        let r = SUB_CONFIRM;
        b.nine_slice(&sprite::NS_UI_WIN, r, palette::WHITE);
        b.text(&self.msg.block, r.x + 16, r.y + 8, palette::TEXT);
        for (blk, x, on) in [
            (&self.no.block, no_x(), !yes),
            (&self.yes.block, yes_x(), yes),
        ] {
            let by = button_y();
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

/// せってい を閉じるか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuExit {
    /// 開いたまま。
    Stay,
    /// 閉じる。
    Close,
    /// 終わる確認の 2 回目の「はい」(`QuitGame` を出した)。
    Quit,
}

/// せってい メニュー。
pub(crate) struct SettingsMenu {
    /// いまの画面。
    pub page: Page,
    /// いちばん上で選んでいる項目(`ITEMS` の番号)。初期は「つづける」。
    pub sel: usize,
    stages: Vec<TutorialStageId>,
    title: DynText,
    labels: [DynText; ITEMS.len()],
    on_off: DynText,
    music: NumText,
    sound: NumText,
    replay_title: DynText,
    empty: DynText,
    list: Vec<DynText>,
    page_text: DynText,
    page_no: DynText,
    next: DynText,
    confirm: ConfirmBox,
}

impl Default for SettingsMenu {
    fn default() -> Self {
        SettingsMenu {
            page: Page::Top,
            sel: 0,
            stages: Vec::new(),
            title: DynText::default(),
            labels: Default::default(),
            on_off: DynText::default(),
            music: NumText::default(),
            sound: NumText::default(),
            replay_title: DynText::default(),
            empty: DynText::default(),
            list: Vec::new(),
            page_text: DynText::default(),
            page_no: DynText::default(),
            next: DynText::default(),
            confirm: ConfirmBox::default(),
        }
    }
}

fn label(item: SettingsItem) -> TextId {
    match item {
        SettingsItem::Continue => txt::SETTINGS_CONTINUE,
        SettingsItem::MusicVolume => txt::SETTINGS_MUSIC,
        SettingsItem::SoundVolume => txt::SETTINGS_SOUND,
        SettingsItem::ReplayExplanations => txt::SETTINGS_REPLAY,
        SettingsItem::ShowExplanations => txt::SETTINGS_SHOW,
        SettingsItem::Quit => txt::SETTINGS_QUIT,
        SettingsItem::Reset => txt::SETTINGS_RESET,
    }
}

fn confirm_anchor(quit: bool, step: u8) -> MenuAnchor {
    if quit {
        MenuAnchor::QuitConfirm { step }
    } else {
        MenuAnchor::ResetConfirm { step }
    }
}

fn confirm_page(quit: bool, step: u8, yes: bool, guard: Guard) -> Page {
    if quit {
        Page::Quit { step, yes, guard }
    } else {
        Page::Reset { step, yes, guard }
    }
}

impl SettingsMenu {
    /// 画面(層)の番号。
    pub(crate) fn layer_key(&self) -> u32 {
        match self.page {
            Page::Top => 0,
            Page::Replay { .. } => 1,
            Page::Read { .. } => 2,
            Page::Reset { step, .. } => 2 + u32::from(step),
            Page::Quit { step, .. } => 4 + u32::from(step),
        }
    }

    /// Menu: いちばん上なら閉じ、サブ画面ならいちばん上へ [UI-D-20]。
    pub(crate) fn back_to_top(&mut self, cx: &mut Ctx<'_, '_>) -> MenuExit {
        match self.page {
            Page::Top => MenuExit::Close,
            Page::Reset { step, .. } | Page::Quit { step, .. } => {
                cx.se(SE_CANCEL);
                let quit = matches!(self.page, Page::Quit { .. });
                cx.event(UiEvent::MenuClosed(confirm_anchor(quit, step)));
                self.page = Page::Top;
                MenuExit::Stay
            }
            Page::Replay { .. } | Page::Read { .. } => {
                cx.se(SE_CANCEL);
                self.page = Page::Top;
                MenuExit::Stay
            }
        }
    }

    /// 入力を受ける(Menu は呼び出し側が処理する)。
    pub(crate) fn handle(
        &mut self,
        input: &ActionFrame,
        settings: &mut Settings,
        cx: &mut Ctx<'_, '_>,
    ) -> MenuExit {
        let pressed = |a| input.pressed.contains(a);
        match self.page {
            Page::Top => return self.handle_top(input, settings, cx),
            Page::Replay { sel, first } => {
                let n = self.stages.len();
                if pressed(Action::Cancel) {
                    cx.se(SE_CANCEL);
                    self.page = Page::Top;
                } else if pressed(Action::Confirm) && n > 0 {
                    cx.se(SE_CONFIRM);
                    self.page = Page::Read {
                        sel,
                        first,
                        stage: self.stages[sel.min(n - 1)],
                        page: 0,
                    };
                } else if n > 0 {
                    let next = (sel as i32 + step_y(input)).clamp(0, n as i32 - 1) as usize;
                    if next != sel {
                        cx.se(SE_CURSOR);
                        self.page = Page::Replay {
                            sel: next,
                            first: scroll(first, next, n),
                        };
                    }
                }
            }
            Page::Read {
                sel,
                first,
                stage,
                page,
            } => {
                if pressed(Action::Cancel) {
                    cx.se(SE_CANCEL);
                    self.page = Page::Replay { sel, first };
                } else if pressed(Action::Confirm) {
                    cx.se(SE_CONFIRM);
                    self.page = if page + 1 < stage_pages(stage).len() {
                        Page::Read {
                            sel,
                            first,
                            stage,
                            page: page + 1,
                        }
                    } else {
                        Page::Replay { sel, first }
                    };
                }
            }
            Page::Reset {
                step,
                mut yes,
                guard,
            }
            | Page::Quit {
                step,
                mut yes,
                guard,
            } => {
                let quit = matches!(self.page, Page::Quit { .. });
                let mut sounds = Vec::new();
                let a = answer(input, &mut yes, guard, cx.now, &mut sounds);
                for id in sounds {
                    cx.se(id);
                }
                self.page = confirm_page(quit, step, yes, guard);
                return self.decide(quit, step, a, cx);
            }
        }
        MenuExit::Stay
    }

    fn decide(&mut self, quit: bool, step: u8, a: Answer, cx: &mut Ctx<'_, '_>) -> MenuExit {
        match a {
            Answer::None => {}
            Answer::Cancel | Answer::No => {
                cx.se(if a == Answer::Cancel {
                    SE_CANCEL
                } else {
                    SE_CONFIRM
                });
                cx.event(UiEvent::MenuClosed(confirm_anchor(quit, step)));
                self.page = Page::Top;
            }
            Answer::Yes if step == 1 => {
                cx.se(SE_CONFIRM);
                cx.event(UiEvent::MenuClosed(confirm_anchor(quit, 1)));
                cx.event(UiEvent::MenuOpened(confirm_anchor(quit, 2)));
                self.page = confirm_page(quit, 2, false, Guard::from(cx.now));
            }
            // 13 §3.3: 終わるときは UiEvent を出さない
            Answer::Yes if quit => {
                cx.se(SE_CONFIRM);
                cx.requests.push(AppRequest::QuitGame);
                return MenuExit::Quit;
            }
            Answer::Yes => {
                cx.se(SE_RESET);
                cx.requests.push(AppRequest::ResetToTitle);
            }
        }
        MenuExit::Stay
    }

    fn handle_top(
        &mut self,
        input: &ActionFrame,
        settings: &mut Settings,
        cx: &mut Ctx<'_, '_>,
    ) -> MenuExit {
        let pressed = |a| input.pressed.contains(a);
        if pressed(Action::Cancel) {
            return MenuExit::Close;
        }
        let next = (self.sel as i32 + step_y(input)).clamp(0, ITEMS.len() as i32 - 1) as usize;
        if next != self.sel {
            self.sel = next;
            cx.se(SE_CURSOR);
            cx.event(UiEvent::CursorOn(UiAnchor::Menu(MenuAnchor::SettingsItem(
                ITEMS[next],
            ))));
        }
        let item = ITEMS[self.sel];
        let dx = step_x(input);
        match item {
            SettingsItem::MusicVolume | SettingsItem::SoundVolume if dx != 0 => {
                let (v, bus) = if item == SettingsItem::MusicVolume {
                    (&mut settings.music, AudioBus::Bgm)
                } else {
                    (&mut settings.sound, AudioBus::Se)
                };
                let nv = (i32::from(*v) + dx).clamp(0, i32::from(VOLUME_MAX)) as u8;
                if nv != *v {
                    *v = nv;
                    cx.sounds.push(UiSound::Volume(bus, nv));
                }
            }
            SettingsItem::ShowExplanations if dx != 0 => {
                let on = dx < 0;
                if on != settings.show_explanations {
                    cx.se(SE_CURSOR);
                    set_show(settings, on, cx);
                }
            }
            _ => {}
        }
        if !pressed(Action::Confirm) {
            return MenuExit::Stay;
        }
        match item {
            SettingsItem::Continue => return MenuExit::Close,
            SettingsItem::MusicVolume | SettingsItem::SoundVolume => {}
            SettingsItem::ReplayExplanations => {
                cx.se(SE_CONFIRM);
                self.stages.clear();
                if let Some(t) = cx.tutorial.as_deref() {
                    self.stages.extend_from_slice(t.shown_stages());
                }
                self.page = Page::Replay { sel: 0, first: 0 };
            }
            SettingsItem::ShowExplanations => {
                cx.se(SE_CONFIRM);
                set_show(settings, !settings.show_explanations, cx);
            }
            SettingsItem::Quit | SettingsItem::Reset => {
                let quit = item == SettingsItem::Quit;
                cx.se(SE_CONFIRM);
                cx.event(UiEvent::MenuOpened(confirm_anchor(quit, 1)));
                self.page = confirm_page(quit, 1, false, Guard::from(cx.now));
            }
        }
        MenuExit::Stay
    }

    /// 描画の材料を作る(文は変わったときだけ並べ直す)。`save_enabled` で終わる確認の文が変わる。
    pub(crate) fn prepare(&mut self, settings: &Settings, save_enabled: bool, dev: InputDevice) {
        let body = FontId::Body12;
        self.title
            .set(txt::SETTINGS_TITLE, &[], FontId::Head16, INNER_W, dev);
        self.replay_title
            .set(txt::SETTINGS_REPLAY, &[], FontId::Head16, INNER_W, dev);
        for (t, item) in self.labels.iter_mut().zip(ITEMS) {
            t.set(label(item), &[], body, INNER_W, dev);
        }
        let on = if settings.show_explanations {
            txt::SETTINGS_ON
        } else {
            txt::SETTINGS_OFF
        };
        self.on_off.set(on, &[], body, INNER_W, dev);
        self.music.set_people(u64::from(settings.music), body);
        self.sound.set_people(u64::from(settings.sound), body);
        if matches!(self.page, Page::Replay { .. } | Page::Read { .. }) {
            self.empty
                .set(txt::SETTINGS_REPLAY_EMPTY, &[], body, INNER_W, dev);
            while self.list.len() < self.stages.len() {
                self.list.push(DynText::default());
            }
            for (t, &s) in self.list.iter_mut().zip(&self.stages) {
                t.set(stage_title(s), &[], body, INNER_W, dev);
            }
        }
        match self.page {
            Page::Top | Page::Replay { .. } => {}
            Page::Read { stage, page, .. } => {
                let pages = stage_pages(stage);
                self.page_text.set(
                    pages[page.min(pages.len() - 1)],
                    &[],
                    FontId::Head16,
                    (EXPLAIN_W as i32 - 2 * EXPLAIN_PAD) as u16,
                    dev,
                );
                let (mut a, mut b) = ([0u8; 4], [0u8; 4]);
                self.page_no.set(
                    txt::EXPL_PAGE,
                    &[
                        TextArg {
                            name: "n",
                            value: decimal(page as u64 + 1, &mut a),
                        },
                        TextArg {
                            name: "m",
                            value: decimal(pages.len() as u64, &mut b),
                        },
                    ],
                    body,
                    INNER_W,
                    dev,
                );
                self.next.set(
                    super::explain::next_label(page as u8, pages.len() as u8),
                    &[],
                    body,
                    INNER_W,
                    dev,
                );
            }
            Page::Reset { step, .. } => self.confirm.prepare(
                if step == 1 {
                    txt::RESET_CONFIRM1
                } else {
                    txt::RESET_CONFIRM2
                },
                dev,
            ),
            Page::Quit { step, .. } => self.confirm.prepare(
                match (step, save_enabled) {
                    (1, true) => txt::QUIT_CONFIRM1,
                    (1, false) => txt::QUIT_CONFIRM1_NOSAVE,
                    _ => txt::QUIT_CONFIRM2,
                },
                dev,
            ),
        }
    }

    /// せってい とその下位の画面を描く(層 8)。下位の確認と読み直しのページは せってい の箱の下に出す。
    pub(crate) fn draw(&self, b: &mut DrawBuilder<'_>, settings: &Settings) {
        b.nine_slice(&sprite::NS_UI_WIN, SETTINGS, palette::WHITE);
        match self.page {
            Page::Top | Page::Reset { .. } | Page::Quit { .. } => self.draw_top(b, settings),
            Page::Replay { sel, first } | Page::Read { sel, first, .. } => {
                self.draw_list(b, sel, first)
            }
        }
        match self.page {
            Page::Reset { yes, .. } | Page::Quit { yes, .. } => self.confirm.draw(b, yes),
            Page::Read { stage, .. } => draw_page_box(
                b,
                self.read_rect(),
                &self.page_text.block,
                (stage_pages(stage).len() > 1).then_some(&self.page_no.block),
                &self.next.block,
                true,
            ),
            Page::Top | Page::Replay { .. } => {}
        }
    }

    fn draw_top(&self, b: &mut DrawBuilder<'_>, settings: &Settings) {
        let s = SETTINGS;
        b.text(&self.title.block, s.x + 12, s.y + PAD, palette::TEXT);
        for (i, item) in ITEMS.iter().enumerate() {
            let y = row_y(i);
            if i == self.sel {
                select_row(b, y);
            }
            b.text(&self.labels[i].block, LABEL_X, y + 2, palette::TEXT);
            match item {
                SettingsItem::MusicVolume => volume(b, y, settings.music, &self.music),
                SettingsItem::SoundVolume => volume(b, y, settings.sound, &self.sound),
                SettingsItem::ShowExplanations => {
                    b.sprite(sprite::ICO_TRI_LEFT, CTRL_X, y + 6, palette::WHITE);
                    let w = i32::from(self.on_off.block.width);
                    b.text(
                        &self.on_off.block,
                        CTRL_X + 40 - w / 2,
                        y + 2,
                        palette::TEXT,
                    );
                    b.sprite(sprite::ICO_TRI_RIGHT, CTRL_X + 72, y + 6, palette::WHITE);
                }
                _ => {}
            }
        }
        b.fill(
            rect(LABEL_X, row_y(SEP_AT) - SEP_H / 2, s.w - 44, 1),
            palette::TEXT_DISABLED,
        );
    }

    fn draw_list(&self, b: &mut DrawBuilder<'_>, sel: usize, first: usize) {
        let s = SETTINGS;
        b.text(&self.replay_title.block, s.x + 12, s.y + PAD, palette::TEXT);
        if self.stages.is_empty() {
            b.text(
                &self.empty.block,
                LABEL_X,
                ROWS_Y + 2,
                palette::TEXT_DISABLED,
            );
            return;
        }
        let end = (first + LIST_ROWS).min(self.stages.len());
        for i in first..end {
            let y = ROWS_Y + SETTINGS_ROW_H * (i - first) as i32;
            if i == sel {
                select_row(b, y);
            }
            if let Some(t) = self.list.get(i) {
                b.text(&t.block, LABEL_X, y + 2, palette::TEXT);
            }
        }
        if first > 0 {
            b.sprite(sprite::ICO_TRI_UP, right(s) - 16, s.y + 12, palette::WHITE);
        }
        if end < self.stages.len() {
            b.sprite(
                sprite::ICO_TRI_DOWN,
                right(s) - 16,
                bottom(s) - 10,
                palette::WHITE,
            );
        }
    }

    /// 読み直しのページの箱。#15 と同じく せってい の下に固定 [13 §3.5]。
    fn read_rect(&self) -> tm_platform_api::Rect {
        page_rect(EXPLAIN_X, self.page_text.block.lines, false)
    }

    /// いちばん上の画面か(#15 の説明はここでだけ出す)。
    pub(crate) fn is_top(&self) -> bool {
        self.page == Page::Top
    }

    /// せってい の層が描く範囲(箱と、その下の読み直しのページ・確認。テスト用)。
    #[cfg(test)]
    pub(crate) fn bounds(&self) -> tm_platform_api::Rect {
        let union = |a: tm_platform_api::Rect, b: tm_platform_api::Rect| {
            let (x, y) = (a.x.min(b.x), a.y.min(b.y));
            let r = right(a).max(right(b));
            let bt = bottom(a).max(bottom(b));
            rect(x, y, (r - x) as u32, (bt - y) as u32)
        };
        match self.page {
            Page::Top | Page::Replay { .. } => SETTINGS,
            Page::Read { .. } => union(SETTINGS, self.read_rect()),
            Page::Reset { .. } | Page::Quit { .. } => union(SETTINGS, SUB_CONFIRM),
        }
    }

    /// 下の帯の右の操作のヒント(13 §4.2: モード中はそのモードの操作)。読むページは「もどる」だけ(つぎへ は箱の中)。
    pub(crate) fn band_hint(&self) -> BandChips {
        BandChips::Hint(match self.page {
            Page::Read { .. } => txt::BUILD_END_HINT,
            Page::Top | Page::Replay { .. } | Page::Reset { .. } | Page::Quit { .. } => {
                txt::BUILD_CHOOSE_HINT
            }
        })
    }

    /// highlight を置く部分(箱、いちばん上の行、リセット確認、終わる確認)。見えていなければ `None` [13 §3.13]。
    pub(crate) fn part_rect(&self, part: MenuAnchor) -> Option<tm_platform_api::Rect> {
        match (part, self.page) {
            (MenuAnchor::SettingsMenu, _) => Some(SETTINGS),
            (MenuAnchor::SettingsItem(item), Page::Top) => {
                let i = ITEMS.iter().position(|&x| x == item)?;
                Some(row_rect(row_y(i)))
            }
            (MenuAnchor::ResetConfirm { step }, Page::Reset { step: s, .. })
            | (MenuAnchor::QuitConfirm { step }, Page::Quit { step: s, .. })
                if step == s =>
            {
                Some(SUB_CONFIRM)
            }
            _ => None,
        }
    }

    /// 確認の箱の文(テスト用)。
    #[cfg(test)]
    pub(crate) fn confirm_text(&self) -> Option<TextId> {
        match self.page {
            Page::Reset { .. } | Page::Quit { .. } => self.confirm.text(),
            _ => None,
        }
    }

    /// 読み直しの一覧に出す段階(テスト用)。
    #[cfg(test)]
    pub(crate) fn set_stages(&mut self, stages: &[TutorialStageId]) {
        self.stages = stages.to_vec();
    }
}

/// いちばん上の画面の `i` 行目の上端。区切り線より下は 8 下げる。
fn row_y(i: usize) -> i32 {
    let extra = if i >= SEP_AT { SEP_H } else { 0 };
    ROWS_Y + SETTINGS_ROW_H * i as i32 + extra
}

/// 行の枠(選んでいる行、highlight)。
fn row_rect(y: i32) -> tm_platform_api::Rect {
    rect(ROW_X, y, SETTINGS.w - 12, SETTINGS_ROW_H as u32)
}

fn select_row(b: &mut DrawBuilder<'_>, y: i32) {
    b.nine_slice(&sprite::NS_UI_SEL, row_rect(y), palette::WHITE);
    b.sprite(
        sprite::UI_MENU_CURSOR,
        ROW_X + 4,
        y + SETTINGS_ROW_H / 2,
        palette::WHITE,
    );
}

fn button_y() -> i32 {
    bottom(SUB_CONFIRM) - 8 - BUTTON_H
}

fn no_x() -> i32 {
    SUB_CONFIRM.x + SUB_CONFIRM.w as i32 / 2 - 24 - BUTTON_W
}

fn yes_x() -> i32 {
    SUB_CONFIRM.x + SUB_CONFIRM.w as i32 / 2 + 24
}

/// 一覧の見える範囲の先頭。選択を端の行の 1 つ内側に保つ [13 §3.10]。
pub(crate) fn scroll(first: usize, sel: usize, n: usize) -> usize {
    if n <= LIST_ROWS {
        return 0;
    }
    let max_first = n - LIST_ROWS;
    let first = if sel < first + 1 {
        sel.saturating_sub(1)
    } else if sel + 2 > first + LIST_ROWS {
        sel + 2 - LIST_ROWS
    } else {
        first
    };
    first.min(max_first)
}

fn volume(b: &mut DrawBuilder<'_>, y: i32, level: u8, num: &NumText) {
    let x = CTRL_X;
    b.sprite(sprite::ICO_TRI_LEFT, x, y + 6, palette::WHITE);
    for i in 0..i32::from(VOLUME_MAX) {
        let c = if i < i32::from(level) {
            palette::GAUGE_FILL
        } else {
            palette::GAUGE_EMPTY
        };
        b.sprite(sprite::UI_VOL_SEG, x + 12 + i * 8, y + 5, c);
    }
    let blk = &num.block;
    b.text(blk, x + 106 - i32::from(blk.width), y + 2, palette::TEXT);
    b.sprite(sprite::ICO_TRI_RIGHT, x + 110, y + 6, palette::WHITE);
}

fn set_show(settings: &mut Settings, on: bool, cx: &mut Ctx<'_, '_>) {
    settings.show_explanations = on;
    if let Some(t) = cx.tutorial.as_deref_mut() {
        t.set_show_explanations(on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_follow_01_order_with_quit_above_reset() {
        assert_eq!(ITEMS[0], SettingsItem::Continue);
        assert_eq!(
            ITEMS[ITEMS.len() - 2..],
            [SettingsItem::Quit, SettingsItem::Reset]
        );
        assert_eq!(
            ITEMS,
            [
                SettingsItem::Continue,
                SettingsItem::MusicVolume,
                SettingsItem::SoundVolume,
                SettingsItem::ReplayExplanations,
                SettingsItem::ShowExplanations,
                SettingsItem::Quit,
                SettingsItem::Reset
            ]
        );
    }

    #[test]
    fn list_scrolls_one_row_inside() {
        assert_eq!(LIST_ROWS, 7, "13 §3.10: 一度に 7 行");
        assert_eq!(scroll(0, 4, 5), 0);
        assert_eq!(scroll(0, LIST_ROWS - 2, 14), 0);
        assert_eq!(scroll(0, LIST_ROWS - 1, 14), 1);
        assert_eq!(scroll(1, 13, 14), 14 - LIST_ROWS);
        assert_eq!(scroll(6, 6, 14), 5);
        assert_eq!(scroll(5, 0, 14), 0);
    }

    #[test]
    fn rows_fit_the_box_and_the_separator_comes_before_quit_and_reset() {
        assert_eq!(ITEMS[SEP_AT..], [SettingsItem::Quit, SettingsItem::Reset]);
        assert_eq!(row_y(SEP_AT) - row_y(SEP_AT - 1), SETTINGS_ROW_H + SEP_H);
        let seventh = ROWS_Y + SETTINGS_ROWS * SETTINGS_ROW_H + SEP_H;
        assert_eq!(
            seventh,
            bottom(SETTINGS) - PAD,
            "7 行と区切りでちょうど埋まる"
        );
        assert_eq!(ITEMS.len(), SETTINGS_ROWS as usize);
        assert_eq!(row_y(ITEMS.len() - 1) + SETTINGS_ROW_H, seventh);
        let list_end = ROWS_Y + SETTINGS_ROW_H * LIST_ROWS as i32;
        assert!(list_end <= bottom(SETTINGS) - PAD, "読み直しの 7 行");
        assert!(row_rect(0).x > SETTINGS.x && right(row_rect(0)) < right(SETTINGS));
    }
}
