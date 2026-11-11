//! APP-T-30: 「ゲームを終わる」(16 §1.15、14 PL-D-32、13 UI-D-33)。bin の終了コードは `main.rs` の単体テスト。

mod common;

use std::time::{Duration, Instant};

use common::{Rig, SEED_A, saving, wait_saved};
use tm_app::save::{decode_settings, encode_settings};
use tm_app::{AppConfig, AppExit, ExitAction, FrameOutcome, MemStore, SavePolicy, SeedPolicy, run};
use tm_platform_api::{
    AudioCmd, HeadlessClock, HeadlessConfig, HeadlessPlatform, Key, PhysicalInput, RecordPolicy,
};
use tm_ui::{Settings, testing::draw_text};

/// 13 の箱の左上(`tm-ui` の `layout::SAVING_BOX`・`POWER_BOX`)。
const SAVING_BOX_TL: &str = "Sprite ui_win_tl 232,164 ";
const POWER_BOX_TL: &str = "Sprite ui_win_tl 160,150 ";
/// 500ms のフェード(Exact60 のフレーム数)。
const FADE_FRAMES: u64 = 30;
/// 電源の箱の 1500ms。
const POWER_FRAMES: u64 = 90;
/// 保存を待つ上限の 5 秒。
const TIMEOUT_FRAMES: u64 = 300;
const WAIT: Duration = Duration::from_secs(5);

fn cfg(exit: ExitAction, store: Option<&MemStore>) -> AppConfig {
    let mut c = match store {
        Some(s) => saving(SeedPolicy::Fixed(SEED_A), s),
        None => AppConfig {
            seed: SeedPolicy::Fixed(SEED_A),
            save: SavePolicy::Off,
            ..AppConfig::default()
        },
    };
    c.exit_action = exit;
    c
}

/// ゲームを始めて 5 秒進め、せってい を開いた(開いたときの保存が済んだ)ところ。
fn playing(exit: ExitAction, store: Option<&MemStore>) -> Rig {
    let mut r = Rig::new(HeadlessClock::Exact60, cfg(exit, store));
    assert_eq!(
        r.app.app_status().power_off_on_quit,
        exit == ExitAction::PowerOff
    );
    r.start_game();
    r.disable_explanations();
    r.frames(300);
    assert!(r.tick() > 0);
    r.tap(Key::Escape);
    wait_saved(&r.app);
    r
}

/// 開いている せってい から「ゲームを終わる」。`QuitGame` のフレーム番号。
fn quit(r: &mut Rig) -> u64 {
    let from = r.audio_len();
    r.quit_in_settings();
    let stops: Vec<_> = r
        .audio_since(from)
        .into_iter()
        .filter(|(_, c)| matches!(c, AudioCmd::StopBgm { .. } | AudioCmd::PlayBgm { .. }))
        .collect();
    assert_eq!(stops.len(), 1, "{stops:?}");
    assert_eq!(stops[0].1, AudioCmd::StopBgm { fade_ms: 500 });
    let q = stops[0].0;
    assert_eq!(q + 2, r.frame_index(), "2 回目の「はい」を押したフレーム");
    q
}

/// `shutdown` と drop で書き込みスレッドを待ち続けない(空いたスレッドはすぐ止まる)。
fn close(mut r: Rig) {
    let t = Instant::now();
    r.app.shutdown(tm_app::ShutdownReason::Quit);
    drop(r);
    assert!(
        t.elapsed() < Duration::from_millis(900),
        "shutdown と drop: {:?}",
        t.elapsed()
    );
}

fn drawn(r: &Rig, tl: &str) -> bool {
    draw_text(r.app.draw_list()).contains(tl)
}

/// 1 フレーム回し、終了中の決まり(present と audio が毎フレーム、tick が進まない、音を出さない)を確かめる。
fn quitting_frame(r: &mut Rig, tick: u64) -> Option<AppExit> {
    let (f, calls, audio) = (r.frame_index(), r.audio_calls, r.audio_len());
    r.frame();
    assert_eq!(r.frame_index(), f + 1, "present");
    assert_eq!(r.audio_calls, calls + 1, "audio");
    assert_eq!(r.audio_len(), audio, "終了中は音の命令を出さない");
    assert_eq!(r.tick(), tick, "tick が進まない");
    assert!(r.app.ui().is_paused(r.app.tutorial()));
    r.exit
}

