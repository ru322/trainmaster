//! HUD、下の帯(情報の段と案内・チップの段)[13 §4.1、§4.2]。数は 10 Hz で作り直す [UI-D-11]。

use tm_assets::font::FontId;
use tm_assets::sprite;
use tm_assets::text::{TextArg, TextId};
use tm_assets::txt;
use tm_core::{GameView, GridPos, LineId, StationId};
use tm_input::InputDevice;
use tm_numeric::{BigNum, FormatStyle};
use tm_platform_api::{Color, MonoTime, Rect};
use tm_tutorial::UiAnchor;

use crate::draw::map::MapCache;
use crate::draw::palette::{self, line_color};
use crate::draw::{DrawBuilder, rect};
use crate::layout::{
    BAND_LEFT, BAND_RIGHT, BAND_ROW2, BAND_TEXT_W, BAND_TEXT_X, COIN_AT, GAUGE, GOAL_RIGHT, HUD,
    HUD_PAD, HUD_ROW1, HUD_ROW2, INFO_GAP, INFO_ROW, INFO_TEXT_W, INFO_TEXT_X, LEVEL_RIGHT,
    MONEY_X, SAVE_MARK, right,
};
use crate::text::numbers::NumText;
use crate::text::{DynText, TextBlock};

/// HUD とパネルの数を作り直す間隔 [UI-D-11]。
pub(crate) const REFRESH_US: u64 = 100_000;
/// 購入で所持金が減ったときに文字を明るくする長さ [UI-D-10]。
const MONEY_FLASH_US: u64 = 150_000;
/// ゲージが満タンになったときに光る長さ。
const GAUGE_FLASH_US: u64 = 500_000;
/// 保存中の印を出し始めてから残す最短の長さ [UI-D-34]。
pub(crate) const SAVE_MARK_MIN_US: u64 = 1_000_000;
/// 下の帯の 2 段目の文の上端(16px の行を 24px の段の縦の中央に)。
const BAND_TEXT_Y: i32 = BAND_ROW2.y + (BAND_ROW2.h as i32 - 16) / 2;

/// 10 Hz で作り直す時計。`Purchased` と `CommandRejected` を受けたら次のフレームですぐ作り直す。
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Refresh {
    next: Option<MonoTime>,
    force: bool,
}

impl Refresh {
    /// 今のフレームで作り直すか。作り直すなら次の時刻を決める。
    pub(crate) fn due(&mut self, now: MonoTime) -> bool {
        let due = self.force || self.next.is_none_or(|t| now >= t);
        if due {
            self.next = Some(MonoTime(now.0 + REFRESH_US));
            self.force = false;
        }
        due
    }

    /// 次のフレームですぐ作り直す。
    pub(crate) fn force(&mut self) {
        self.force = true;
    }
}

/// HUD の表示データ。
#[derive(Default)]
pub(crate) struct Hud {
    pub refresh: Refresh,
    money: NumText,
    money_value: Option<BigNum>,
    money_flash_until: u64,
    income_num: NumText,
    income: DynText,
    level: DynText,
    goal: DynText,
    gauge_percent: u8,
    gauge_gold: bool,
    gauge_flash_until: u64,
    saving: bool,
    save_from: Option<MonoTime>,
    /// 作り直した回数(テスト用)。
    #[cfg(test)]
    pub refreshes: u32,
}

