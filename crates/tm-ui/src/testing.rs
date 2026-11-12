//! ヘッドレスのテストの補助 `UiHarness` と、`DrawList` のテキスト化 [13 §10.2]。

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::{Arc, OnceLock};

use tm_assets::font::{self, FontId};
use tm_assets::{sprite, text};
use tm_core::{BalanceTable, Command, Game, GameConfig, GameEvent, GridPos};
use tm_input::{Action, Bindings, InputMapper};
use tm_platform_api::{
    AudioCmd, Color, DrawCmd, DrawList, Key, MonoTime, PhysicalInput, Rect, SpriteId,
};
use tm_tutorial::{Tutorial, UiEvent};

use crate::request::{AppRequest, AppStatus};
use crate::ui::Ui;

/// 1 フレームの長さ(µs)。tm-app のスナップ後の累積器と同じく 1 フレーム 1 step(16 §1.7)。
pub const FRAME_US: u64 = 16_667;
/// `with_game` と `new_title` から始めたゲームのシード。
pub const DEFAULT_SEED: u64 = 1;

/// `Ui` を tm-app と同じ順で回すテストの台。
pub struct UiHarness {
    /// UI。
    pub ui: Ui,
    /// ゲーム(タイトルでは `None`)。
    pub game: Option<Game>,
    /// チュートリアル(タイトルでは `None`)。
    pub tutorial: Option<Tutorial>,
    /// いまの時刻。
    pub now: MonoTime,
    /// UI が出したコマンド(コアに渡したもの。積み上げ)。
    pub commands: Vec<Command>,
    /// UI が出した要求(積み上げ)。
    pub requests: Vec<AppRequest>,
    /// 音の命令(積み上げ。`SetBgmDuck` を含む)。
    pub audio: Vec<AudioCmd>,
    /// `observe_ui` に渡した出来事(積み上げ)。
    pub ui_events: Vec<UiEvent>,
    /// `StartNewGame` で作るゲームのシード。
    pub new_game_seed: u64,
    /// `ContinueGame` で始めるゲーム(なければ `new_game_seed` で作る)。
    pub saved_game: Option<Game>,
    app: AppStatus,
    mapper: InputMapper,
    pending: Vec<PhysicalInput>,
    frame_cmds: Vec<Command>,
    frame_reqs: Vec<AppRequest>,
    events: Vec<GameEvent>,
    draw: DrawList,
    ducked: bool,
    gamepad: bool,
}

fn builtin() -> Arc<BalanceTable> {
    static TABLE: OnceLock<Arc<BalanceTable>> = OnceLock::new();
    Arc::clone(TABLE.get_or_init(|| Arc::new(BalanceTable::builtin())))
}

fn key_for(a: Action) -> Key {
    Bindings::default_v1()
        .keys
        .iter()
        .find(|(_, b)| *b == a)
        .map(|(k, _)| *k)
        .expect("すべての Action にキーがある")
}

fn button_for(a: Action) -> tm_platform_api::PadButton {
    Bindings::default_v1()
        .pad_buttons
        .iter()
        .find(|(_, b)| *b == a)
        .map(|(k, _)| *k)
        .expect("すべての Action にパッドのボタンがある")
}

const PAD: tm_platform_api::PadId = tm_platform_api::PadId(0);

impl UiHarness {
    fn blank(now: MonoTime) -> UiHarness {
        UiHarness {
            ui: Ui::new(),
            game: None,
            tutorial: None,
            now,
            commands: Vec::new(),
            requests: Vec::new(),
            audio: Vec::new(),
            ui_events: Vec::new(),
            new_game_seed: DEFAULT_SEED,
            saved_game: None,
            app: AppStatus::default(),
            mapper: InputMapper::new(Bindings::default_v1()),
            pending: Vec::new(),
            frame_cmds: Vec::new(),
            frame_reqs: Vec::new(),
            events: Vec::new(),
            draw: DrawList::default(),
            ducked: false,
            gamepad: false,
        }
    }

