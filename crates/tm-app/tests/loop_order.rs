//! APP-T-01〜08: 起動、新しいゲーム、累積器、処理落ち、せってい と止める説明による一時停止、「しない」、止まっている間のコマンド(16 §2.4)。
//! ほかに、1 フレームに複数 step するときに説明が始まったら残りを捨てること。

mod common;

use common::{Rig, SEED_A, TITLE_READY_FRAME, config};
use tm_app::{App, SeedPolicy};
use tm_assets::sound::BGM_TITLE;
use tm_core::{GameEvent, StationId, Tick};
use tm_platform_api::{
    AssetBundle, AudioBus, AudioCmd, DrawList, HeadlessClock, HeadlessPlatform, Key, MonoTime,
    PadButton, PadId, PhysicalInput, Platform, PlatformError,
};
use tm_tutorial::{DirectiveKind, TutorialStageId};

/// `load_assets` の回数を数える。
struct Counting {
    inner: HeadlessPlatform,
    loads: u32,
}

impl Platform for Counting {
    fn now(&self) -> MonoTime {
        self.inner.now()
    }
    fn poll_input(&mut self, out: &mut Vec<PhysicalInput>) {
        self.inner.poll_input(out)
    }
    fn load_assets(&mut self, assets: &AssetBundle) -> Result<(), PlatformError> {
        self.loads += 1;
        self.inner.load_assets(assets)
    }
    fn present(&mut self, frame: &DrawList) -> Result<(), PlatformError> {
        self.inner.present(frame)
    }
    fn audio(&mut self, cmds: &[AudioCmd]) {
        self.inner.audio(cmds)
    }
    fn logical_size(&self) -> (u32, u32) {
        self.inner.logical_size()
    }
}

#[test]
fn app_t_01_boot() {
    let mut p = Counting {
        inner: common::platform(HeadlessClock::Exact60),
        loads: 0,
    };
    let mut app = App::new(&mut p, config(SeedPolicy::Fixed(SEED_A))).unwrap();
    assert_eq!(p.loads, 1);
    assert!(app.game().is_none() && app.tutorial().is_none());
    for _ in 0..60 {
        app.frame(&mut p).unwrap();
    }
    assert_eq!(p.loads, 1, "load_assets は起動の 1 回だけ");
    let log = p.inner.audio_log();
    assert_eq!(
        log,
        [
            (
                0,
                AudioCmd::SetVolume {
                    bus: AudioBus::Bgm,
                    level: 6
                }
            ),
            (
                0,
                AudioCmd::SetVolume {
                    bus: AudioBus::Se,
                    level: 8
                }
            ),
            (0, AudioCmd::StopBgm { fade_ms: 0 }),
            (
                0,
                AudioCmd::PlayBgm {
                    id: BGM_TITLE,
                    crossfade_ms: 0
                }
            ),
        ],
        "最初のフレームの 4 命令だけ。go_to_title の分が重ならない"
    );
    assert!(app.game().is_none(), "タイトル画面のまま");
    assert_eq!(p.inner.frame_index(), 60);
    assert!(p.inner.audio_violations().is_empty());
}

#[test]
fn app_t_02_title_to_new_game() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(7)));
    r.frames(25);
    r.tap(Key::Z);
    assert!(r.app.game().is_none(), "出てから 500ms は決定を受けない");
    r.frames(TITLE_READY_FRAME - r.frame_index());
    r.down(Key::Z);
    r.frame();
    let g = r.app.game().expect("決定で Session ができる");
    assert_eq!(g.config().seed, 7);
    assert_eq!(r.app.seed(), Some(7));
    assert_eq!(g.now(), Tick(0));
    assert!(r.app.ui().show_explanations());
    let t = r.app.tutorial().expect("チュートリアル");
    assert!(
        t.directive().is_some(),
        "「する」で作った Tutorial は説明を出す"
    );
    assert_eq!(r.app.stats().sessions_started, 1);
    r.assert_audio_ok();
}

#[test]
fn app_t_03_one_tick_per_frame() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.start_game();
    r.disable_explanations();
    let t0 = r.tick();
    for i in 1..=600 {
        r.frame();
        assert_eq!(r.tick(), t0 + i, "スナップで毎フレーム 1 tick");
    }
    r.assert_audio_ok();
}