/// 決まったフレームごとにキーを押す(終了中の入力は無視される)。
fn mash(r: &mut Rig, n: u64) {
    const KEYS: [Key; 6] = [Key::Z, Key::Escape, Key::X, Key::Down, Key::B, Key::E];
    let f = r.frame_index();
    let k = KEYS[(n % KEYS.len() as u64) as usize];
    r.p.schedule(f, PhysicalInput::KeyDown(k));
    r.p.schedule(f + 1, PhysicalInput::KeyUp(k));
}

#[test]
fn app_t_30_power_off_waits_for_the_save_then_shows_the_power_box() {
    let store = MemStore::new();
    let mut r = playing(ExitAction::PowerOff, Some(&store));
    r.tap(Key::Down);
    r.tap(Key::Left);
    r.tap(Key::Up);
    let changed = r.app.ui().settings();
    assert_ne!(changed, Settings::default(), "音楽の音量を変えた");
    let writes = store.history().len();
    let settings_writes = store.counts().settings_writes;
    store.hold();
    let q = quit(&mut r);
    let tick = r.tick();
    assert!(store.wait_held(WAIT), "終わるときの保存");
    assert!(r.app.app_status().saving);
    for n in 0..200 {
        mash(&mut r, n);
        assert_eq!(quitting_frame(&mut r, tick), None, "保存が済むまで回る");
        assert!(drawn(&r, SAVING_BOX_TL) && !drawn(&r, POWER_BOX_TL));
    }
    store.release();
    wait_saved(&r.app);
    let mut n = 0;
    let exit = loop {
        n += 1;
        mash(&mut r, n);
        let e = quitting_frame(&mut r, tick);
        assert!(drawn(&r, POWER_BOX_TL) && !drawn(&r, SAVING_BOX_TL));
        if let Some(e) = e {
            break e;
        }
        assert!(n < 1_000);
    };
    assert_eq!(exit, AppExit::PowerOff);
    assert_eq!(n, POWER_FRAMES, "保存の完了から 1500ms 電源の箱");
    assert!(r.frame_index() - q > FADE_FRAMES);
    r.assert_audio_ok();
    close(r);
    let hist = store.history();
    assert_eq!(hist.len(), writes + 1, "保存は 1 回だけ");
    assert_eq!(hist.last().unwrap().tick, Some(tick), "終わったときの tick");
    assert_eq!(store.counts().settings_writes, settings_writes + 1);
    assert_eq!(decode_settings(&store.settings().unwrap()), Ok(changed));
}

#[test]
fn app_t_30_quit_requested_while_quitting_waits_at_most_1_second() {
    let store = MemStore::new();
    let mut r = playing(ExitAction::PowerOff, Some(&store));
    let writes = store.history().len();
    store.hold();
    quit(&mut r);
    assert!(store.wait_held(WAIT));
    let f = r.frame_index();
    r.p.schedule(f, PhysicalInput::QuitRequested);
    assert_eq!(r.frame(), FrameOutcome::Quit(AppExit::Normal));
    assert_eq!(r.frame_index(), f, "QuitRequested のフレームは処理しない");
    let t = Instant::now();
    r.app.shutdown(tm_app::ShutdownReason::Quit);
    drop(r);
    let took = t.elapsed();
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_millis(2_500),
        "{took:?}"
    );
    store.release();
    let t = Instant::now();
    while store.history().len() < writes + 1 && t.elapsed() < WAIT {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        store.history().len(),
        writes + 1,
        "保存は終わるときの 1 回だけ"
    );
}

