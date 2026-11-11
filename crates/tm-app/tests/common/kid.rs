//! #1〜#6 を通す「子ども」。`Rig`(App)をキーだけで操作する [13 §10.3 の UI の通し]。
//! 見る値は `directive()`、`shown_stages()`、駅と路線の盤面。操作の前に highlight と選べる駅の点が描画に出ていることも確かめる。

use tm_core::{GridPos, LineId, ParamKey, StationId};
use tm_platform_api::Key;
use tm_tutorial::{DirectiveKind, MenuAnchor, TutorialDirective, TutorialStageId, UiAnchor};

use tm_ui::testing::draw_text;

use super::Rig;

/// 説明を 1 ページ読む時間(13 §10.3)。
pub const READ_MS: u64 = 5_000;
/// ゲームの時間でこれより長くかかったら失敗(フレーム数。15 分)。
const LIMIT_FRAMES: u64 = 15 * 60 * 60;

pub struct Kid {
    pub r: Rig,
    pub paused_frames: u64,
    pub pages: u32,
    downs: u32,
    picks: u32,
    extend: u8,
    buy_step: u8,
}

impl Kid {
    pub fn new(r: Rig) -> Kid {
        Kid {
            r,
            paused_frames: 0,
            pages: 0,
            downs: 0,
            picks: 0,
            extend: 0,
            buy_step: 0,
        }
    }

    pub fn directive(&self) -> Option<TutorialDirective> {
        self.r.app.tutorial().and_then(|t| t.directive().copied())
    }

    fn pos(&self, id: StationId) -> GridPos {
        self.r.station_pos(id)
    }

    /// 1 フレーム進める。止まっているフレームを数える。
    pub fn step(&mut self) {
        self.paused_frames += u64::from(self.r.app.ui().is_paused(self.r.app.tutorial()));
        self.r.frame();
    }

    pub fn wait(&mut self, ms: u64) {
        for _ in 0..(ms * 1000).div_ceil(16_666) {
            self.step();
        }
    }

    pub fn tap(&mut self, key: Key) {
        self.r.down(key);
        self.step();
        self.r.up(key);
        self.step();
    }

    fn go(&mut self, pos: GridPos) {
        self.r.go(pos);
    }

    fn go_and_confirm(&mut self, pos: GridPos) {
        self.go(pos);
        self.tap(Key::Z);
    }

    fn pick_end(&mut self, target: StationId) {
        let to = self.pos(target);
        let ends = self.r.game().view().line_ends(LineId(0));
        let (_, near) = ends
            .into_iter()
            .min_by_key(|&(_, s)| self.pos(s).chebyshev(to))
            .expect("はし");
        let p = self.pos(near);
        self.go_and_confirm(p);
    }

    fn screen(&self) -> String {
        draw_text(self.r.app.draw_list())
    }

    /// highlight の枠・矢印・線(グリフでない、highlight の色の描画)が出ている。
    fn assert_mark_drawn(&self, d: &TutorialDirective) {
        let drawn = self.screen().lines().any(|l| {
            l.contains("#FFD400")
                && (l.starts_with("Fill") || (l.starts_with("Sprite") && !l.contains('\'')))
        });
        assert!(drawn, "highlight が描画に出ていない: {d:?}");
    }

    /// 選べる駅の点(`fx_dot`)が出ている。
    fn assert_picks_drawn(&self) {
        let drawn = self
            .screen()
            .lines()
            .any(|l| l.starts_with("Sprite fx_dot ") && l.contains("#FFD400"));
        assert!(drawn, "選べる駅の点が描画に出ていない");
    }

    fn follow(&mut self, d: TutorialDirective) {
        let stage = d.stage;
        let h = d.highlight;
        if h.is_some() {
            self.assert_mark_drawn(&d);
        }
        if !matches!(h, Some(UiAnchor::Menu(MenuAnchor::UpgradeItem(_)))) {
            self.buy_step = 0;
        }
        match h {
            Some(UiAnchor::Menu(MenuAnchor::BuildMode)) => {
                self.downs = 0;
                self.picks = 0;
                self.extend = 0;
                self.tap(Key::B);
            }
            Some(UiAnchor::Menu(MenuAnchor::BuildCreate)) => self.tap(Key::Z),
            Some(UiAnchor::Menu(MenuAnchor::BuildExtend)) => {
                if self.downs == 0 {
                    self.downs += 1;
                    self.tap(Key::Down);
                } else {
                    self.extend = 1;
                    self.tap(Key::Z);
                }
            }
            Some(UiAnchor::Menu(MenuAnchor::LineConfirm)) => {
                self.wait(300);
                self.tap(Key::Z);
            }
            Some(UiAnchor::Menu(MenuAnchor::UpgradePanel | MenuAnchor::UpgradeTab(_))) => {
                self.tap(Key::E)
            }
            Some(UiAnchor::Menu(MenuAnchor::UpgradeItem(ParamKey::S2Fare))) => {
                if self.buy_step == 0 {
                    self.buy_step = 1;
                    self.tap(Key::Down);
                } else {
                    self.wait(300);
                    self.tap(Key::Z);
                }
            }
            Some(UiAnchor::Station(id)) if stage == TutorialStageId(5) => {
                if self.extend == 1 {
                    self.extend = 2;
                    self.pick_end(id);
                } else {
                    let p = self.pos(id);
                    self.go_and_confirm(p);
                }
            }
            Some(UiAnchor::Station(id)) => {
                let p = self.pos(id);
                self.go(p);
                self.wait(100);
            }
            None if stage == TutorialStageId(3) => {
                self.assert_picks_drawn();
                let id = StationId(self.picks.min(1));
                self.picks += 1;
                let p = self.pos(id);
                self.go_and_confirm(p);
            }
            Some(UiAnchor::Train(_) | UiAnchor::MoneyDisplay) => self.wait(500),
            other => panic!("知らない案内: {other:?} {d:?}"),
        }
    }

    /// 左と上を押しっぱなしにして(リピートを使う)、カーソルを左上の角に置く。
    pub fn hold_to_corner(&mut self) {
        self.r.down(Key::Left);
        self.r.down(Key::Up);
        self.r.wait_ms(4_000);
        self.r.up(Key::Left);
        self.r.up(Key::Up);
        self.r.frame();
        self.r.cursor = Some(GridPos { x: 0, y: 0 });
    }

    /// #6 の説明と案内が終わるまで通す。
    pub fn run_stages_1_to_6(&mut self) {
        let start = self.r.frame_index();
        loop {
            assert!(
                self.r.frame_index() - start < LIMIT_FRAMES,
                "時間内に #6 が終わらない"
            );
            let t = self.r.app.tutorial().expect("チュートリアル");
            let done = t.shown_stages().contains(&TutorialStageId(6));
            match t.directive().copied() {
                None if done => break,
                Some(d) if d.kind == DirectiveKind::Explanation => {
                    if d.highlight.is_some() {
                        self.assert_mark_drawn(&d);
                    }
                    self.wait(READ_MS);
                    self.tap(Key::Z);
                    self.pages += 1;
                }
                Some(d) => self.follow(d),
                None => self.wait(500),
            }
        }
    }
}