    /// タイトル画面から始める(時刻 0)。
    pub fn new_title() -> UiHarness {
        UiHarness::blank(MonoTime(0))
    }

    fn start_play(&mut self, game: Game, show_explanations: bool) {
        self.game = Some(game);
        self.tutorial = Some(Tutorial::new(show_explanations));
        self.ui.enter_play();
    }

    /// 駅の位置を指定したゲームの画面から始める(`plan.len()` は駅の最大数)。
    pub fn with_game(plan: Vec<GridPos>, show_explanations: bool) -> UiHarness {
        let mut h = UiHarness::blank(MonoTime(0));
        let game = Game::with_station_plan(
            GameConfig {
                seed: DEFAULT_SEED,
                balance: builtin(),
            },
            plan,
        );
        h.start_play(game, show_explanations);
        h
    }

    /// 作ったゲーム(表を変えたものなど)の画面から始める。
    pub fn from_game(game: Game, show_explanations: bool) -> UiHarness {
        let mut h = UiHarness::blank(MonoTime(0));
        h.start_play(game, show_explanations);
        h
    }

    /// シードから作ったゲームの画面から始める(APP-T-21 で App と比べる入口)。
    /// `start` は App の `StartNewGame` のフレームの時刻で、UI はそのフレームの後の状態になる。最初のフレームは `start + FRAME_US`。
    pub fn with_seed(seed: u64, show_explanations: bool, start: MonoTime) -> UiHarness {
        let mut h = UiHarness::blank(start);
        h.game = Some(Game::new(GameConfig {
            seed,
            balance: builtin(),
        }));
        h.tutorial = Some(Tutorial::new(show_explanations));
        h.ui.enter_play_from_title(start, show_explanations);
        h.ducked = h.ui.is_paused(h.tutorial.as_ref());
        h
    }

    /// 以後の押し方をゲームパッドのボタンにする(次のフレームでパッドを 1 台つなぐ)。
    pub fn use_gamepad(&mut self) {
        self.pending.push(PhysicalInput::PadConnected(PAD));
        self.gamepad = true;
    }

    fn down(&self, a: Action) -> PhysicalInput {
        if self.gamepad {
            PhysicalInput::PadButtonDown {
                pad: PAD,
                button: button_for(a),
            }
        } else {
            PhysicalInput::KeyDown(key_for(a))
        }
    }

    fn up(&self, a: Action) -> PhysicalInput {
        if self.gamepad {
            PhysicalInput::PadButtonUp {
                pad: PAD,
                button: button_for(a),
            }
        } else {
            PhysicalInput::KeyUp(key_for(a))
        }
    }

    /// 次のフレームでキーを押す(フレームは進めない)。
    pub fn key_down(&mut self, a: Action) {
        self.pending.push(self.down(a));
    }

    /// 次のフレームでキーを離す(フレームは進めない)。
    pub fn key_up(&mut self, a: Action) {
        self.pending.push(self.up(a));
    }

    /// 1 フレーム押して、次のフレームで離す(2 フレーム進む)。
    pub fn press(&mut self, a: Action) {
        self.press_all(&[a]);
    }

    fn press_all(&mut self, actions: &[Action]) {
        for &a in actions {
            self.pending.push(self.down(a));
        }
        self.frames(1);
        for &a in actions {
            self.pending.push(self.up(a));
        }
        self.frames(1);
    }

    /// `ms` の間押して離す。
    pub fn hold(&mut self, a: Action, ms: u64) {
        self.pending.push(self.down(a));
        self.frames((ms * 1000).div_ceil(FRAME_US).max(1) as u32);
        self.pending.push(self.up(a));
        self.frames(1);
    }