#[test]
fn app_t_30_quit_requested_with_settings_open_writes_settings() {
    let store = MemStore::new();
    let mut r = playing(ExitAction::Exit, Some(&store));
    let settings_writes = store.counts().settings_writes;
    r.tap(Key::Down);
    r.tap(Key::Left);
    let changed = r.app.ui().settings();
    assert_ne!(changed, Settings::default());
    assert!(r.app.ui().is_settings_open());
    let f = r.frame_index();
    r.p.schedule(f, PhysicalInput::QuitRequested);
    assert_eq!(r.frame(), FrameOutcome::Quit(AppExit::Normal));
    close(r);
    assert_eq!(store.counts().settings_writes, settings_writes + 1);
    assert_eq!(decode_settings(&store.settings().unwrap()), Ok(changed));
}

#[test]
fn app_t_30_exit_ends_500ms_after_quit_game() {
    let store = MemStore::new();
    let mut r = playing(ExitAction::Exit, Some(&store));
    let writes = store.history().len();
    let q = quit(&mut r);
    let tick = r.tick();
    wait_saved(&r.app);
    let exit = loop {
        if let Some(e) = quitting_frame(&mut r, tick) {
            break e;
        }
        assert!(!drawn(&r, POWER_BOX_TL), "開発機は電源の箱を出さない");
    };
    assert_eq!(exit, AppExit::Normal);
    assert_eq!(r.frame_index() - q, FADE_FRAMES, "QuitGame から 500ms");
    assert_eq!(store.history().len(), writes + 1);
    assert_eq!(store.history().last().unwrap().tick, Some(tick));
    close(r);
}

#[test]
fn app_t_30_ends_without_saving() {
    for (exit, frames) in [
        (ExitAction::Exit, FADE_FRAMES),
        (ExitAction::PowerOff, 1 + POWER_FRAMES),
    ] {
        let mut r = playing(exit, None);
        assert!(!r.app.app_status().save_enabled);
        let q = quit(&mut r);
        let tick = r.tick();
        let mut power = u64::from(drawn(&r, POWER_BOX_TL));
        let got = loop {
            let e = quitting_frame(&mut r, tick);
            assert!(!drawn(&r, SAVING_BOX_TL));
            power += u64::from(drawn(&r, POWER_BOX_TL));
            if let Some(e) = e {
                break e;
            }
        };
        assert_eq!(r.frame_index() - q, frames, "{exit:?}");
        let want = match exit {
            ExitAction::Exit => (AppExit::Normal, 0),
            ExitAction::PowerOff => (AppExit::PowerOff, POWER_FRAMES),
        };
        assert_eq!((got, power), want);
        close(r);
    }
}

#[test]
fn app_t_30_save_failure_still_ends() {
    let store = MemStore::new();
    let mut r = playing(ExitAction::PowerOff, Some(&store));
    store.fail_next_writes(u32::MAX);
    store.hold();
    let q = quit(&mut r);
    let tick = r.tick();
    assert!(store.wait_held(WAIT));
    store.release();
    wait_saved(&r.app);
    let mut n = 0;
    while quitting_frame(&mut r, tick).is_none() {
        n += 1;
    }
    assert_eq!(r.exit, Some(AppExit::PowerOff));
    assert_eq!(n + 1, POWER_FRAMES);
    assert!(r.frame_index() - q > POWER_FRAMES);
    assert_eq!(r.app.stats().save_failures, 1);
    close(r);
}

#[test]
fn app_t_30_gives_up_when_the_save_takes_5_seconds() {
    for exit in [ExitAction::Exit, ExitAction::PowerOff] {
        let store = MemStore::new();
        let mut r = playing(exit, Some(&store));
        store.hold();
        let q = quit(&mut r);
        let tick = r.tick();
        assert!(store.wait_held(WAIT));
        while quitting_frame(&mut r, tick).is_none() {
            assert!(drawn(&r, SAVING_BOX_TL));
        }
        assert_eq!(r.frame_index() - q, TIMEOUT_FRAMES, "{exit:?}");
        let want = match exit {
            ExitAction::Exit => AppExit::Normal,
            ExitAction::PowerOff => AppExit::PowerOff,
        };
        assert_eq!(r.exit, Some(want));
        assert!(r.app.app_status().saving, "まだ書き込み中");
        let t = Instant::now();
        r.app.shutdown(tm_app::ShutdownReason::Quit);
        drop(r);
        assert!(
            t.elapsed() < Duration::from_millis(900),
            "shutdown と drop で書き込みを待たない: {:?}",
            t.elapsed()
        );
        store.release();
    }
}