impl Hud {
    /// 10 Hz(または強制)で値を取り直す。作り直したら true。
    pub(crate) fn update(&mut self, now: MonoTime, view: GameView<'_>, dev: InputDevice) -> bool {
        if !self.refresh.due(now) {
            return false;
        }
        #[cfg(test)]
        {
            self.refreshes += 1;
        }
        let money = view.money();
        if self.money_value.is_some_and(|old| money < old) {
            self.money_flash_until = now.0 + MONEY_FLASH_US;
        }
        self.money_value = Some(money);
        self.money.set(money, &FormatStyle::DEFAULT, FontId::Body12);
        if self
            .income_num
            .set(view.income_per_sec(), &FormatStyle::DEFAULT, FontId::Body12)
            || self.income.id().is_none()
        {
            self.income.set(
                txt::HUD_INCOME,
                &[TextArg {
                    name: "money",
                    value: &self.income_num.text,
                }],
                FontId::Body12,
                u16::MAX,
                dev,
            );
        }
        let company = view.company();
        let mut n = [0u8; 3];
        self.level.set(
            txt::HUD_COMPANY_LEVEL,
            &[TextArg {
                name: "n",
                value: decimal(u64::from(company.level.0), &mut n),
            }],
            FontId::Body12,
            u16::MAX,
            dev,
        );
        let goal = if view.goal_reached() {
            txt::HUD_GOAL_DONE
        } else {
            txt::HUD_GOAL
        };
        self.goal.set(goal, &[], FontId::Body12, u16::MAX, dev);
        let pct = company.gauge.display_percent.min(100);
        if pct == 100 && self.gauge_percent < 100 {
            self.gauge_flash_until = now.0 + GAUGE_FLASH_US;
        }
        self.gauge_percent = pct;
        self.gauge_gold = company.level >= company.max_level;
        true
    }

    /// 毎フレーム、tm-app の「書き込み中か」を受ける。
    pub(crate) fn set_saving(&mut self, now: MonoTime, saving: bool) {
        if saving && !self.saving {
            self.save_from = Some(now);
        }
        self.saving = saving;
    }

    /// 保存中の印を出すか(書き込み中か、出し始めから 1 秒の間)。
    pub(crate) fn save_mark(&self, now: MonoTime) -> bool {
        self.saving
            || self
                .save_from
                .is_some_and(|t| now.saturating_since(t) < SAVE_MARK_MIN_US)
    }

    /// highlight を置く HUD の部分(お金、収入、会社レベル、ゲージ)[13 §3.13]。
    pub(crate) fn part_rect(&self, a: UiAnchor) -> Option<Rect> {
        let (row1, row2, h) = (HUD_ROW1.y, HUD_ROW2.y, HUD_ROW1.h);
        let level_x = LEVEL_RIGHT - i32::from(self.level.block.width);
        Some(match a {
            UiAnchor::MoneyDisplay => {
                let w = MONEY_X + i32::from(self.money.block.width) + 2 - (COIN_AT.0 - 2);
                rect(COIN_AT.0 - 2, row1, w as u32, h)
            }
            UiAnchor::IncomeDisplay => {
                rect(HUD_PAD - 2, row2, u32::from(self.income.block.width) + 4, h)
            }
            UiAnchor::CompanyLevel => {
                rect(level_x - 2, row1, u32::from(self.level.block.width) + 4, h)
            }
            // 右の端に矢印の余地がないので、左の「会社レベル」の文字ごと囲む
            UiAnchor::CompanyGauge => rect(
                level_x - 2,
                row1,
                (right(GAUGE) + 2 - (level_x - 2)) as u32,
                h,
            ),
            _ => return None,
        })
    }

    /// 表示中の所持金の文字列(テスト用)。
    #[cfg(test)]
    pub(crate) fn money_text(&self) -> &str {
        &self.money.text
    }

    /// 所持金の文字の色。購入で減った直後は明るくする。
    pub(crate) fn money_color(&self, now: MonoTime) -> Color {
        if now.0 < self.money_flash_until {
            palette::TEXT
        } else {
            palette::MONEY
        }
    }

