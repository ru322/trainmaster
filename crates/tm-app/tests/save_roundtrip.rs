//! APP-T-24、25、28、29: セーブの往復、保存のきっかけ、リセットと「初めから」、設定の保存(16 §2.4、§1.20)。

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use common::{
    Rig, SEED_A, Spy, TITLE_READY_FRAME, config, initial_cursor, memory, newest, saving, wait_saved,
};
use tm_app::save::{SAVE_INTERVAL_TICKS, SaveFile, decode_settings, encode_settings};
use tm_app::{App, MemStore, ReplayPolicy, SeedPolicy, Slot};
use tm_core::{BalanceTable, CHECKPOINT_INTERVAL_TICKS, Command, StationId, Tick, replay};
use tm_platform_api::{AudioBus, AudioCmd, HeadlessClock, HeadlessPlatform, Key};
use tm_tutorial::TutorialStageId;
use tm_ui::Settings;

const PLAY_TICKS: u64 = 36_000;
const TAIL_FRAMES: u64 = 5 * 60 * 60;

/// 600 tick ごとの `state_hash` を集める。
struct Hashes(Rc<RefCell<Vec<(u64, u64)>>>);

impl Spy for Hashes {
    fn key(&mut self, _key: Key, _down: bool) {}

    fn frame(&mut self, app: &App, _p: &HeadlessPlatform) {
        let Some(g) = app.game() else { return };
        let t = g.now().0;
        let mut v = self.0.borrow_mut();
        if t % CHECKPOINT_INTERVAL_TICKS == 0 && v.last().is_none_or(|&(l, _)| l != t) {
            v.push((t, g.state_hash()));
        }
    }
}

/// 続きからの後に両方へ与える同じ入力(カーソルは続きからの初期位置から): 駅 0 と 1 で 1 つずつ強化する。
fn tail(r: &mut Rig) -> Vec<(u64, u64)> {
    let hashes = Rc::new(RefCell::new(Vec::new()));
    r.spy = Some(Box::new(Hashes(hashes.clone())));
    let start = r.frame_index();
    r.frames(60);
    for s in [0, 1] {
        let p = r.station_pos(StationId(s));
        r.go(p);
        r.tap(Key::Z);
        r.wait_ms(350);
        r.tap(Key::Z);
        r.frames(5);
        r.tap(Key::X);
        r.frames(3_600);
    }
    let left = TAIL_FRAMES - (r.frame_index() - start);
    r.frames(left);
    r.spy = None;
    hashes.take()
}

#[test]
fn app_t_24_save_and_continue() {
    let store = MemStore::new();
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r.build_extend_upgrade();
    wait_saved(&r.app);
    let from = store.history().len();
    let first = r.tick();
    while r.tick() < PLAY_TICKS {
        r.frame();
        wait_saved(&r.app);
    }
    let gaps: Vec<u64> = ticks(&store)[from - 1..]
        .windows(2)
        .map(|w| w[1] - w[0])
        .collect();
    assert!(
        gaps.iter().all(|&g| g == SAVE_INTERVAL_TICKS)
            && gaps.len() as u64 >= (PLAY_TICKS - first) / SAVE_INTERVAL_TICKS,
        "60 秒ごとに書かれる: {gaps:?}"
    );

    let pos = initial_cursor(r.game());
    r.go(pos);
    r.tap(Key::Escape);
    assert!(r.app.ui().is_settings_open());
    wait_saved(&r.app);
    let (tick, hash, tut) = (
        r.tick(),
        r.game().state_hash(),
        r.app.tutorial().unwrap().save(),
    );
    let (_, h, _) = newest(&store).expect("セーブ");
    assert_eq!((h.tick, h.state_hash), (tick, hash), "最後の保存");
    r.tap(Key::Escape);
    assert!(!r.app.ui().is_settings_open());

    let replays = memory();
    let mut cfg = saving(SeedPolicy::Fixed(99), &store);
    cfg.replay = ReplayPolicy::Memory(replays.clone());
    let mut r2 = Rig::new(HeadlessClock::Exact60, cfg);
    r2.frames(TITLE_READY_FRAME);
    assert!(r2.app.app_status().has_save && r2.app.app_status().save_enabled);
    r2.continue_game();
    assert_eq!(r2.tick(), tick, "続きからのフレームは進めない");
    assert_eq!(r2.game().state_hash(), hash);
    assert_eq!(r2.app.seed(), Some(SEED_A), "セーブのシード");
    assert_eq!(r2.app.tutorial().unwrap().save(), tut);
    assert_eq!(r2.app.stats().sessions_started, 1);
    let seq = newest(&store).unwrap().1.seq;
    while r2.tick() < r.tick() {
        r2.frame();
    }
    wait_saved(&r2.app);
    assert_eq!(
        newest(&store).unwrap().1.seq,
        seq,
        "続きからの直後は書かない"
    );
    assert_eq!(r2.tick(), r.tick());
    assert_eq!(r2.cursor, r.cursor);

    let a = tail(&mut r);
    let b = tail(&mut r2);
    assert_eq!(a.len(), 30);
    assert_eq!(a, b, "続けたものと読み込んだもの");
    let log = r2.app.replay().expect("リプレイ");
    let start = log.start.as_ref().expect("読み込んだ写しから");
    assert_eq!(
        (start.tick, start.state_hash, start.seed),
        (tick, hash, SEED_A)
    );
    let ups = log
        .entries
        .iter()
        .filter(|e| matches!(e.cmd, Command::LevelUp { .. }))
        .count();
    assert!(ups >= 1, "同じ入力で強化した: {:?}", log.entries);
    let mut ev = Vec::new();
    let g = replay(
        log,
        Arc::new(BalanceTable::builtin()),
        Tick(r2.tick()),
        &mut ev,
    )
    .expect("再生");
    assert_eq!(g.state_hash(), r2.game().state_hash());
    r.assert_audio_ok();
    r2.assert_audio_ok();
}

