//! ヘッドレスの `App` のテストの補助: 入力の台本、カーソル移動、せってい の操作 [16 §1.17]。

#![allow(dead_code)]

pub mod kid;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tm_app::save::{SaveFile, SaveHeader};
use tm_app::{
    App, AppConfig, AppExit, FrameOutcome, MemStore, ReplayPolicy, SavePolicy, SeedPolicy, Slot,
};
use tm_core::{Game, GridPos, LineId, ReplayLog, StationId};
use tm_platform_api::{
    AssetBundle, AudioCmd, DrawList, HeadlessClock, HeadlessConfig, HeadlessPlatform, Key,
    MonoTime, PhysicalInput, Platform, PlatformError, RecordPolicy,
};

pub const SEED_A: u64 = 1;
/// タイトルで決定を受けるまでの時間(500ms)より後のフレーム。
pub const TITLE_READY_FRAME: u64 = 40;

pub fn platform(clock: HeadlessClock) -> HeadlessPlatform {
    HeadlessPlatform::new(HeadlessConfig {
        clock,
        record: RecordPolicy::LastOnly,
        quit_after_frames: None,
        fail_present_at: None,
        check_audio: true,
    })
}

pub fn config(seed: SeedPolicy) -> AppConfig {
    AppConfig {
        seed,
        ..AppConfig::default()
    }
}

/// メモリのセーブを使う設定。
pub fn saving(seed: SeedPolicy, store: &MemStore) -> AppConfig {
    AppConfig {
        seed,
        save: SavePolicy::Memory(store.clone()),
        ..AppConfig::default()
    }
}

/// 書き込みスレッドが空くまで待つ(フレームは回さない)。
pub fn wait_saved(app: &App) {
    let t = Instant::now();
    while app.app_status().saving {
        assert!(t.elapsed() < Duration::from_secs(10), "保存が終わらない");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// `seq` の大きい方のスロットのヘッダと中身。
pub fn newest(store: &MemStore) -> Option<(Slot, SaveHeader, Vec<u8>)> {
    Slot::ALL
        .into_iter()
        .filter_map(|s| {
            let b = store.slot(s)?;
            let h = SaveFile::header(&b).ok()?;
            Some((s, h, b))
        })
        .max_by_key(|(_, h, _)| h.seq)
}

pub fn memory() -> Arc<Mutex<Vec<ReplayLog>>> {
    Arc::new(Mutex::new(Vec::new()))
}

pub fn with_memory(mut cfg: AppConfig, mem: &Arc<Mutex<Vec<ReplayLog>>>) -> AppConfig {
    cfg.replay = ReplayPolicy::Memory(mem.clone());
    cfg
}

/// `Rig` のキー入力とフレームの終わりを横から見る(`UiHarness` との突き合わせ用)。
pub trait Spy {
    fn key(&mut self, key: Key, down: bool);
    fn frame(&mut self, app: &App, p: &HeadlessPlatform);
}

/// `audio` と `present` の呼ばれた回数を数える。
pub struct Counting<'a> {
    pub inner: &'a mut HeadlessPlatform,
    pub audio_calls: &'a mut u64,
}

impl Platform for Counting<'_> {
    fn now(&self) -> MonoTime {
        self.inner.now()
    }
    fn poll_input(&mut self, out: &mut Vec<PhysicalInput>) {
        self.inner.poll_input(out)
    }
    fn load_assets(&mut self, assets: &AssetBundle) -> Result<(), PlatformError> {
        self.inner.load_assets(assets)
    }
    fn present(&mut self, frame: &DrawList) -> Result<(), PlatformError> {
        self.inner.present(frame)
    }
    fn audio(&mut self, cmds: &[AudioCmd]) {
        *self.audio_calls += 1;
        self.inner.audio(cmds)
    }
    fn logical_size(&self) -> (u32, u32) {
        self.inner.logical_size()
    }
}

/// `App` と `HeadlessPlatform` の組。キーの押し方とフレームの進め方を持つ。
pub struct Rig {
    pub app: App,
    pub p: HeadlessPlatform,
    pub cursor: Option<GridPos>,
    pub spy: Option<Box<dyn Spy>>,
    /// `frame` が `Quit` を返した(以後のフレームは回さない)。
    pub exit: Option<AppExit>,
    /// `Platform::audio` が呼ばれた回数。
    pub audio_calls: u64,
}

impl Rig {
    pub fn new(clock: HeadlessClock, cfg: AppConfig) -> Rig {
        let mut p = platform(clock);
        let app = App::new(&mut p, cfg).expect("App::new");
        Rig {
            app,
            p,
            cursor: None,
            spy: None,
            exit: None,
            audio_calls: 0,
        }
    }

    pub fn game(&self) -> &Game {
        self.app.game().expect("ゲーム中")
    }