    /// HUD を描く(層 5)。
    pub(crate) fn draw(&self, b: &mut DrawBuilder<'_>, now: MonoTime) {
        b.fill(HUD, palette::PANEL);
        let row1 = HUD_ROW1.y;
        b.sprite(sprite::ICO_COIN, COIN_AT.0, COIN_AT.1, palette::WHITE);
        b.text(&self.money.block, MONEY_X, row1, self.money_color(now));
        b.text(
            &self.level.block,
            LEVEL_RIGHT - i32::from(self.level.block.width),
            row1,
            palette::TEXT,
        );
        let filled = i32::from(self.gauge_percent / 10);
        let on = if now.0 < self.gauge_flash_until {
            palette::WHITE
        } else if self.gauge_gold {
            palette::GAUGE_GOLD
        } else {
            palette::GAUGE_FILL
        };
        b.fill(
            rect(GAUGE.x - 1, GAUGE.y - 1, GAUGE.w + 1, GAUGE.h + 2),
            palette::BLACK,
        );
        for i in 0..10 {
            let c = if i < filled { on } else { palette::GAUGE_EMPTY };
            b.fill(rect(GAUGE.x + i * 8, GAUGE.y, 7, GAUGE.h), c);
        }

        let row2 = HUD_ROW2.y;
        b.text(&self.income.block, HUD_PAD, row2, palette::INCOME);
        b.text(
            &self.goal.block,
            GOAL_RIGHT - i32::from(self.goal.block.width),
            row2,
            palette::TEXT,
        );
        if self.save_mark(now) {
            b.sprite(sprite::ICO_SAVE, SAVE_MARK.x, SAVE_MARK.y, palette::WHITE);
        }
    }
}

/// 整数を十進で `buf` に書いて返す(確保しない)。
pub(crate) fn decimal(v: u64, buf: &mut [u8]) -> &str {
    let mut n = v;
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 || i == 0 {
            break;
        }
    }
    std::str::from_utf8(&buf[i..]).unwrap_or("?")
}

/// 情報の段の 1 項目: 名前、Lv、値(文)、数を 1 行に並べる。
#[derive(Default)]
struct InfoItem {
    label: DynText,
    level: DynText,
    value: DynText,
    num: NumText,
    show: Shown,
}

#[derive(Clone, Copy, Default)]
struct Shown {
    level: bool,
    value: bool,
    num: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InfoKind {
    Station(StationId),
    Line(LineId),
    Slots,
}

/// 情報の段の項目の数の上限。
const INFO_ITEMS: usize = 5;
/// 名前と値の間(半角の空白 1 つ分)。
const INFO_SPACE: i32 = 6;
/// 路線番号の札の幅と間。
const BADGE_STEP: i32 = 10;

/// 情報の段: カーソルの下の駅・路線の情報を 1 行に並べる [13 §4.2]。
#[derive(Default)]
pub(crate) struct InfoRow {
    items: [InfoItem; INFO_ITEMS],
    used: usize,
    badges: Vec<LineId>,
}

struct ItemWriter<'a> {
    row: &'a mut InfoRow,
    dev: InputDevice,
}