#[test]
fn app_t_04_slow_frames() {
    let mut r = Rig::new(
        HeadlessClock::FixedUs(33_333),
        config(SeedPolicy::Fixed(SEED_A)),
    );
    r.start_game();
    r.disable_explanations();
    for _ in 0..30 {
        let t = r.tick();
        r.frame();
        assert_eq!(r.tick(), t + 2);
    }

    let mut r = Rig::new(
        HeadlessClock::FixedUs(83_333),
        config(SeedPolicy::Fixed(SEED_A)),
    );
    r.start_game();
    r.disable_explanations();
    let dropped = r.app.stats().dropped_ticks;
    for _ in 0..30 {
        let t = r.tick();
        r.frame();
        assert_eq!(r.tick(), t + 4, "上限の 4 tick");
    }
    assert!(r.app.stats().dropped_ticks >= dropped + 10);

    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.start_game();
    r.disable_explanations();
    r.p.advance_clock_us(5_000_000);
    let t = r.tick();
    r.frame();
    assert_eq!(r.tick(), t + 4, "長い停止の後も 4 tick まで");
    assert_eq!(r.app.stats().dropped_ticks, 2);
    for _ in 0..10 {
        let t = r.tick();
        r.frame();
        assert_eq!(r.tick(), t + 1, "次のフレームから 1 tick");
    }
    r.assert_audio_ok();
}

#[test]
fn app_t_05_settings_pauses() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.start_game();
    r.disable_explanations();
    r.frames(10);
    let mark = r.audio_len();
    r.down(Key::Escape);
    r.frame();
    assert!(r.app.ui().is_paused(r.app.tutorial()));
    let t = r.tick();
    r.up(Key::Escape);
    r.frames(120);
    assert_eq!(r.tick(), t, "せってい の間は進まない");
    assert_eq!(r.ducks_since(mark), [true], "SetBgmDuck(true) が 1 回だけ");
    assert!(r.app.game().unwrap().pending().is_empty());

    let mark = r.audio_len();
    r.down(Key::Escape);
    r.frame();
    assert!(!r.app.ui().is_paused(r.app.tutorial()));
    assert_eq!(r.tick(), t + 1, "止まっていた分が一気に進まない");
    r.up(Key::Escape);
    r.frame();
    assert_eq!(r.tick(), t + 2);
    r.frames(60);
    assert_eq!(
        r.ducks_since(mark),
        [false],
        "SetBgmDuck(false) が 1 回だけ"
    );
    r.assert_audio_ok();
}

#[test]
fn app_t_07_no_explanations_after_reset() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.start_game();
    r.disable_explanations();
    r.reset_holding_confirm();
    r.up(Key::Z);
    r.frames(40);
    let mark = r.audio_len();
    r.down(Key::Z);
    r.frame();
    let t = r.app.tutorial().expect("新しいゲーム");
    assert!(t.directive().is_none(), "説明が出ない");
    assert!(!r.app.ui().is_paused(r.app.tutorial()));
    assert_eq!(r.tick(), 1, "最初のフレームから進む");
    r.up(Key::Z);
    for i in 2..=120 {
        r.frame();
        assert_eq!(r.tick(), i);
    }
    assert!(
        !r.ducks_since(mark).contains(&true),
        "ダッキングされない: {:?}",
        r.ducks_since(mark)
    );
    r.assert_audio_ok();
}

fn directive(r: &Rig) -> Option<tm_tutorial::TutorialDirective> {
    r.app.tutorial().and_then(|t| t.directive().copied())
}

fn pausing(r: &Rig) -> bool {
    r.app
        .tutorial()
        .and_then(|t| t.directive())
        .is_some_and(|d| d.pauses_game)
}

/// 止める説明のページを決定で読み進める。
fn read_pages(r: &mut Rig) {
    for _ in 0..20 {
        if !pausing(r) {
            return;
        }
        r.wait_ms(400);
        r.tap(Key::Z);
    }
    panic!("説明が終わらない");
}

#[test]
fn steps_after_a_pausing_explanation_starts_are_dropped() {
    let mut r = Rig::new(
        HeadlessClock::FixedUs(66_666),
        config(SeedPolicy::Fixed(SEED_A)),
    );
    r.start_game();
    read_pages(&mut r);
    let p0 = r.station_pos(StationId(0));
    r.go(p0);
    r.wait_ms(200);
    read_pages(&mut r);
    r.tap(Key::B);
    r.tap(Key::Z);
    for s in [0, 1] {
        let p = r.station_pos(StationId(s));
        r.go(p);
        r.tap(Key::Z);
    }
    r.tap(Key::Z);
    r.wait_ms(350);
    r.tap(Key::Z);
    r.frames(2);
    assert_eq!(r.game().view().lines().len(), 1, "路線ができる");
    assert!(!pausing(&r));
    for _ in 0..3_000 {
        let before = r.game().clone();
        r.frame();
        if !pausing(&r) {
            continue;
        }
        let k = r.tick() - before.now().0;
        let mut g = before;
        let mut events = Vec::new();
        let mut first_income = None;
        for i in 1..=4u64 {
            g.step(&mut events);
            let income = events
                .drain(..)
                .any(|e| matches!(e, GameEvent::TrainArrived { income, .. } if !income.is_zero()));
            if income && first_income.is_none() {
                first_income = Some(i);
            }
        }
        assert_eq!(
            first_income,
            Some(k),
            "最初の収入(#4 の説明の始まり)の step で止め、残りは進めない"
        );
        let t = r.tick();
        r.frames(30);
        assert_eq!(r.tick(), t, "説明の間は止まる");
        r.assert_audio_ok();
        return;
    }
    panic!("止める説明が始まらない");
}