    pub fn tick(&self) -> u64 {
        self.game().now().0
    }

    /// 1 フレーム。`Quit` を返したら `exit` に残す。終わった後には呼べない。
    pub fn frame(&mut self) -> FrameOutcome {
        assert!(self.exit.is_none(), "終わった後のフレーム");
        let had_game = self.app.game().is_some();
        let mut p = Counting {
            inner: &mut self.p,
            audio_calls: &mut self.audio_calls,
        };
        let out = self.app.frame(&mut p).expect("frame");
        if let FrameOutcome::Quit(e) = out {
            self.exit = Some(e);
        }
        match (had_game, self.app.game()) {
            (false, Some(g)) => self.cursor = Some(initial_cursor(g)),
            (_, None) => self.cursor = None,
            _ => {}
        }
        if let Some(spy) = self.spy.as_mut() {
            spy.frame(&self.app, &self.p);
        }
        out
    }

    /// `Quit` を返すまで回す(最大 `max` フレーム)。終わり方と回したフレーム数。
    pub fn run_until_quit(&mut self, max: u64) -> (AppExit, u64) {
        for n in 1..=max {
            if let FrameOutcome::Quit(e) = self.frame() {
                return (e, n);
            }
        }
        panic!("{max} フレームで終わらない");
    }

    pub fn frames(&mut self, n: u64) {
        for _ in 0..n {
            assert_eq!(self.frame(), FrameOutcome::Continue);
        }
    }

    pub fn frame_index(&self) -> u64 {
        self.p.frame_index()
    }

    pub fn down(&mut self, key: Key) {
        let f = self.p.frame_index();
        self.p.schedule(f, PhysicalInput::KeyDown(key));
        if let Some(spy) = self.spy.as_mut() {
            spy.key(key, true);
        }
    }

    pub fn up(&mut self, key: Key) {
        let f = self.p.frame_index();
        self.p.schedule(f, PhysicalInput::KeyUp(key));
        if let Some(spy) = self.spy.as_mut() {
            spy.key(key, false);
        }
    }

    /// 1 フレーム押して次のフレームで離す(2 フレーム進む)。
    pub fn tap(&mut self, key: Key) {
        self.tap_all(&[key]);
    }

    pub fn tap_all(&mut self, keys: &[Key]) {
        for &k in keys {
            self.down(k);
        }
        self.frame();
        for &k in keys {
            self.up(k);
        }
        self.frame();
    }

    pub fn wait_ms(&mut self, ms: u64) {
        self.frames((ms * 1000).div_ceil(16_666));
    }

    /// タイトルで決定してゲームを始める。
    pub fn start_game(&mut self) {
        let f = self.p.frame_index();
        if f < TITLE_READY_FRAME {
            self.frames(TITLE_READY_FRAME - f);
        }
        self.down(Key::Z);
        self.frame();
        assert!(self.app.game().is_some(), "決定でゲームが始まる");
        self.up(Key::Z);
        self.frame();
    }

    /// せってい を開いて「せつめいを 出す: しない」にして閉じる。
    pub fn disable_explanations(&mut self) {
        self.tap(Key::Escape);
        for _ in 0..4 {
            self.tap(Key::Down);
        }
        self.tap(Key::Right);
        self.tap(Key::Escape);
        assert!(!self.app.ui().show_explanations());
        assert!(!self.app.ui().is_paused(self.app.tutorial()));
    }

    /// タイトルで「続きから」(セーブがあれば最初に選ばれている)。決定のフレームでゲームができる。
    pub fn continue_game(&mut self) {
        let f = self.p.frame_index();
        if f < TITLE_READY_FRAME {
            self.frames(TITLE_READY_FRAME - f);
        }
        assert!(self.app.app_status().has_save, "続きからがある");
        self.down(Key::Z);
        self.frame();
        assert!(self.app.game().is_some(), "続きから");
        self.up(Key::Z);
    }

    /// せってい → ゲームを終わる(いちばん下の 1 つ上)→「はい」× 2。
    pub fn quit_game(&mut self) {
        self.tap(Key::Escape);
        self.quit_in_settings();
    }

    /// 開いている せってい から「ゲームを終わる」。
    pub fn quit_in_settings(&mut self) {
        for _ in 0..6 {
            self.tap(Key::Down);
        }
        self.tap(Key::Up);
        self.tap(Key::Z);
        for _ in 0..2 {
            self.tap(Key::Right);
            self.wait_ms(300);
            self.tap(Key::Z);
        }
        assert!(
            self.app.ui().is_paused(self.app.tutorial()),
            "終了中は止まる"
        );
    }