#[test]
fn app_t_24_explanation_restarts_from_its_first_page() {
    let store = MemStore::new();
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r.frames(TITLE_READY_FRAME);
    r.tap(Key::Z);
    let d = r.app.tutorial().unwrap().directive().copied().unwrap();
    assert!(d.pauses_game && d.stage == TutorialStageId(1) && d.page == 0);
    r.wait_ms(400);
    r.tap(Key::Z);
    let d = r.app.tutorial().unwrap().directive().copied().unwrap();
    assert_eq!((d.stage, d.page), (TutorialStageId(1), 1));
    r.frames(2 * SAVE_INTERVAL_TICKS);
    wait_saved(&r.app);
    assert_eq!(ticks(&store), [0], "止める説明の間は数えない・書かない");
    r.tap(Key::Escape);
    assert!(
        r.app.ui().is_settings_open(),
        "説明の間も せってい を開ける"
    );
    wait_saved(&r.app);
    let (_, _, bytes) = newest(&store).unwrap();
    let f = SaveFile::decode(&bytes, &BalanceTable::builtin()).unwrap();
    assert_eq!(f.body.tutorial, r.app.tutorial().unwrap().save());

    let mut r2 = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r2.continue_game();
    r2.frame();
    let d = r2.app.tutorial().unwrap().directive().copied().unwrap();
    assert_eq!(
        (d.stage, d.page, d.pauses_game),
        (TutorialStageId(1), 0, true),
        "その段階の 1 頁目から"
    );
    assert_eq!(r2.tick(), 0);
    assert!(r2.app.ui().is_paused(r2.app.tutorial()));
    r2.assert_audio_ok();
}

fn ticks(store: &MemStore) -> Vec<u64> {
    store.history().iter().filter_map(|w| w.tick).collect()
}

fn check_alternating(store: &MemStore) {
    let h = store.history();
    for w in h.windows(2) {
        assert_eq!(w[1].slot, w[0].slot.other(), "{h:?}");
        assert!(w[1].seq > w[0].seq, "{h:?}");
    }
}

#[test]
fn app_t_25_when_to_save() {
    let store = MemStore::new();
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r.start_game();
    wait_saved(&r.app);
    assert_eq!(ticks(&store), [0], "初めからの直後");
    assert_eq!(store.history()[0].slot, Slot::A);
    r.disable_explanations();
    wait_saved(&r.app);
    assert_eq!(ticks(&store), [0, 0], "せってい を開いたフレーム");
    let i = SAVE_INTERVAL_TICKS;
    while r.tick() < 2 * i + 10 {
        r.frame();
        if r.tick() == i || r.tick() == 2 * i {
            wait_saved(&r.app);
            assert_eq!(*ticks(&store).last().unwrap(), r.tick());
        }
    }
    assert_eq!(ticks(&store), [0, 0, i, 2 * i]);

    r.tap(Key::Escape);
    let opened = r.tick();
    wait_saved(&r.app);
    r.frames(2 * i);
    assert_eq!(r.tick(), opened, "せってい の間は進まない");
    r.tap(Key::Escape);
    while r.tick() < opened + i + 10 {
        r.frame();
    }
    wait_saved(&r.app);
    assert_eq!(
        ticks(&store),
        [0, 0, i, 2 * i, opened, opened + i],
        "止まっている間は数えない・書かない"
    );

    store.hold();
    r.tap(Key::Escape);
    assert!(store.wait_held(Duration::from_secs(5)));
    assert!(r.app.app_status().saving);
    let seq0 = newest(&store).unwrap().1.seq + 1;
    for _ in 0..2 {
        r.tap(Key::Escape);
        r.frames(3);
        r.tap(Key::Escape);
        assert!(r.app.app_status().saving, "書き込み中は保存中の印");
    }
    store.release();
    wait_saved(&r.app);
    let seqs: Vec<u64> = store.history().iter().filter_map(|w| w.seq).collect();
    assert_eq!(
        seqs[seqs.len() - 2..],
        [seq0, seq0 + 2],
        "書き込み中に来た保存は最新の 1 つだけ"
    );
    check_alternating(&store);

    r.tap(Key::Escape);
    assert!(!r.app.ui().is_settings_open());
    r.tap(Key::Escape);
    wait_saved(&r.app);
    let before = store.history().len();
    r.quit_in_settings();
    let at = r.tick();
    wait_saved(&r.app);
    assert_eq!(store.history().len(), before + 1, "ゲームを終わるとき");
    assert_eq!(*ticks(&store).last().unwrap(), at);
    let st = r.app.stats();
    assert_eq!(
        st.saves_written as usize,
        store.counts().slot_writes as usize
    );
    assert_eq!(st.save_failures, 0);
    check_alternating(&store);
    r.assert_audio_ok();
}

