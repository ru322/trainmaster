//! APP-T-22: アプリ全体の長時間(1 週間用: ゲーム時間 16 時間、2 時間ごとに終わって続きから、8 時間目にリセット)と、
//! その縮小版(16 §2.4、02 §9.5)。

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use common::{Rig, Spy, TITLE_READY_FRAME, saving, wait_saved};
use tm_app::{App, MemStore, SeedPolicy};
use tm_platform_api::{AudioCmd, HeadlessClock, HeadlessPlatform, Key};

/// 生きているヒープのバイト数を数える。
struct CountingAlloc;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static BLOCKS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: `System` に委ねるだけで、数を足し引きする。
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Ordering::Relaxed);
        BLOCKS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        BLOCKS.fetch_sub(1, Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Ordering::Relaxed);
        BLOCKS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        LIVE.fetch_add(new, Ordering::Relaxed);
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// 1 フレームの `AudioCmd` の上限と、描画コマンドの上限(16 §2.4 APP-T-22)。
const MAX_AUDIO_PER_FRAME: usize = 64;
const MAX_DRAW_CMDS: usize = 2000;
/// 基準の後、タイトルでヒープが増えてよい幅(バイト)と、生きているブロックの数。
/// 幅の中には保存の文字列と test-support の `Ui::ui_log`(App から消せない)が増える分が入る。
const HEAP_SLACK: usize = 8 * 1024;
const BLOCK_SLACK: usize = 8;
const TICKS_PER_FRAME: u64 = 4;
const HOUR: u64 = 3600 * 60;
/// 描画コマンドの容量の上限(`MAX_DRAW_CMDS` の倍々の 1 段)。
const MAX_DRAW_CAP: usize = 2048;

/// `HeadlessPlatform` が溜める音の記録の分(`Vec` の倍々の容量)を引いた、生きているヒープ。
fn heap(r: &Rig) -> usize {
    let n = r.p.audio_log().len();
    let cap = if n == 0 {
        0
    } else {
        n.next_power_of_two().max(4)
    };
    LIVE.load(Ordering::Relaxed)
        .saturating_sub(cap * std::mem::size_of::<(u64, AudioCmd)>())
}

/// 全フレームの検査(開始・リセットのフレームも含む)。
#[derive(Default)]
struct Watch {
    audio_mark: usize,
    max_draw_cap: usize,
    max_draw_cmds: usize,
    max_audio: usize,
}

struct WatchSpy(Rc<RefCell<Watch>>);

impl Spy for WatchSpy {
    fn key(&mut self, _key: Key, _down: bool) {}

    fn frame(&mut self, app: &App, p: &HeadlessPlatform) {
        let mut w = self.0.borrow_mut();
        let log = p.audio_log();
        let mut per_frame = 0;
        let mut cur = u64::MAX;
        for (f, _) in &log[w.audio_mark..] {
            if *f != cur {
                cur = *f;
                per_frame = 0;
            }
            per_frame += 1;
            w.max_audio = w.max_audio.max(per_frame);
        }
        w.audio_mark = log.len();
        let draw = app.draw_list().cmds.len();
        w.max_draw_cmds = w.max_draw_cmds.max(draw);
        w.max_draw_cap = w.max_draw_cap.max(app.draw_list().cmds.capacity());
        assert!(
            w.max_audio <= MAX_AUDIO_PER_FRAME,
            "音の命令 {}",
            w.max_audio
        );
        assert!(draw <= MAX_DRAW_CMDS, "描画コマンド {draw}");
    }
}

/// 区切り: その tick まで放置してから、終わって続きからにするか、リセットするか。
#[derive(Clone, Copy, Debug)]
enum Cut {
    Continue(u64),
    Reset(u64),
    End(u64),
}

struct Soak {
    store: MemStore,
    seeds: Vec<u64>,
    watch: Rc<RefCell<Watch>>,
    /// (新しい App のタイトル、リセット後のタイトル)ごとの (ヒープ、ブロック)。
    heaps: [Vec<(usize, usize)>; 2],
    /// 「続きから」の区切りごとの、ゲーム中の (放置の 4 分の 1 の後、終わる直前) の (ヒープ、ブロック)。
    in_game: Vec<[(usize, usize); 2]>,
    frames: u64,
    idle_frames: u64,
    idle_us: u128,
    ticks: u64,
    saves: u32,
    continues: u32,
    apps: u32,
}