#[test]
fn start_button_opens_and_closes_settings() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.start_game();
    r.disable_explanations();
    let pad = PadId(0);
    let f = r.frame_index();
    r.p.schedule(f, PhysicalInput::PadConnected(pad));
    r.frame();
    for open in [true, false] {
        let f = r.frame_index();
        r.p.schedule(
            f,
            PhysicalInput::PadButtonDown {
                pad,
                button: PadButton::Start,
            },
        );
        r.p.schedule(
            f + 1,
            PhysicalInput::PadButtonUp {
                pad,
                button: PadButton::Start,
            },
        );
        r.frames(2);
        assert_eq!(r.app.ui().is_paused(r.app.tutorial()), open);
    }
    r.assert_audio_ok();
}

/// 止める説明の間は、毎フレーム tick が動かず、コマンドが溜まらず、記録も増えない。
fn assert_frozen(r: &Rig, tick: u64) {
    assert_eq!(r.tick(), tick, "止まっている間は進まない");
    assert!(r.game().pending().is_empty(), "コマンドが溜まらない");
    assert!(
        r.app.replay().is_none_or(|l| l.entries.is_empty()),
        "リプレイの記録が増えない"
    );
}

#[test]
fn app_t_06_pausing_explanation_stops_the_game() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.frames(TITLE_READY_FRAME);
    let mark = r.audio_len();
    r.down(Key::Z);
    r.frame();
    let d = directive(&r).expect("#1 の説明");
    assert!(d.pauses_game && d.stage == TutorialStageId(1) && d.page_count == 2);
    assert!(r.app.ui().is_paused(r.app.tutorial()));
    assert_eq!(r.tick(), 0);
    assert_eq!(
        r.ducks_since(mark),
        [true],
        "始まりのフレームで SetBgmDuck(true)"
    );
    r.up(Key::Z);
    r.frames(120);
    assert_frozen(&r, 0);

    let mut closing_frames = 0;
    for _ in 0..20 {
        if !pausing(&r) {
            break;
        }
        r.wait_ms(400);
        assert_frozen(&r, 0);
        r.down(Key::Z);
        r.frame();
        closing_frames += 1;
        if !pausing(&r) {
            assert_eq!(r.tick(), 1, "最後のページの決定のフレームから再開する");
            assert_eq!(
                r.ducks_since(mark),
                [true, false],
                "ページをめくっても鳴らし直さず、終わりで SetBgmDuck(false) が 1 回だけ"
            );
        } else {
            assert_frozen(&r, 0);
        }
        r.up(Key::Z);
        r.frame();
    }
    assert!(!pausing(&r), "説明が終わる");
    assert!(closing_frames >= 3, "#1 の 2 ページと #2 の 1 ページ");
    assert_eq!(directive(&r).map(|d| d.kind), Some(DirectiveKind::Guide));
    for i in 1..=60 {
        let before = r.tick();
        r.frame();
        assert_eq!(r.tick(), before + 1, "再開後は 1 フレーム 1 tick: {i}");
    }
    assert_eq!(r.ducks_since(mark), [true, false]);
    r.assert_audio_ok();
}

/// 止める説明の間に、コマンドになりうるキーを連打する。
fn mash(r: &mut Rig, tick: u64) {
    let keys = [
        Key::B,
        Key::Z,
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Down,
        Key::E,
        Key::Q,
        Key::C,
        Key::R,
        Key::Tab,
        Key::X,
    ];
    for k in keys {
        r.tap(k);
        assert_frozen(r, tick);
        assert!(pausing(r), "連打で止める説明が終わらない");
    }
}

#[test]
fn app_t_08_no_commands_while_paused() {
    let mem = common::memory();
    let cfg = common::with_memory(config(SeedPolicy::Fixed(SEED_A)), &mem);
    let mut r = Rig::new(HeadlessClock::Exact60, cfg);
    r.frames(TITLE_READY_FRAME);
    r.down(Key::Z);
    r.frame();
    r.up(Key::Z);
    r.frame();
    assert!(pausing(&r));
    mash(&mut r, 0);

    let mut stage3 = false;
    for _ in 0..30 {
        if !pausing(&r) {
            let d = directive(&r).expect("案内");
            assert_eq!(
                d.stage,
                TutorialStageId(2),
                "#2 の案内はカーソルを駅に合わせる"
            );
            let p0 = r.station_pos(StationId(0));
            r.go(p0);
            r.wait_ms(200);
        }
        if directive(&r).is_some_and(|d| d.stage == TutorialStageId(3) && d.pauses_game) {
            stage3 = true;
            break;
        }
        r.wait_ms(400);
        r.tap(Key::Z);
    }
    assert!(stage3, "#3 の説明に着く");
    let t = r.tick();
    mash(&mut r, t);
    r.assert_audio_ok();
}