    /// 矢印の押し方を計算して、マップのカーソルを `pos` へ動かす(斜めは同時押し)。
    pub fn move_cursor_to(&mut self, pos: GridPos) {
        for _ in 0..64 {
            let Some(c) = self.ui.map_cursor() else {
                return;
            };
            if c == pos {
                return;
            }
            let mut keys = Vec::new();
            match pos.x.cmp(&c.x) {
                std::cmp::Ordering::Less => keys.push(Action::MoveLeft),
                std::cmp::Ordering::Greater => keys.push(Action::MoveRight),
                std::cmp::Ordering::Equal => {}
            }
            match pos.y.cmp(&c.y) {
                std::cmp::Ordering::Less => keys.push(Action::MoveUp),
                std::cmp::Ordering::Greater => keys.push(Action::MoveDown),
                std::cmp::Ordering::Equal => {}
            }
            self.press_all(&keys);
        }
    }

    /// `n` フレーム進める。1 フレームは時計を 16,667µs 進め、tm-app の `App::frame` と同じ順で処理する。
    pub fn frames(&mut self, n: u32) {
        for _ in 0..n {
            self.frame();
        }
    }

    fn frame(&mut self) {
        self.now = MonoTime(self.now.0 + FRAME_US);
        let now = self.now;
        for ev in self.pending.drain(..) {
            self.mapper.feed(&ev);
        }
        let actions = self.mapper.end_frame(now);
        self.ui.set_app_status(self.app);
        match (self.game.as_ref(), self.tutorial.as_mut()) {
            (Some(g), Some(t)) => {
                t.observe_view(g.view());
                self.ui.update(
                    now,
                    &actions,
                    Some(g.view()),
                    Some(t),
                    &mut self.frame_cmds,
                    &mut self.frame_reqs,
                    &mut self.audio,
                );
            }
            _ => self.ui.update(
                now,
                &actions,
                None,
                None,
                &mut self.frame_cmds,
                &mut self.frame_reqs,
                &mut self.audio,
            ),
        }
        self.ui_events.append(&mut self.ui.ui_log);
        for r in std::mem::take(&mut self.frame_reqs) {
            self.requests.push(r);
            self.handle_request(r);
        }
        let paused = self.ui.is_paused(self.tutorial.as_ref());
        if paused != self.ducked {
            self.audio.push(AudioCmd::SetBgmDuck(paused));
            self.ducked = paused;
        }
        match (self.game.as_mut(), self.tutorial.as_mut()) {
            (Some(g), Some(t)) if !paused => {
                for c in self.frame_cmds.drain(..) {
                    self.commands.push(c.clone());
                    g.enqueue(c);
                }
                g.step(&mut self.events);
                for e in self.events.drain(..) {
                    t.observe_game(&e);
                    self.ui.observe(now, &e, &mut self.audio);
                }
            }
            (Some(_), _) => {}
            (None, _) => self.frame_cmds.clear(),
        }
        self.draw.cmds.clear();
        let view = self.game.as_ref().map(Game::view);
        self.ui.render(view, self.tutorial.as_ref(), &mut self.draw);
    }

    fn handle_request(&mut self, r: AppRequest) {
        match r {
            AppRequest::StartNewGame => {
                if self.game.is_none() {
                    self.game = Some(Game::new(GameConfig {
                        seed: self.new_game_seed,
                        balance: builtin(),
                    }));
                    self.tutorial = Some(Tutorial::new(self.ui.show_explanations()));
                }
            }
            AppRequest::ContinueGame => {
                if self.game.is_none() {
                    let seed = self.new_game_seed;
                    self.game = Some(self.saved_game.take().unwrap_or_else(|| {
                        Game::new(GameConfig {
                            seed,
                            balance: builtin(),
                        })
                    }));
                    self.tutorial = Some(Tutorial::new(self.ui.show_explanations()));
                }
            }
            AppRequest::QuitGame => {}
            AppRequest::ResetToTitle => {
                if self.game.take().is_some() {
                    self.tutorial = None;
                    self.frame_cmds.clear();
                    self.events.clear();
                    self.mapper.reset();
                    self.ui.go_to_title(&mut self.audio);
                }
            }
        }
    }