impl ItemWriter<'_> {
    fn item_args(&mut self, id: TextId, args: &[TextArg<'_>]) -> &mut InfoItem {
        let i = self.row.used;
        self.row.used += 1;
        let it = &mut self.row.items[i];
        it.show = Shown::default();
        it.label.set(id, args, FontId::Body12, u16::MAX, self.dev);
        it
    }

    fn item(&mut self, id: TextId, n: Option<u64>) -> &mut InfoItem {
        let mut buf = [0u8; 20];
        match n {
            Some(v) => self.item_args(
                id,
                &[TextArg {
                    name: "n",
                    value: decimal(v, &mut buf),
                }],
            ),
            None => self.item_args(id, &[]),
        }
    }

    /// 何もないマス: 路線の数(使っている数 / 枠)。
    fn slots(&mut self, used: &str, slots: &str) {
        self.item_args(
            txt::HUD_INFO_SLOTS,
            &[
                TextArg {
                    name: "used",
                    value: used,
                },
                TextArg {
                    name: "slots",
                    value: slots,
                },
            ],
        );
    }

    /// 路線に入っている駅: 待っている人、運賃(Lv と値)、お客さんの数(Lv)、ホームの長さ。
    fn station(&mut self, waiting: u64, fare: (u8, BigNum), spawn: u8, platform: u64) {
        let dev = self.dev;
        self.item(txt::HUD_INFO_WAITING, None)
            .num(BigNum::from_u64(waiting), &crate::text::numbers::PEOPLE);
        self.item(txt::PARAM_S2_NAME, None)
            .level(fare.0, dev)
            .num(fare.1, &FormatStyle::DEFAULT);
        self.item(txt::PARAM_S1_NAME, None).level(spawn, dev);
        self.item(txt::PARAM_S4_NAME, None).cars(platform, dev);
    }

    /// 路線: 番号と札、電車の本数、車両の数、通過する駅の数。
    fn line(&mut self, id: LineId, trains: u64, cars: u64, pass: u64) {
        let dev = self.dev;
        self.item(txt::HUD_INFO_LINE, Some(u64::from(id.0) + 1));
        self.row.badges.push(id);
        self.item(txt::HUD_INFO_TRAINS, Some(trains));
        self.item(txt::PARAM_T5_NAME, None).cars(cars, dev);
        self.item(txt::HUD_INFO_PASS_COUNT, Some(pass));
    }
}

impl InfoItem {
    fn level(&mut self, lv: u8, dev: InputDevice) -> &mut Self {
        let mut buf = [0u8; 3];
        self.level.set(
            txt::PANEL_LEVEL,
            &[TextArg {
                name: "n",
                value: decimal(u64::from(lv), &mut buf),
            }],
            FontId::Body12,
            u16::MAX,
            dev,
        );
        self.show.level = true;
        self
    }

    fn cars(&mut self, n: u64, dev: InputDevice) -> &mut Self {
        let mut buf = [0u8; 20];
        self.value.set(
            txt::HUD_INFO_CARS,
            &[TextArg {
                name: "n",
                value: decimal(n, &mut buf),
            }],
            FontId::Body12,
            u16::MAX,
            dev,
        );
        self.show.value = true;
        self
    }

    fn num(&mut self, v: BigNum, style: &FormatStyle) -> &mut Self {
        self.num.set(v, style, FontId::Body12);
        self.show.num = true;
        self
    }

    /// 名前の後に並べる部分(Lv、値、数)と色。
    fn parts(&self) -> impl Iterator<Item = (&TextBlock, Color)> {
        [
            (self.show.level, &self.level.block, palette::TEXT),
            (self.show.value, &self.value.block, palette::TEXT),
            (self.show.num, &self.num.block, palette::MONEY),
        ]
        .into_iter()
        .filter(|p| p.0)
        .map(|(_, b, c)| (b, c))
    }
}

impl InfoRow {
    /// カーソルの下のものに合わせて作り直す。
    pub(crate) fn update(
        &mut self,
        view: GameView<'_>,
        map: &MapCache,
        cursor: GridPos,
        dev: InputDevice,
    ) {
        let kind = match view.station_at(cursor) {
            Some(id) => InfoKind::Station(id),
            None => map
                .first_line_at(cursor)
                .map_or(InfoKind::Slots, InfoKind::Line),
        };
        self.used = 0;
        self.badges.clear();
        let mut w = ItemWriter { row: self, dev };
        match kind {
            InfoKind::Station(id) => {
                let Some(st) = view.station(id) else { return };
                w.item(txt::HUD_INFO_STATION, None);
                w.row.badges.extend_from_slice(st.lines);
                if !st.connected {
                    w.item(txt::HUD_INFO_NOT_CONNECTED, None);
                    return;
                }
                w.station(
                    st.waiting.0,
                    (st.levels.fare.0, st.fare),
                    st.levels.spawn.0,
                    u64::from(st.platform.0),
                );
            }
            InfoKind::Line(id) => {
                let Some(line) = view.line(id) else { return };
                let pass = line.stops.iter().filter(|s| !**s).count() as u64;
                w.line(id, line.trains.len() as u64, u64::from(line.cars.0), pass);
            }
            InfoKind::Slots => {
                let (used, slots) = view.line_slots();
                let (mut a, mut b2) = ([0u8; 10], [0u8; 10]);
                w.slots(
                    decimal(u64::from(used), &mut a),
                    decimal(u64::from(slots), &mut b2),
                );
            }
        }
    }