fn cheap() -> Arc<BalanceTable> {
    let mut spec = BalanceTable::builtin().spec().clone();
    for p in &mut spec.params {
        p.base_yen = 1;
        p.cost_ratio = 1;
        p.cost_steps.clear();
    }
    spec.economy.initial_money_yen = u64::MAX;
    spec.economy.track.c1_yen = 1;
    spec.economy.track.level_ratio = 1;
    spec.economy.track.tile_ratio = (1, 1);
    Arc::new(BalanceTable::from_spec(spec).expect("検証を通る表"))
}

#[test]
fn app_t_25_company_level_up_saves_in_that_frame() {
    let store = MemStore::new();
    let mut cfg = saving(SeedPolicy::Fixed(SEED_A), &store);
    cfg.balance = cheap();
    let mut r = Rig::new(HeadlessClock::Exact60, cfg);
    r.start_game();
    r.disable_explanations();
    r.build_first_line();
    wait_saved(&r.app);
    let level = r.game().view().company().level;
    let before = store.history().len();
    let mut up_tick = None;
    let step = |r: &mut Rig, up: &mut Option<u64>| {
        r.frame();
        if up.is_none() && r.game().view().company().level > level {
            *up = Some(r.tick());
        }
    };
    for _ in 0..30 {
        for (key, wait) in [(Key::R, 25), (Key::C, 1), (Key::X, 1)] {
            r.down(key);
            step(&mut r, &mut up_tick);
            r.up(key);
            for _ in 0..wait {
                step(&mut r, &mut up_tick);
            }
        }
        if up_tick.is_some() {
            break;
        }
    }
    let up_tick = up_tick.expect("会社レベルが上がる");
    wait_saved(&r.app);
    assert_eq!(store.history().len(), before + 1);
    assert_eq!(
        *ticks(&store).last().unwrap(),
        up_tick,
        "上がったフレームで書く"
    );
    let (_, h, _) = newest(&store).unwrap();
    assert!(h.tick < SAVE_INTERVAL_TICKS * 2);
    r.assert_audio_ok();
}

#[test]
fn app_t_28_reset_and_new_game_clear_the_save() {
    let store = MemStore::new();
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Script(vec![1, 2, 3]), &store),
    );
    r.start_game();
    r.disable_explanations();
    r.frames(100);
    wait_saved(&r.app);
    assert!(newest(&store).is_some());
    let settings = store.settings().expect("せってい を閉じて書いた");
    r.reset_holding_confirm();
    assert!(
        !r.app.app_status().has_save,
        "リセットの後は「初めから」だけ"
    );
    wait_saved(&r.app);
    assert!(store.slot(Slot::A).is_none() && store.slot(Slot::B).is_none());
    assert_eq!(store.counts().clears, 2, "初めからとリセット");
    assert_eq!(store.settings(), Some(settings.clone()), "設定は消さない");
    drop(r);
    let r2 = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    assert!(!r2.app.app_status().has_save);
    drop(r2);

    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Script(vec![1, 2, 3]), &store),
    );
    r.start_game();
    assert_eq!(r.app.seed(), Some(1));
    r.frames(10);
    wait_saved(&r.app);
    let old = newest(&store).unwrap().1;
    drop(r);

    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Script(vec![2]), &store),
    );
    r.frames(TITLE_READY_FRAME);
    assert!(r.app.app_status().has_save);
    r.tap(Key::Down);
    r.tap(Key::Z);
    assert!(r.app.game().is_none(), "確認が出る");
    for _ in 0..2 {
        r.tap(Key::Right);
        r.wait_ms(300);
        r.tap(Key::Z);
    }
    assert_eq!(r.app.seed(), Some(2), "新しいゲーム");
    wait_saved(&r.app);
    let (slot, h, _) = newest(&store).unwrap();
    assert_eq!(slot, Slot::A);
    assert!(h.tick <= 1, "始めたフレームで書く: {}", h.tick);
    assert!(h.seq > old.seq);
    assert!(
        store.slot(Slot::B).is_none(),
        "古いセーブを消してから新しいゲームを保存する"
    );
    assert_eq!(store.settings(), Some(settings));
    drop(r);
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r.continue_game();
    assert_eq!(r.app.seed(), Some(2), "新しい App は新しいゲームを読む");
    r.assert_audio_ok();
}