    /// せってい の 2 回目の「はい」と同じ後片付けでタイトルに戻す(tm-app の `ResetToTitle` の処理)。
    pub fn reset_to_title(&mut self) {
        self.requests.push(AppRequest::ResetToTitle);
        self.handle_request(AppRequest::ResetToTitle);
    }

    /// 最後のフレームの描画コマンド。
    pub fn draw_list(&self) -> &DrawList {
        &self.draw
    }

    /// 最後のフレームの `DrawList` のテキスト表現(1 コマンド 1 行)。
    pub fn draw_text(&self) -> String {
        draw_text(&self.draw)
    }

    /// 描画要素の上限を変える(予算のテスト用)。
    pub fn set_draw_budget(&mut self, budget: usize) {
        self.ui.set_budget(budget);
    }

    /// 以後の毎フレーム `Ui` に渡す値(セーブの有無・保存中・終わり方)。
    pub fn set_app_status(&mut self, s: AppStatus) {
        self.app = s;
    }
}

fn glyph_names() -> &'static BTreeMap<u16, (u8, char)> {
    static NAMES: OnceLock<BTreeMap<u16, (u8, char)>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut chars: Vec<char> = (' '..='~').collect();
        for i in 0..text::TEXT_COUNT {
            chars.extend(text::text(text::TextId(i as u16)).chars());
        }
        chars.extend(tm_numeric::OUTPUT_CHARS.chars());
        for (k, y) in tm_numeric::JA_UNITS {
            chars.extend(k.chars().chain(y.chars()));
        }
        let mut map = BTreeMap::new();
        for (size, f) in [(12, FontId::Body12), (16, FontId::Head16)] {
            for &c in &chars {
                if let Some(b) = font::glyph(f, c).and_then(|g| g.bitmap) {
                    map.insert(b.sprite.0, (size, c));
                }
            }
        }
        map
    })
}

fn sprite_name(id: SpriteId) -> String {
    if usize::from(id.0) < sprite::SPRITE_COUNT {
        return sprite::name(id).to_string();
    }
    match glyph_names().get(&id.0) {
        Some((size, c)) => format!("'{c}'{size}"),
        None => format!("glyph#{}", id.0),
    }
}

fn color(c: Color) -> String {
    format!("#{:02X}{:02X}{:02X}{:02X}", c.r, c.g, c.b, c.a)
}

fn rect(r: Rect) -> String {
    format!("{},{} {}x{}", r.x, r.y, r.w, r.h)
}

/// `DrawList` のテキスト表現(1 コマンド 1 行)。スプライトは名前、グリフは字と大きさで書く。
pub fn draw_text(list: &DrawList) -> String {
    let mut s = String::new();
    for cmd in &list.cmds {
        let _ = match *cmd {
            DrawCmd::Clear(c) => writeln!(s, "Clear {}", color(c)),
            DrawCmd::FillRect { rect: r, color: c } => writeln!(s, "Fill {} {}", rect(r), color(c)),
            DrawCmd::Line {
                x0,
                y0,
                x1,
                y1,
                color: c,
            } => {
                writeln!(s, "Line {x0},{y0}-{x1},{y1} {}", color(c))
            }
            DrawCmd::Sprite { id, dst, tint } => {
                writeln!(
                    s,
                    "Sprite {} {} {}",
                    sprite_name(id),
                    rect(dst),
                    color(tint)
                )
            }
            DrawCmd::BeginCachedLayer {
                layer,
                revision,
                x,
                y,
                w,
                h,
            } => writeln!(
                s,
                "BeginLayer {} rev={revision} {}",
                layer.0,
                rect(Rect { x, y, w, h })
            ),
            DrawCmd::Quad { pts, color: c } => {
                let p: Vec<String> = pts.iter().map(|(x, y)| format!("{x},{y}")).collect();
                writeln!(s, "Quad {} {}", p.join(" "), color(c))
            }
            DrawCmd::EndCachedLayer => writeln!(s, "EndLayer"),
            DrawCmd::DrawCachedLayer { layer } => writeln!(s, "DrawLayer {}", layer.0),
        };
    }
    s
}