    /// 項目ごとの幅(最初の項目は路線番号の札を含む)。
    fn item_widths(&self) -> impl Iterator<Item = i32> + '_ {
        self.items[..self.used].iter().enumerate().map(|(i, it)| {
            let mut w = i32::from(it.label.block.width);
            if i == 0 && !self.badges.is_empty() {
                w += 2 + BADGE_STEP * self.badges.len() as i32;
            }
            it.parts()
                .fold(w, |w, (b, _)| w + INFO_SPACE + i32::from(b.width))
        })
    }

    /// すべての項目を並べたときの幅(はみ出す項目も数える。テスト用)。
    #[cfg(test)]
    pub(crate) fn full_width(&self) -> i32 {
        let (n, w) = self
            .item_widths()
            .fold((0, 0), |(n, w), iw| (n + 1, w + iw));
        w + INFO_GAP * (n - 1).max(0)
    }

    /// 段に収まる項目の数。収まらない項目は後ろから省く。
    pub(crate) fn fitting_items(&self) -> usize {
        let mut x = 0;
        let mut n = 0;
        for (i, w) in self.item_widths().enumerate() {
            let end = x + if i > 0 { INFO_GAP } else { 0 } + w;
            if end > i32::from(INFO_TEXT_W) {
                break;
            }
            x = end;
            n += 1;
        }
        n
    }

    /// 情報の段を描く(層 5)。
    pub(crate) fn draw(&self, b: &mut DrawBuilder<'_>) {
        b.fill(INFO_ROW, palette::INFO_BG);
        let y = INFO_ROW.y;
        let mut x = INFO_TEXT_X;
        for (i, it) in self.items[..self.fitting_items()].iter().enumerate() {
            if i > 0 {
                x += INFO_GAP;
            }
            b.text(&it.label.block, x, y, palette::TEXT);
            x += i32::from(it.label.block.width);
            if i == 0 && !self.badges.is_empty() {
                x += 2;
                for &l in &self.badges {
                    b.sprite(
                        sprite::LINE_NO[l.index() % sprite::LINE_NO.len()],
                        x + BADGE_STEP / 2 - 1,
                        y + INFO_ROW.h as i32 / 2,
                        line_color(l),
                    );
                    x += BADGE_STEP;
                }
            }
            for (blk, c) in it.parts() {
                x += INFO_SPACE;
                b.text(blk, x, y, c);
                x += i32::from(blk.width);
            }
        }
    }
}

/// 案内を出す長さ [13 §3.13]。
const NOTICE_US: u64 = 2_000_000;

/// 下の帯の一時的な案内(エラーなど。2 秒)。
struct Notice {
    text: DynText,
    until: MonoTime,
    color: Color,
}

/// 下の帯の右のチップ。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BandChips {
    /// マップと強化パネル: `chip.build` と `chip.panel`。
    #[default]
    Map,
    /// そのモードやダイアログの操作のヒント(`build.hint` など)。
    Hint(TextId),
    /// 出さない(終了中)。
    None,
}

/// 下の帯: 左に案内、右にボタンのチップ [13 §4.2]。
#[derive(Default)]
pub(crate) struct Band {
    chip_build: DynText,
    chip_panel: DynText,
    star: bool,
    notice: Option<Notice>,
    mode: DynText,
    show_mode: bool,
    guide: DynText,
    show_guide: bool,
    chips: BandChips,
    build_hint: DynText,
}