#[test]
fn app_t_08_no_commands_while_settings_is_open() {
    let mem = common::memory();
    let cfg = common::with_memory(config(SeedPolicy::Fixed(SEED_A)), &mem);
    let mut r = Rig::new(HeadlessClock::Exact60, cfg);
    r.start_game();
    r.disable_explanations();
    r.tap(Key::B);
    r.tap(Key::Z);
    for s in [0, 1] {
        let p = r.station_pos(StationId(s));
        r.go(p);
        r.tap(Key::Z);
    }
    r.tap(Key::Z);
    r.wait_ms(350);
    assert!(r.app.replay().unwrap().entries.is_empty());

    r.tap(Key::Escape);
    assert!(r.app.ui().is_paused(r.app.tutorial()), "せってい が開く");
    let t = r.tick();
    r.tap(Key::Down);
    for k in [
        Key::Z,
        Key::Right,
        Key::Left,
        Key::B,
        Key::E,
        Key::Q,
        Key::C,
        Key::R,
        Key::Tab,
        Key::Z,
    ] {
        r.tap(k);
        r.frames(20);
        assert_frozen(&r, t);
        assert!(r.app.ui().is_paused(r.app.tutorial()));
    }
    r.tap(Key::Escape);
    assert!(!r.app.ui().is_paused(r.app.tutorial()), "せってい が閉じる");
    assert!(
        r.app.replay().unwrap().entries.is_empty(),
        "閉じただけではコマンドは出ない"
    );
    r.wait_ms(350);
    r.tap(Key::Z);
    r.frames(2);
    assert_eq!(
        r.game().view().lines().len(),
        1,
        "作りかけの路線が残っていて、閉じた後に作れる"
    );
    assert_eq!(r.app.replay().unwrap().entries.len(), 1);
    r.assert_audio_ok();
}

#[test]
fn app_t_08_confirm_is_ignored_during_the_page_guard() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.frames(TITLE_READY_FRAME);
    r.down(Key::Z);
    r.frame();
    r.up(Key::Z);
    r.frame();
    let page = directive(&r).expect("#1 の説明").page;
    for _ in 0..4 {
        r.tap(Key::Z);
        r.frames(2);
        assert_eq!(
            directive(&r).unwrap().page,
            page,
            "出てから 300ms は決定を受けない"
        );
    }
    assert_frozen(&r, 0);
    r.wait_ms(300);
    r.tap(Key::Z);
    assert_eq!(
        directive(&r).unwrap().page,
        page + 1,
        "待ち時間の後は決定でめくれる"
    );
    r.tap(Key::Z);
    assert_eq!(
        directive(&r).unwrap().page,
        page + 1,
        "次のページも出てすぐは受けない"
    );
    assert_frozen(&r, 0);
}

#[test]
fn app_t_08_settings_pauses_during_a_guide_too() {
    let mem = common::memory();
    let cfg = common::with_memory(config(SeedPolicy::Fixed(SEED_A)), &mem);
    let mut r = Rig::new(HeadlessClock::Exact60, cfg);
    r.start_game();
    read_pages(&mut r);
    let d = directive(&r).expect("案内");
    assert!(d.kind == DirectiveKind::Guide && !d.pauses_game);
    r.frames(10);
    let t = r.tick();
    assert!(
        !r.app.ui().is_paused(r.app.tutorial()),
        "案内だけでは止まらない"
    );
    let mark = r.audio_len();
    r.tap(Key::Escape);
    assert!(
        r.app.ui().is_paused(r.app.tutorial()),
        "案内が出ていても せってい の間は止まる"
    );
    let t_open = r.tick();
    assert!(t_open <= t + 2);
    r.frames(60);
    assert_frozen(&r, t_open);
    assert_eq!(r.ducks_since(mark), [true]);
    r.tap(Key::Escape);
    assert!(!r.app.ui().is_paused(r.app.tutorial()));
    let before = r.tick();
    r.frames(10);
    assert_eq!(r.tick(), before + 10, "閉じた後は 1 フレーム 1 tick");
    assert_eq!(r.ducks_since(mark), [true, false]);
    r.assert_audio_ok();
}