impl Soak {
    /// 新しい App。次の「初めから」は `next` 番目のゲーム(0 始まり)のシードを使う。
    fn rig(&mut self, next: usize) -> Rig {
        let mut r = Rig::new(
            HeadlessClock::FixedUs(66_666),
            saving(SeedPolicy::Script(self.seeds[next..].to_vec()), &self.store),
        );
        self.watch.borrow_mut().audio_mark = 0;
        r.spy = Some(Box::new(WatchSpy(self.watch.clone())));
        self.apps += 1;
        r
    }

    /// `kind` 0 = 新しい App のタイトル(続きからの前)、1 = リセット後のタイトル(画面の作り置きが残る)。
    fn measure(&mut self, r: &Rig, kind: usize) {
        self.heaps[kind].push((heap(r), BLOCKS.load(Ordering::Relaxed)));
    }

    /// この App を閉じる前の検査。
    fn close(&mut self, r: Rig) {
        let s = r.app.stats();
        assert_eq!(s.dropped_ticks, 0, "捨てた tick がない");
        assert_eq!(s.save_failures, 0);
        self.saves += s.saves_written;
        self.frames += r.frame_index();
        r.assert_audio_ok();
    }

    /// 30 分より長い放置では、最初の 4 分の 1 の後に描画コマンドの容量が増えないことも見る
    /// (容量は App ごとに増え方が違うので、App をまたいでは比べない)。そのときのヒープを返す。
    fn idle(&mut self, r: &mut Rig, until: u64) -> Option<(usize, usize)> {
        let begin = Instant::now();
        let from = r.frame_index();
        let quarter = r.tick() + (until.saturating_sub(r.tick())) / 4;
        let long = until.saturating_sub(r.tick()) >= HOUR / 2;
        let mut cap = None;
        let mut warm = None;
        while r.tick() < until {
            let before = r.tick();
            r.frame();
            assert_eq!(r.tick(), before + TICKS_PER_FRAME, "毎フレーム 4 tick");
            if long && cap.is_none() && r.tick() >= quarter {
                cap = Some(r.app.draw_list().cmds.capacity());
                warm = Some((heap(r), BLOCKS.load(Ordering::Relaxed)));
            }
        }
        if let Some(c) = cap {
            assert_eq!(
                r.app.draw_list().cmds.capacity(),
                c,
                "放置の間に描画コマンドの容量が増えた"
            );
        }
        self.idle_us += begin.elapsed().as_micros();
        self.idle_frames += r.frame_index() - from;
        warm
    }
}

fn start_session(r: &mut Rig) {
    r.frames(12);
    r.down(Key::Z);
    r.frame();
    assert!(r.app.game().is_some(), "決定でゲームが始まる");
    r.up(Key::Z);
    r.frame();
    if r.app.ui().show_explanations() {
        r.disable_explanations();
    }
    r.build_first_line();
}