impl Band {
    /// モードの説明(左。エラーの案内より下の優先度)と右のチップ。
    pub(crate) fn set_mode(&mut self, text: Option<TextId>, chips: BandChips, dev: InputDevice) {
        self.show_mode = text.is_some();
        if let Some(id) = text {
            self.mode.set(id, &[], FontId::Body12, BAND_TEXT_W, dev);
        }
        self.chips = chips;
        if let BandChips::Hint(id) = chips {
            self.build_hint
                .set(id, &[], FontId::Body12, BAND_RIGHT.w as u16, dev);
        }
    }

    /// チュートリアルの止めない案内(Guide)の文。エラーの案内より下、モードの説明より上 [13 §3.13]。
    pub(crate) fn set_guide(&mut self, text: Option<TextId>, dev: InputDevice) {
        self.show_guide = text.is_some();
        if let Some(id) = text {
            self.guide.set(id, &[], FontId::Body12, BAND_TEXT_W, dev);
        }
    }

    /// 右のチップ(テスト用)。
    #[cfg(test)]
    pub(crate) fn chips(&self) -> BandChips {
        self.chips
    }

    /// 左に出している Guide の文(テスト用)。
    #[cfg(test)]
    pub(crate) fn guide_text(&self) -> Option<TextId> {
        self.show_guide.then(|| self.guide.id()).flatten()
    }

    /// 出している案内(エラーなど)の文。
    pub(crate) fn notice_text(&self) -> Option<TextId> {
        self.notice.as_ref().and_then(|n| n.text.id())
    }

    /// 左のモードの説明(テスト用)。
    #[cfg(test)]
    pub(crate) fn mode_text(&self) -> Option<TextId> {
        self.show_mode.then(|| self.mode.id()).flatten()
    }

    /// チップを機器に合わせて作る。おすすめが買えるときは星を付ける。
    pub(crate) fn update(&mut self, now: MonoTime, dev: InputDevice, star: bool) {
        self.chip_build.set(
            txt::CHIP_BUILD,
            &[],
            FontId::Body12,
            BAND_RIGHT.w as u16,
            dev,
        );
        self.chip_panel.set(
            txt::CHIP_PANEL,
            &[],
            FontId::Body12,
            BAND_RIGHT.w as u16,
            dev,
        );
        self.star = star;
        if self.notice.as_ref().is_some_and(|n| now >= n.until) {
            self.notice = None;
        }
    }