fn headless(quit_at: u64) -> HeadlessPlatform {
    HeadlessPlatform::new(HeadlessConfig {
        clock: HeadlessClock::Exact60,
        record: RecordPolicy::LastOnly,
        quit_after_frames: Some(quit_at),
        fail_present_at: None,
        check_audio: true,
    })
}

fn no_explanations(store: &MemStore) {
    let s = Settings {
        show_explanations: false,
        ..Settings::default()
    };
    store.set_settings(Some(encode_settings(s).into_bytes()));
}

#[test]
fn app_t_30_quit_requested_in_game_saves_and_ends_normally() {
    let store = MemStore::new();
    no_explanations(&store);
    let mut p = headless(200);
    p.tap(common::TITLE_READY_FRAME, Key::Z, 1);
    let r = run(&mut p, cfg(ExitAction::PowerOff, Some(&store)));
    assert_eq!(r.ok(), Some(AppExit::Normal), "QuitRequested は 0");
    assert_eq!(p.frame_index(), 200);
    let want = 200 - common::TITLE_READY_FRAME;
    let t = Instant::now();
    while store.history().last().and_then(|w| w.tick) != Some(want) && t.elapsed() < WAIT {
        std::thread::sleep(Duration::from_millis(1));
    }
    let ticks: Vec<u64> = store.history().iter().filter_map(|w| w.tick).collect();
    assert_eq!(
        ticks.last(),
        Some(&want),
        "始めたフレームから QuitRequested の前のフレームまで 1 tick ずつ: {ticks:?}"
    );
}

#[test]
fn app_t_30_run_returns_power_off_after_quit_game() {
    let store = MemStore::new();
    no_explanations(&store);
    // 書き込みを止めておき、QuitGame から 5 秒で諦めさせる(フレーム数が書き込みスレッドの速さによらない)
    store.hold();
    let mut p = headless(10_000);
    let t = common::TITLE_READY_FRAME;
    let mut taps = vec![(t, Key::Z), (t + 40, Key::Escape)];
    taps.extend((0..6).map(|i| (t + 44 + 4 * i, Key::Down)));
    taps.extend([
        (t + 68, Key::Up),
        (t + 72, Key::Z),
        (t + 76, Key::Right),
        (t + 100, Key::Z),
        (t + 104, Key::Right),
        (t + 128, Key::Z),
    ]);
    for (f, k) in taps {
        p.tap(f, k, 1);
    }
    let r = run(&mut p, cfg(ExitAction::PowerOff, Some(&store)));
    assert_eq!(r.ok(), Some(AppExit::PowerOff));
    let q = t + 128;
    assert!(
        p.audio_log()
            .iter()
            .any(|&(f, c)| f == q && c == AudioCmd::StopBgm { fade_ms: 500 })
    );
    assert_eq!(p.frame_index() - q, TIMEOUT_FRAMES);
    assert!(store.wait_held(WAIT));
    store.release();
}

#[test]
fn app_t_30_quit_requested_waits_at_most_1_second_for_the_save() {
    let store = MemStore::new();
    no_explanations(&store);
    let mut p = headless(200);
    p.tap(common::TITLE_READY_FRAME, Key::Z, 1);
    let mut app = tm_app::App::new(&mut p, cfg(ExitAction::Exit, Some(&store))).unwrap();
    while app.frame(&mut p).unwrap() == tm_app::FrameOutcome::Continue {}
    wait_saved(&app);
    store.hold();
    let t = Instant::now();
    app.shutdown(tm_app::ShutdownReason::Quit);
    drop(app);
    let took = t.elapsed();
    assert!(store.wait_held(WAIT));
    assert!(
        took >= Duration::from_millis(900) && took < Duration::from_millis(2_500),
        "{took:?}"
    );
    store.release();
}
