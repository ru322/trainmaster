//! APP-T-21: `UiHarness` と `App` + `HeadlessPlatform(FixedUs(16_667))` の一致と、UI-V-12 の機械の分(16 §2.4)。

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::kid::{Kid, READ_MS};
use common::{Rig, Spy, TITLE_READY_FRAME, config, memory, with_memory};
use tm_app::{App, SeedPolicy};
use tm_assets::sound::SE_ERROR;
use tm_core::Command;
use tm_input::{Action, Bindings};
use tm_platform_api::{AudioCmd, HeadlessClock, HeadlessPlatform, Key, MonoTime};
use tm_tutorial::{TutorialStageId, stage_pages};
use tm_ui::testing::{FRAME_US, UiHarness, draw_text};

const US: u64 = 16_667;

fn action_of(key: Key) -> Action {
    Bindings::default_v1()
        .keys
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, a)| *a)
        .expect("割り当てのあるキー")
}

/// 同じ入力を `UiHarness` にも流し、毎フレームの終わりに突き合わせる。
struct Mirror {
    h: UiHarness,
    audio_mark: usize,
    h_audio_mark: usize,
    seen: Rc<Cell<Seen>>,
}

/// 突き合わせた量(何も比べていない通過を防ぐ)。
#[derive(Clone, Copy, Default)]
struct Seen {
    frames: u64,
    audio_cmds: u64,
    draw_lines: u64,
    pauses: u64,
}

impl Mirror {
    fn new(seed: u64, start: MonoTime, audio_mark: usize, seen: Rc<Cell<Seen>>) -> Mirror {
        let h = UiHarness::with_seed(seed, true, start);
        Mirror {
            h_audio_mark: h.audio.len(),
            h,
            audio_mark,
            seen,
        }
    }
}

fn first_diff(a: &str, b: &str) -> String {
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("{i} 行目\n App: {x}\n UI : {y}");
        }
    }
    format!("行数 App {} UI {}", a.lines().count(), b.lines().count())
}

impl Spy for Mirror {
    fn key(&mut self, key: Key, down: bool) {
        let a = action_of(key);
        if down {
            self.h.key_down(a);
        } else {
            self.h.key_up(a);
        }
    }

    fn frame(&mut self, app: &App, p: &HeadlessPlatform) {
        self.h.frames(1);
        let f = p.frame_index() - 1;
        assert_eq!(self.h.now.0, f * US, "フレーム {f}: 時刻");
        let app_audio: Vec<AudioCmd> = p.audio_log()[self.audio_mark..]
            .iter()
            .map(|(_, c)| *c)
            .collect();
        self.audio_mark = p.audio_log().len();
        let ui_audio = self.h.audio[self.h_audio_mark..].to_vec();
        self.h_audio_mark = self.h.audio.len();
        assert_eq!(app_audio, ui_audio, "フレーム {f}: AudioCmd の列");
        let app_text = draw_text(app.draw_list());
        let ui_text = self.h.draw_text();
        assert!(
            app_text == ui_text,
            "フレーム {f}: draw_text が違う {}",
            first_diff(&app_text, &ui_text)
        );
        let mut seen = self.seen.get();
        seen.frames += 1;
        seen.audio_cmds += app_audio.len() as u64;
        seen.draw_lines += app_text.lines().count() as u64;
        seen.pauses += u64::from(app.ui().is_paused(app.tutorial()));
        self.seen.set(seen);
        let g = app.game().expect("ゲーム中");
        let hg = self.h.game.as_ref().expect("ゲーム中");
        assert_eq!(g.now(), hg.now(), "フレーム {f}: game.now()");
        assert_eq!(g.state_hash(), hg.state_hash(), "フレーム {f}: state_hash");
        let log = app.replay().expect("リプレイ");
        assert_eq!(
            log.entries.len(),
            self.h.commands.len(),
            "フレーム {f}: コマンドの数"
        );
        let n = self.h.commands.len();
        if n > 0 {
            assert_eq!(
                log.entries[n - 1].cmd,
                self.h.commands[n - 1],
                "フレーム {f}: コマンド"
            );
        }
    }
}

fn run(seed: u64) -> (Kid, Seen) {
    assert_eq!(FRAME_US, US);
    let mem = memory();
    let cfg = with_memory(config(SeedPolicy::Fixed(seed)), &mem);
    let mut r = Rig::new(HeadlessClock::FixedUs(US), cfg);
    r.frames(TITLE_READY_FRAME);
    r.down(Key::Z);
    r.frame();
    assert!(r.app.game().is_some(), "決定でゲームが始まる");
    let start = MonoTime((r.frame_index() - 1) * US);
    let seen = Rc::new(Cell::new(Seen::default()));
    r.spy = Some(Box::new(Mirror::new(
        seed,
        start,
        r.audio_len(),
        seen.clone(),
    )));
    r.up(Key::Z);
    r.frame();
    r.frames(60);
    let mut kid = Kid::new(r);
    kid.hold_to_corner();
    kid.run_stages_1_to_6();
    kid.wait(2_000);
    (kid, seen.get())
}

#[test]
fn app_t_21_matches_ui_harness_for_stages_1_to_6() {
    for seed in [1, 2, 3] {
        let (kid, seen) = run(seed);
        let r = &kid.r;
        let t = r.app.tutorial().expect("チュートリアル");
        assert_eq!(
            t.shown_stages(),
            (1..=6).map(TutorialStageId).collect::<Vec<_>>(),
            "シード {seed}"
        );
        assert!(
            !r.p.audio_log()
                .iter()
                .any(|(_, c)| *c == AudioCmd::PlaySe(SE_ERROR)),
            "シード {seed}: 案内どおりの操作でエラー音が鳴らない"
        );
        let log = r.app.replay().expect("リプレイ");
        assert!(
            matches!(&log.entries.iter().map(|e| &e.cmd).collect::<Vec<_>>()[..], [
                Command::CreateLine { stations, .. },
                Command::ExtendLine { .. },
                Command::LevelUp { .. },
            ] if stations.len() == 2),
            "シード {seed}: {:?}",
            log.entries
        );
        let pages_of = |ids: &[u8]| -> u32 {
            ids.iter()
                .map(|&n| stage_pages(TutorialStageId(n)).len() as u32)
                .sum()
        };
        assert_eq!(kid.pages, pages_of(&[1, 2, 3, 4, 5, 6]), "読んだページの数");
        assert!(seen.frames > 3_000 && seen.audio_cmds > 20 && seen.pauses > 3_000);
        assert!(seen.draw_lines > seen.frames * 20, "描画が空ではない");
        let measured_s = kid.paused_frames * US / 1_000_000;
        let later_pages = pages_of(&[7, 8, 9, 10, 11, 12, 14]);
        let total_s = measured_s + u64::from(later_pages) * READ_MS / 1000;
        println!(
            "UI-V-12 シード {seed}: #1〜#6 で止まった時間 {measured_s} 秒({} ページ)、#7〜#14 は {later_pages} ページ × 5 秒を足して合計 {total_s} 秒、{} フレームを突き合わせた",
            kid.pages, seen.frames
        );
        assert!(total_s <= 200, "止まる時間の合計は 200 秒以下(UI-V-12)");
        r.assert_audio_ok();
    }
}