    /// せってい → さいしょから やりなおす →「はい」× 2。最後の Z は押したまま。
    pub fn reset_holding_confirm(&mut self) {
        self.tap(Key::Escape);
        for _ in 0..6 {
            self.tap(Key::Down);
        }
        self.tap(Key::Z);
        for _ in 0..2 {
            self.tap(Key::Right);
            self.wait_ms(300);
            self.down(Key::Z);
            self.frame();
            if self.app.game().is_some() {
                self.up(Key::Z);
                self.frame();
            }
        }
        assert!(self.app.game().is_none(), "タイトルに戻る");
    }

    /// 路線を作り(駅 0・1)、駅 2 へのばし、1 分後に 1 つ強化する。説明は「しない」にする。
    pub fn build_extend_upgrade(&mut self) {
        self.start_game();
        self.disable_explanations();
        self.build_first_line();
        let target = self.station_pos(StationId(2));
        let ends = self.game().view().line_ends(LineId(0));
        let (_, end) = ends
            .into_iter()
            .min_by_key(|&(_, s)| self.station_pos(s).chebyshev(target))
            .expect("はし");
        self.tap(Key::B);
        self.tap(Key::Down);
        self.tap(Key::Z);
        let p = self.station_pos(end);
        self.go(p);
        self.tap(Key::Z);
        self.go(target);
        self.tap(Key::Z);
        self.wait_ms(350);
        self.tap(Key::Z);
        self.frames(5);
        let len = self
            .game()
            .view()
            .line(LineId(0))
            .expect("路線")
            .stations
            .len();
        assert_eq!(len, 3, "のばせる");

        self.frames(3_600);
        self.tap(Key::E);
        self.wait_ms(350);
        self.tap(Key::Z);
        self.frames(5);
        self.tap(Key::X);
    }

    /// 駅 0 と駅 1 で最初の路線を作る。
    pub fn build_first_line(&mut self) {
        self.tap(Key::B);
        self.tap(Key::Z);
        for s in [0, 1] {
            let p = self.station_pos(StationId(s));
            self.go(p);
            self.tap(Key::Z);
        }
        self.tap(Key::Z);
        self.wait_ms(350);
        self.tap(Key::Z);
        self.frames(5);
        assert_eq!(self.game().view().lines().len(), 1, "路線ができる");
    }

    /// 矢印でマップのカーソルを `pos` へ(斜めは同時押し)。
    pub fn go(&mut self, pos: GridPos) {
        for _ in 0..64 {
            let c = self.cursor.expect("ゲーム中");
            if c == pos {
                return;
            }
            let mut keys = Vec::new();
            let mut next = c;
            if pos.x < c.x {
                keys.push(Key::Left);
                next.x -= 1;
            } else if pos.x > c.x {
                keys.push(Key::Right);
                next.x += 1;
            }
            if pos.y < c.y {
                keys.push(Key::Up);
                next.y -= 1;
            } else if pos.y > c.y {
                keys.push(Key::Down);
                next.y += 1;
            }
            self.tap_all(&keys);
            self.cursor = Some(next);
        }
        panic!("カーソルが届かない");
    }

    pub fn station_pos(&self, id: StationId) -> GridPos {
        self.game().view().station(id).expect("駅").pos
    }

    /// `HeadlessPlatform` に渡った音の命令(フレーム番号つき)の `from` 以降。
    pub fn audio_since(&self, from: usize) -> Vec<(u64, AudioCmd)> {
        self.p.audio_log()[from..].to_vec()
    }

    pub fn audio_len(&self) -> usize {
        self.p.audio_log().len()
    }

    pub fn ducks_since(&self, from: usize) -> Vec<bool> {
        self.p.audio_log()[from..]
            .iter()
            .filter_map(|(_, c)| match c {
                AudioCmd::SetBgmDuck(d) => Some(*d),
                _ => None,
            })
            .collect()
    }

    pub fn now(&self) -> MonoTime {
        self.p.now()
    }

    pub fn assert_audio_ok(&self) {
        assert!(
            self.p.audio_violations().is_empty(),
            "{:?}",
            self.p.audio_violations()
        );
    }
}

/// マップのカーソルの初期位置: マップの中央 [13 UI-D-27]。
pub fn cursor_start(game: &Game) -> GridPos {
    let (w, h) = game.view().map_size();
    GridPos {
        x: i32::from(w) / 2,
        y: i32::from(h) / 2,
    }
}

/// 中央。駅なら左へ、左の端まで駅なら右へずらす(13 UI-D-27 と同じ規則)。
pub fn initial_cursor(game: &Game) -> GridPos {
    let view = game.view();
    let start = cursor_start(game);
    let mut p = start;
    while view.station_at(p).is_some() && p.x > 0 {
        p.x -= 1;
    }
    if view.station_at(p).is_some() {
        p = start;
        while view.station_at(p).is_some() {
            p.x += 1;
        }
    }
    p
}