/// ヒープの数が混ざらないよう、このバイナリの中では 1 つずつ回す。
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `games` の区切りのとおりに遊ぶ。ヒープは新しい App のタイトル(続きからの前)とリセット後のタイトルで測り、
/// それぞれ最初の 1 回(1 週間用では最初の 2 時間)と比べる。
fn soak(games: &[&[Cut]]) -> Soak {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = Soak {
        store: MemStore::new(),
        seeds: (1..=games.len() as u64 + 1).collect(),
        watch: Rc::new(RefCell::new(Watch::default())),
        heaps: [Vec::with_capacity(64), Vec::with_capacity(64)],
        in_game: Vec::with_capacity(64),
        frames: 0,
        idle_frames: 0,
        idle_us: 0,
        ticks: 0,
        saves: 0,
        continues: 0,
        apps: 0,
    };
    let mut r = s.rig(0);
    r.frames(TITLE_READY_FRAME);
    for (n, cuts) in games.iter().enumerate() {
        start_session(&mut r);
        assert!(!r.app.ui().show_explanations());
        assert_eq!(r.app.seed(), Some(n as u64 + 1), "ゲームごとに別のシード");
        for &cut in *cuts {
            match cut {
                Cut::Continue(t) => {
                    if let Some(warm) = s.idle(&mut r, t) {
                        s.in_game
                            .push([warm, (heap(&r), BLOCKS.load(Ordering::Relaxed))]);
                    }
                    r.quit_game();
                    wait_saved(&r.app);
                    let (tick, hash) = (r.tick(), r.game().state_hash());
                    s.close(r);
                    r = s.rig(n + 1);
                    r.frames(TITLE_READY_FRAME);
                    s.measure(&r, 0);
                    r.continue_game();
                    assert_eq!(
                        (r.tick(), r.game().state_hash()),
                        (tick, hash),
                        "続きからの直後"
                    );
                    s.continues += 1;
                    r.frame();
                }
                Cut::Reset(t) | Cut::End(t) => {
                    s.idle(&mut r, t);
                    s.ticks += r.tick();
                    if matches!(cut, Cut::End(_)) {
                        break;
                    }
                    r.reset_holding_confirm();
                    r.up(Key::Z);
                    r.frames(4);
                    wait_saved(&r.app);
                    assert!(!r.app.app_status().has_save);
                    s.measure(&r, 1);
                }
            }
        }
    }
    r.app.shutdown(tm_app::ShutdownReason::Quit);
    s.close(r);
    let w = s.watch.borrow();
    assert!(w.max_audio <= MAX_AUDIO_PER_FRAME);
    assert!(w.max_draw_cmds <= MAX_DRAW_CMDS);
    assert!(w.max_draw_cap <= MAX_DRAW_CAP, "{}", w.max_draw_cap);
    drop(w);
    // 基準は最初の App の放置の 4 分の 1 の後(終わる直前を入れると、App の中で溜まる分を見落とす)
    if let Some(&[(base, base_blocks), _]) = s.in_game.first() {
        for (i, &[(warm, warm_blocks), (end, end_blocks)]) in s.in_game.iter().enumerate() {
            assert!(
                end <= warm + HEAP_SLACK && end <= base + HEAP_SLACK,
                "{} 回目の区切り: ゲーム中のヒープが増えた {end}(放置の始め {warm}、最初 {base})",
                i + 1
            );
            assert!(
                end_blocks <= warm_blocks + BLOCK_SLACK && end_blocks <= base_blocks + BLOCK_SLACK,
                "{} 回目の区切り: ゲーム中のブロックが増えた {end_blocks}(放置の始め {warm_blocks}、最初 {base_blocks})",
                i + 1
            );
        }
    }
    for (kind, v) in s.heaps.iter().enumerate() {
        let Some(&(base, base_blocks)) = v.first() else {
            continue;
        };
        for (i, &(h, b)) in v.iter().enumerate().skip(1) {
            assert!(
                h <= base + HEAP_SLACK,
                "{kind}: {} 回目: ヒープが増えた {h} > {base} + {HEAP_SLACK} ({v:?})",
                i + 1
            );
            assert!(
                b <= base_blocks + BLOCK_SLACK,
                "{kind}: {} 回目: ブロックが増えた {b} > {base_blocks} + {BLOCK_SLACK} ({v:?})",
                i + 1
            );
        }
    }
    s
}

fn report(s: &Soak) {
    let w = s.watch.borrow();
    println!(
        "APP-V-06: App::frame {:.1} µs/フレーム({} フレーム、{} tick、App {} 個、続きから {} 回、保存 {} 回)、描画コマンド最大 {}(容量 {})、音の命令/フレーム最大 {}、タイトルのヒープとブロック(新しい App、リセット後) {:?}、ゲーム中(放置の 4 分の 1 の後、終わる直前) {:?}",
        s.idle_us as f64 / s.idle_frames as f64,
        s.frames,
        s.ticks,
        s.apps,
        s.continues,
        s.saves,
        w.max_draw_cmds,
        w.max_draw_cap,
        w.max_audio,
        s.heaps,
        s.in_game
    );
}

#[test]
fn app_t_22_soak_smoke() {
    let t = 7_200;
    let s = soak(&[
        &[Cut::Continue(t / 2), Cut::Reset(t)],
        &[Cut::Reset(t)],
        &[Cut::Reset(t)],
        &[Cut::Reset(t)],
    ]);
    assert_eq!((s.apps, s.continues), (2, 1));
    assert!(s.saves >= 4 * 2, "{}", s.saves);
    report(&s);
}

#[test]
#[ignore = "1 週間用(ゲーム時間 16 時間)。test:long で回す"]
fn app_t_22_soak_week() {
    let h = HOUR;
    let s = soak(&[
        &[
            Cut::Continue(2 * h),
            Cut::Continue(4 * h),
            Cut::Continue(6 * h),
            Cut::Continue(8 * h),
            Cut::Reset(8 * h),
        ],
        &[
            Cut::Continue(2 * h),
            Cut::Continue(4 * h),
            Cut::Continue(6 * h),
            Cut::End(8 * h),
        ],
    ]);
    assert_eq!(s.continues, 7);
    assert!(s.ticks >= 16 * h);
    assert!(s.saves >= 900, "60 秒ごとの保存: {}", s.saves);
    report(&s);
}
