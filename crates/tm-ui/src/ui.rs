//! `Ui`(02 §4.6 の API)[13 §3.2]。

use tm_assets::sound::{SE_CONFIRM, SE_POPUP};
use tm_core::{Command, GameEvent, GameView};
use tm_input::ActionFrame;
use tm_platform_api::{AudioCmd, DrawList, MonoTime};
use tm_tutorial::{Tutorial, UiEvent};

use crate::audio::AudioDirector;
use crate::draw::{DrawBuilder, DrawStats, UI_DRAW_BUDGET};
use crate::request::{AppRequest, AppStatus};
use crate::scene::{PlayScene, Scene, TitleExit, UiSound};
use crate::settings::Settings;
use crate::text::TextCache;
use crate::tutorial_bridge;

/// `update` が作り `render` が読む、フレームの共通の値。
#[derive(Clone, Copy, Debug)]
struct FrameData {
    now: MonoTime,
}

/// 画面の状態機械、表示データ、設定を持つ。
pub struct Ui {
    settings: Settings,
    scene: Scene,
    text: TextCache,
    frame: FrameData,
    budget: usize,
    // 新しいゲームの map_revision も 0 から始まるので、リセットしても戻さない
    layer_versions: u64,
    sound: AudioDirector,
    app: AppStatus,
    /// 「続きから」の後、ゲームが見えた最初の `update` で会社レベルの曲を流す [13 §5.1]。
    continue_bgm: bool,
    /// `observe_ui` に渡した出来事(テスト用。`UiHarness` が取り出す)。
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) ui_log: Vec<UiEvent>,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    /// 設定は 01 の初期値、シーンはタイトル。tm-app が起動時に 1 回だけ呼ぶ(リセットでは `go_to_title`)。
    pub fn new() -> Ui {
        Ui {
            settings: Settings::default(),
            scene: Scene::Title(Box::default()),
            text: TextCache::default(),
            frame: FrameData { now: MonoTime(0) },
            budget: UI_DRAW_BUDGET,
            layer_versions: 0,
            sound: AudioDirector::new(),
            app: AppStatus::default(),
            continue_bgm: false,
            #[cfg(any(test, feature = "test-support"))]
            ui_log: Vec::new(),
        }
    }

    /// tm-app が毎フレーム `update` の前に渡す(タイトルのメニュー、終わる確認の文、保存中の印、終了中の表示)。
    pub fn set_app_status(&mut self, s: AppStatus) {
        self.app = s;
    }

    /// 音量と「説明を出す」の設定(tm-app が `settings.ron` に保存する)。
    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// 設定を戻す。`new` の直後、最初の `update` の前だけ呼ぶ(最初の `SetVolume` がこの値になる)。音量は 0..=10 に収める。
    pub fn set_settings(&mut self, s: Settings) {
        self.settings = s.clamped();
    }

    /// せってい(下位の画面を含む)が開いているか。
    pub fn is_settings_open(&self) -> bool {
        matches!(&self.scene, Scene::Play(p) if p.settings_open())
    }

    /// 入力を処理し、コアへのコマンド、アプリへの要求、音の命令を出す。`game` はタイトル画面では `None`。
    #[allow(clippy::too_many_arguments, reason = "02 §4.6 の API")]
    pub fn update(
        &mut self,
        now: MonoTime,
        input: &ActionFrame,
        game: Option<GameView<'_>>,
        mut tutorial: Option<&mut Tutorial>,
        commands: &mut Vec<Command>,
        requests: &mut Vec<AppRequest>,
        audio: &mut Vec<AudioCmd>,
    ) {
        self.sound.boot(&self.settings, audio);
        // 13 §3.3: 終了中は BGM の命令も FanfareFinished も出さない(tm-app が StopBgm で止める)
        let quitting = matches!(&self.scene, Scene::Play(p) if p.quitting());
        // UI-D-24: 入力より先に渡す
        if !quitting
            && self.sound.poll(now, audio)
            && let Some(t) = tutorial.as_deref_mut()
        {
            #[cfg(any(test, feature = "test-support"))]
            self.ui_log.push(UiEvent::FanfareFinished);
            t.observe_ui(&UiEvent::FanfareFinished);
        }
        self.text.begin_frame();
        self.frame = FrameData { now };
        let exit = match &mut self.scene {
            Scene::Title(t) => {
                let exit = t.update(now, input, &mut self.text, self.app);
                for id in t.sounds.drain(..) {
                    self.sound.se(now, id, audio);
                }
                exit
            }
            Scene::Play(p) => {
                if let Some(view) = game
                    && std::mem::take(&mut self.continue_bgm)
                {
                    self.sound.continue_game(now, view.company().level, audio);
                }
                p.update(
                    now,
                    input,
                    game,
                    tutorial.as_deref_mut(),
                    &mut self.settings,
                    self.app,
                    commands,
                    requests,
                    &mut self.layer_versions,
                );
                // 13 §3.13: 同じフレームに UI の操作音があれば se_popup は鳴らさない
                let operated = !p.sounds.is_empty();
                for s in p.sounds.drain(..) {
                    match s {
                        UiSound::Se(id) => self.sound.se(now, id, audio),
                        UiSound::Error(e) => self.sound.error(now, &e, audio),
                        UiSound::Volume(bus, level) => {
                            self.sound.volume_changed(now, bus, level, audio)
                        }
                    }
                }
                // 13 §3.3: 終了中は UiEvent を出さない
                if let Some(view) = game
                    && !p.quitting()
                {
                    #[cfg(any(test, feature = "test-support"))]
                    let log = Some(&mut self.ui_log);
                    #[cfg(not(any(test, feature = "test-support")))]
                    let log = None;
                    tutorial_bridge::deliver(p, view, tutorial.as_deref_mut(), log);
                    if p.prepare_explain(now, view, tutorial.as_deref()) && !operated {
                        self.sound.se(now, SE_POPUP, audio);
                    }
                }
                TitleExit::Stay
            }
        };
        match exit {
            TitleExit::Stay => return,
            TitleExit::NewGame => {
                requests.push(AppRequest::StartNewGame);
                self.sound.start_game(now, audio);
            }
            TitleExit::Continue => {
                requests.push(AppRequest::ContinueGame);
                self.sound.se(now, SE_CONFIRM, audio);
                self.continue_bgm = true;
            }
        }
        self.scene = Scene::Play(Box::new(PlayScene::new()));
    }

    /// ゲームイベントを演出(ポップアップ、帯、トーストなど)と音に変える。
    pub fn observe(&mut self, now: MonoTime, ev: &GameEvent, audio: &mut Vec<AudioCmd>) {
        if let Scene::Play(p) = &mut self.scene {
            p.observe(now, ev);
        }
        self.sound.observe(now, ev, audio);
        if let Scene::Play(p) = &mut self.scene {
            p.anims.defer_spawns(now, self.sound.after_jingles(now));
        }
    }

    /// true の間、tm-app はコアを進めない。せってい を開いているとき、終了中、止める説明(`pauses_game`)を出しているとき true。
    pub fn is_paused(&self, tutorial: Option<&Tutorial>) -> bool {
        matches!(&self.scene, Scene::Play(p) if p.settings_open() || p.quitting())
            || matches!(self.scene, Scene::Play(_))
                && tutorial
                    .and_then(Tutorial::directive)
                    .is_some_and(|d| d.pauses_game)
    }

    /// 「せつめいを 出す」の設定。tm-app が新しい `Tutorial` を作るときに渡す。
    pub fn show_explanations(&self) -> bool {
        self.settings.show_explanations
    }

    /// シーンを作り直してタイトル画面にし、タイトルの曲を頭から流す。設定は保つ。
    pub fn go_to_title(&mut self, audio: &mut Vec<AudioCmd>) {
        self.sound.go_to_title(audio);
        self.continue_bgm = false;
        self.scene = Scene::Title(Box::default());
    }

    /// 描画コマンドを `out` の末尾に書く。状態は変えない。
    pub fn render(
        &self,
        game: Option<GameView<'_>>,
        tutorial: Option<&Tutorial>,
        out: &mut DrawList,
    ) {
        self.render_stats(game, tutorial, out);
    }

    /// 描いて集計を返す。1 回目で数え、予算を超える分を決めてから 2 回目で書く(13 §9)。
    pub(crate) fn render_stats(
        &self,
        game: Option<GameView<'_>>,
        tutorial: Option<&Tutorial>,
        out: &mut DrawList,
    ) -> DrawStats {
        let mut counter = DrawBuilder::counting();
        self.emit(&mut counter, game, tutorial);
        let drop = counter.plan(self.budget);
        let mut b = DrawBuilder::emitting(&mut out.cmds, drop);
        self.emit(&mut b, game, tutorial);
        b.stats(&counter)
    }

    fn emit(
        &self,
        b: &mut DrawBuilder<'_>,
        game: Option<GameView<'_>>,
        tutorial: Option<&Tutorial>,
    ) {
        let _ = tutorial;
        match &self.scene {
            Scene::Title(t) => t.draw(b, &self.text, self.frame.now),
            Scene::Play(p) => p.draw(b, game, self.frame.now, &self.settings, self.app),
        }
    }

    /// マップのカーソル(ゲーム中だけ)。
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn map_cursor(&self) -> Option<tm_core::GridPos> {
        match &self.scene {
            Scene::Play(p) => Some(p.cursor),
            Scene::Title(_) => None,
        }
    }

    /// タイトルを飛ばしてゲームの画面にする(テストの入口)。
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn enter_play(&mut self) {
        self.scene = Scene::Play(Box::new(PlayScene::new()));
    }

    /// タイトルで `now` に決定した直後と同じ状態にする(起動の音とゲームの曲は出したことにする。APP-T-21)。
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn enter_play_from_title(&mut self, now: MonoTime, show_explanations: bool) {
        self.settings.show_explanations = show_explanations;
        let mut sent = Vec::new();
        self.sound.boot(&self.settings, &mut sent);
        self.sound.start_game(now, &mut sent);
        self.enter_play();
    }

    /// 描画要素の上限を変える(テスト用)。
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn set_budget(&mut self, budget: usize) {
        self.budget = budget;
    }

    /// ゲーム中の画面(テスト用)。
    #[cfg(test)]
    pub(crate) fn play(&self) -> Option<&PlayScene> {
        match &self.scene {
            Scene::Play(p) => Some(p),
            Scene::Title(_) => None,
        }
    }

    /// ゲーム中の画面(テスト用)。
    #[cfg(test)]
    pub(crate) fn play_mut(&mut self) -> Option<&mut PlayScene> {
        match &mut self.scene {
            Scene::Play(p) => Some(p),
            Scene::Title(_) => None,
        }
    }

    /// タイトルの画面(テスト用)。
    #[cfg(test)]
    pub(crate) fn title(&self) -> Option<&crate::scene::TitleScene> {
        match &self.scene {
            Scene::Title(t) => Some(t),
            Scene::Play(_) => None,
        }
    }
}