fn volumes(r: &Rig, bus: AudioBus) -> Vec<u8> {
    r.audio_since(0)
        .into_iter()
        .filter_map(|(_, c)| match c {
            AudioCmd::SetVolume { bus: b, level } if b == bus => Some(level),
            _ => None,
        })
        .collect()
}

#[test]
fn app_t_29_settings_file() {
    let store = MemStore::new();
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r.start_game();
    r.tap(Key::Escape);
    r.tap(Key::Down);
    for _ in 0..3 {
        r.tap(Key::Left);
    }
    for _ in 0..3 {
        r.tap(Key::Down);
    }
    r.tap(Key::Right);
    assert!(store.settings().is_none(), "閉じるまで書かない");
    r.tap(Key::Escape);
    wait_saved(&r.app);
    let want = Settings {
        music: 3,
        sound: 8,
        show_explanations: false,
    };
    assert_eq!(decode_settings(&store.settings().unwrap()), Ok(want));
    assert_eq!(store.counts().settings_writes, 1);
    r.tap(Key::Escape);
    r.tap(Key::Escape);
    wait_saved(&r.app);
    assert_eq!(
        store.counts().settings_writes,
        1,
        "変わっていなければ書かない"
    );
    drop(r);

    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r.frame();
    assert_eq!(volumes(&r, AudioBus::Bgm), [3], "最初のフレームの音量");
    assert_eq!(volumes(&r, AudioBus::Se), [8]);
    assert_eq!(r.app.ui().settings(), want);
    r.continue_game();
    r.frame();
    assert!(
        r.app.tutorial().unwrap().directive().is_none(),
        "Tutorial::restore に「出さない」が渡る"
    );
    drop(r);

    store.set_settings(Some(b"Settings(music: ".to_vec()));
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r.frame();
    assert_eq!(volumes(&r, AudioBus::Bgm), [6], "壊れていれば初期値");
    assert_eq!(r.app.ui().settings(), Settings::default());
    drop(r);

    store.set_settings(Some(
        encode_settings(Settings {
            music: 10,
            sound: 0,
            show_explanations: true,
        })
        .into_bytes(),
    ));
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.frame();
    assert_eq!(
        volumes(&r, AudioBus::Bgm),
        [6],
        "保存しない設定では読まない"
    );
}

#[test]
fn no_saving_mark_while_a_command_waits_for_its_tick() {
    let store = MemStore::new();
    let mut r = Rig::new(
        HeadlessClock::FixedUs(8_000),
        saving(SeedPolicy::Fixed(SEED_A), &store),
    );
    r.frames(70);
    r.tap(Key::Z);
    assert!(r.app.game().is_some());
    r.disable_explanations();
    r.tap(Key::B);
    r.tap(Key::Z);
    for s in [0, 1] {
        let p = r.station_pos(StationId(s));
        r.go(p);
        r.tap(Key::Z);
    }
    r.tap(Key::Z);
    r.frames(60);
    wait_saved(&r.app);
    let saves = store.history().len();
    loop {
        let t = r.tick();
        r.frame();
        if r.tick() > t {
            break;
        }
    }
    r.down(Key::Z);
    r.frame();
    assert_eq!(
        r.game().pending().len(),
        1,
        "0.48 tick のフレームでは step しない"
    );
    r.up(Key::Z);
    r.down(Key::Escape);
    r.frame();
    r.up(Key::Escape);
    assert!(r.app.ui().is_settings_open());
    for _ in 0..30 {
        r.frame();
        assert!(
            !r.app.app_status().saving,
            "写しを取れない間は保存中の印を出さない"
        );
    }
    assert_eq!(r.game().pending().len(), 1);
    r.tap(Key::Escape);
    r.frames(4);
    assert!(r.game().pending().is_empty());
    assert_eq!(r.game().view().lines().len(), 1);
    wait_saved(&r.app);
    assert_eq!(store.history().len(), saves, "捨てたきっかけでは書かない");
    r.assert_audio_ok();
}
