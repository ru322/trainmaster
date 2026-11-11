//! APP-T-14〜15: 終了と描画の故障(16 §2.4)。APP-T-12・16 は `app.rs` の単体テスト。

mod common;

use common::{SEED_A, TITLE_READY_FRAME, config, memory, newest, saving, wait_saved, with_memory};
use tm_app::{
    App, AppError, AppExit, FrameOutcome, MemStore, SeedPolicy, ShutdownReason, Slot, run,
};
use tm_platform_api::{
    HeadlessClock, HeadlessConfig, HeadlessPlatform, Key, PhysicalInput, PlatformError,
    RecordPolicy,
};

fn platform(quit: Option<u64>, fail: Option<u64>) -> HeadlessPlatform {
    let mut p = HeadlessPlatform::new(HeadlessConfig {
        clock: HeadlessClock::Exact60,
        record: RecordPolicy::LastOnly,
        quit_after_frames: quit,
        fail_present_at: fail,
        check_audio: true,
    });
    p.tap(TITLE_READY_FRAME, Key::Z, 1);
    p
}

fn slot_ticks(store: &MemStore) -> Vec<u64> {
    store.history().iter().filter_map(|w| w.tick).collect()
}

#[test]
fn app_t_14_quit_after_frames_writes_replay() {
    let mem = memory();
    let mut cfg = with_memory(config(SeedPolicy::Fixed(SEED_A)), &mem);
    cfg.quit_after_frames = Some(120);
    let mut p = platform(None, None);
    assert_eq!(run(&mut p, cfg).ok(), Some(AppExit::Normal));
    assert_eq!(p.frame_index(), 120);
    let logs = mem.lock().unwrap();
    assert_eq!(logs.len(), 1, "shutdown(Quit) で書き出す");
    assert_eq!(logs[0].header.seed, SEED_A);
    assert!(p.audio_violations().is_empty());
}

#[test]
fn app_t_14_quit_after_frames_saves_the_game() {
    let store = MemStore::new();
    let mut cfg = saving(SeedPolicy::Fixed(SEED_A), &store);
    cfg.quit_after_frames = Some(120);
    let mut p = platform(None, None);
    let mut app = App::new(&mut p, cfg).unwrap();
    let out = (0..200).find_map(|_| match app.frame(&mut p).unwrap() {
        FrameOutcome::Continue => None,
        FrameOutcome::Quit(e) => Some(e),
    });
    assert_eq!(out, Some(AppExit::Normal));
    let tick = app.game().unwrap().now().0;
    wait_saved(&app);
    app.shutdown(ShutdownReason::Quit);
    assert_eq!(
        slot_ticks(&store).last(),
        Some(&tick),
        "shutdown(Quit) で今の tick を保存する"
    );
    assert_eq!(app.stats().saves_written, 2, "初めからと終わるとき");
    assert!(!app.app_status().saving, "1 秒以内に書き終える");
}

#[test]
fn app_t_14_quit_requested_stops_before_the_frame() {
    let mem = memory();
    let store = MemStore::new();
    let mut p = platform(Some(90), None);
    p.schedule(90, PhysicalInput::KeyDown(Key::Escape));
    let cfg = with_memory(saving(SeedPolicy::Fixed(SEED_A), &store), &mem);
    let mut app = App::new(&mut p, cfg).unwrap();
    let quit_at =
        (0..200).find(|_| app.frame(&mut p).unwrap() == FrameOutcome::Quit(AppExit::Normal));
    assert_eq!(quit_at, Some(90));
    assert_eq!(p.frame_index(), 90, "QuitRequested のフレームは処理しない");
    assert!(
        p.audio_log().iter().all(|(f, _)| *f < 90),
        "同じフレームの Escape も処理しない"
    );
    let tick = app.game().unwrap().now().0;
    wait_saved(&app);
    app.shutdown(ShutdownReason::Quit);
    assert_eq!(mem.lock().unwrap().len(), 1);
    assert_eq!(
        slot_ticks(&store).last(),
        Some(&tick),
        "ゲーム中なら保存する"
    );
    assert_eq!(store.history().len(), 2, "初めからと終わるとき");
    assert!(newest(&store).is_some());
}

#[test]
fn app_t_15_render_failure_returns_error_and_writes_replay() {
    let mem = memory();
    let store = MemStore::new();
    let mut p = platform(None, Some(100));
    let r = run(
        &mut p,
        with_memory(saving(SeedPolicy::Fixed(SEED_A), &store), &mem),
    );
    assert!(
        matches!(r, Err(AppError::Platform(PlatformError::Render(_)))),
        "{r:?}"
    );
    assert_eq!(p.frame_index(), 101);
    assert_eq!(mem.lock().unwrap().len(), 1, "エラーでも書き出す");
    assert_eq!(
        store.counts().slot_writes,
        1,
        "初めからの保存だけ。shutdown(Error) は保存しない"
    );
    assert!(store.slot(Slot::A).is_some());
}

#[test]
fn app_t_15_shutdown_after_an_error_does_not_save() {
    let store = MemStore::new();
    let mut p = platform(None, Some(100));
    let mut app = App::new(&mut p, saving(SeedPolicy::Fixed(SEED_A), &store)).unwrap();
    let err = (0..200).find_map(|_| app.frame(&mut p).err());
    assert!(matches!(
        err,
        Some(AppError::Platform(PlatformError::Render(_)))
    ));
    assert!(app.game().is_some());
    wait_saved(&app);
    let before = store.history().len();
    app.shutdown(ShutdownReason::Error);
    drop(app);
    assert_eq!(
        store.history().len(),
        before,
        "shutdown(Error) は保存しない"
    );
}