    /// エラーの案内を 2 秒出す(左の文の優先順位はいちばん高い)。
    pub(crate) fn show_error(
        &mut self,
        now: MonoTime,
        id: TextId,
        args: &[TextArg<'_>],
        dev: InputDevice,
    ) {
        let mut text = self.notice.take().map(|n| n.text).unwrap_or_default();
        text.set(id, args, FontId::Body12, BAND_TEXT_W, dev);
        self.notice = Some(Notice {
            text,
            until: MonoTime(now.0 + NOTICE_US),
            color: palette::ERROR,
        });
    }

    /// 下の帯の 2 段目を描く(層 5)。左は案内・エラー・モードの説明、右はチップ。
    pub(crate) fn draw(&self, b: &mut DrawBuilder<'_>) {
        b.fill(BAND_ROW2, palette::PANEL);
        if let Some(n) = &self.notice {
            b.sprite(
                sprite::ICO_CROSS,
                BAND_LEFT.x,
                BAND_ROW2.y + 7,
                palette::WHITE,
            );
            b.text(&n.text.block, BAND_TEXT_X, BAND_TEXT_Y, n.color);
        } else if self.show_guide {
            b.sprite(
                sprite::ICO_GUIDE,
                BAND_LEFT.x + 3,
                BAND_ROW2.y + BAND_ROW2.h as i32 / 2,
                palette::HIGHLIGHT,
            );
            b.text(
                &self.guide.block,
                BAND_TEXT_X,
                BAND_TEXT_Y,
                palette::HIGHLIGHT,
            );
        } else if self.show_mode {
            b.text(&self.mode.block, BAND_TEXT_X, BAND_TEXT_Y, palette::TEXT);
        }
        self.draw_chips(b);
    }

    /// せってい の うすい黒の上に 2 段目を描き直す。左(下の画面の案内)は描かず、右に操作のヒントだけ。
    pub(crate) fn draw_over_menu(&self, b: &mut DrawBuilder<'_>) {
        b.fill(BAND_ROW2, palette::PANEL);
        self.draw_chips(b);
    }

    fn draw_chips(&self, b: &mut DrawBuilder<'_>) {
        match self.chips {
            BandChips::Map => {}
            BandChips::None => return,
            BandChips::Hint(_) => {
                let w = i32::from(self.build_hint.block.width);
                b.text(
                    &self.build_hint.block,
                    right(BAND_RIGHT) - w,
                    BAND_TEXT_Y,
                    palette::TEXT,
                );
                return;
            }
        }
        if self.star {
            b.sprite(
                sprite::ICO_STAR,
                right(BAND_RIGHT) - STAR_W,
                BAND_ROW2.y + (BAND_ROW2.h as i32 - STAR_W) / 2,
                palette::WHITE,
            );
        }
        let [build, panel] = self.chip_xs();
        b.text(&self.chip_panel.block, panel, BAND_TEXT_Y, palette::TEXT);
        b.text(&self.chip_build.block, build, BAND_TEXT_Y, palette::TEXT);
        debug_assert!(build >= BAND_RIGHT.x, "チップが帯の右に収まらない");
    }

    /// 「ろせん」と「レベルアップ」のチップの左端。
    fn chip_xs(&self) -> [i32; 2] {
        let edge = right(BAND_RIGHT) - if self.star { STAR_W + 2 } else { 0 };
        let panel = edge - i32::from(self.chip_panel.block.width);
        [
            panel - CHIP_GAP - i32::from(self.chip_build.block.width),
            panel,
        ]
    }

    /// ろせんモードの操作のヒントの矩形。右がその形のときだけ。
    pub(crate) fn hint_rect(&self) -> Option<Rect> {
        if !matches!(self.chips, BandChips::Hint(_)) {
            return None;
        }
        let w = i32::from(self.build_hint.block.width);
        Some(rect(
            right(BAND_RIGHT) - w - 2,
            BAND_TEXT_Y - 2,
            (w + 4) as u32,
            CHIP_FRAME_H,
        ))
    }

    /// チップの矩形(右のチップがマップの形のときだけ)。`build` なら「ろせん」、でなければ「レベルアップ」(星を含む)。
    pub(crate) fn chip_rect(&self, build: bool) -> Option<Rect> {
        if self.chips != BandChips::Map {
            return None;
        }
        let [bx, px] = self.chip_xs();
        let y = BAND_TEXT_Y - 2;
        Some(if build {
            rect(
                bx - 2,
                y,
                u32::from(self.chip_build.block.width) + 4,
                CHIP_FRAME_H,
            )
        } else {
            rect(px - 2, y, (right(BAND_RIGHT) - px + 4) as u32, CHIP_FRAME_H)
        })
    }
}

/// おすすめの星の大きさ。
const STAR_W: i32 = 12;
/// 「ろせん」と「レベルアップ」のチップの間。
const CHIP_GAP: i32 = 8;
/// チップとヒントを囲む枠の高さ(16px の行の上下に 2px)。
const CHIP_FRAME_H: u32 = 20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_is_10hz_and_can_be_forced() {
        let mut r = Refresh::default();
        let t = |ms: u64| MonoTime(ms * 1000);
        assert!(r.due(t(0)));
        assert!(!r.due(t(17)));
        assert!(!r.due(t(99)));
        assert!(r.due(t(100)));
        r.force();
        assert!(r.due(t(117)));
        assert!(!r.due(t(134)));
        assert!(r.due(t(217)));
    }

    #[test]
    fn error_notice_lasts_two_seconds_and_fits() {
        let mut band = Band::default();
        band.show_error(
            MonoTime(0),
            txt::ERR_NOT_CONNECTED,
            &[],
            InputDevice::Keyboard,
        );
        let n = band.notice.as_ref().unwrap();
        assert_eq!(n.text.block.lines, 1);
        assert!(n.text.block.width <= BAND_TEXT_W);
        assert_eq!(n.text.block.line_height, 16);
        band.update(MonoTime(1_999_999), InputDevice::Keyboard, false);
        assert!(band.notice.is_some());
        band.update(MonoTime(2_000_000), InputDevice::Keyboard, false);
        assert!(band.notice.is_none());
    }

    fn longest_money() -> BigNum {
        let mut v = BigNum::ONE;
        for _ in 0..68 {
            v = v.mul_small(10);
        }
        v.mul_small(12_345).div_small(10_000)
    }

    fn longest_rows() -> Vec<(&'static str, InfoRow)> {
        let dev = InputDevice::Gamepad;
        let mut station = InfoRow::default();
        let mut w = ItemWriter {
            row: &mut station,
            dev,
        };
        w.item(txt::HUD_INFO_STATION, None);
        w.row.badges.extend([LineId(0), LineId(1), LineId(2)]);
        w.station(12_340_000, (99, longest_money()), 99, 12);
        let mut line = InfoRow::default();
        ItemWriter {
            row: &mut line,
            dev,
        }
        .line(LineId(5), 99, 99, 99);
        let mut slots = InfoRow::default();
        ItemWriter {
            row: &mut slots,
            dev,
        }
        .slots("16", "16");
        let mut off = InfoRow::default();
        let mut w = ItemWriter { row: &mut off, dev };
        w.item(txt::HUD_INFO_STATION, None);
        w.item(txt::HUD_INFO_NOT_CONNECTED, None);
        vec![
            ("station", station),
            ("line", line),
            ("slots", slots),
            ("not_connected", off),
        ]
    }

    /// 13 §3.5.1・§4.5: 情報の段は最長の値で段の幅(632)に 1 行で収まる。収まらない項目は後ろから省き、段の外に描かない。
    #[test]
    fn info_row_fits_with_the_longest_values() {
        for (kind, row) in longest_rows() {
            let full = row.full_width();
            assert!(
                full <= i32::from(INFO_TEXT_W),
                "{kind}: {full}px(上限 {INFO_TEXT_W}px)"
            );
            assert_eq!(row.fitting_items(), row.used, "{kind}");
            let mut out = Vec::new();
            let mut b = DrawBuilder::emitting(&mut out, [0; 4]);
            row.draw(&mut b);
            for cmd in &out {
                if let tm_platform_api::DrawCmd::Sprite { dst, .. } = cmd {
                    assert!(
                        dst.x >= INFO_TEXT_X - 1
                            && right(*dst) <= INFO_TEXT_X + i32::from(INFO_TEXT_W) + 1
                            && dst.y >= INFO_ROW.y
                            && dst.y + dst.h as i32 <= INFO_ROW.y + INFO_ROW.h as i32,
                        "{kind}: 段の外に描かない {dst:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn info_items_are_spaced_by_the_gap() {
        let rows = longest_rows();
        let (_, line) = &rows[1];
        let widths: Vec<i32> = line.item_widths().collect();
        assert_eq!(widths.len(), 4);
        assert_eq!(
            line.full_width(),
            widths.iter().sum::<i32>() + INFO_GAP * 3,
            "項目の間は 12px"
        );
    }

    #[test]
    fn decimal_writes_without_allocation() {
        let mut b = [0u8; 20];
        assert_eq!(decimal(0, &mut b), "0");
        assert_eq!(decimal(16, &mut b), "16");
        assert_eq!(decimal(u64::MAX, &mut b), "18446744073709551615");
    }
}
