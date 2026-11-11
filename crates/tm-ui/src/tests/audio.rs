//! 音の命令を `UiHarness` で確かめる(13 §10.2 の「音」)。

use tm_assets::sound::{
    BGM_GAME_1, BGM_GAME_2, BGM_TITLE, JINGLE_COMPANY_UP, SE_CONFIRM, SE_STATION_NEW, sound_data,
};
use tm_core::{CompanyLevel, GameEvent};
use tm_input::Action;
use tm_platform_api::{AudioBus, AudioCmd, MonoTime, SoundId};
use tm_tutorial::UiEvent;

use super::{buy_all, create_line, harness_with_line};
use crate::AppRequest;
use crate::testing::{DEFAULT_SEED, FRAME_US, UiHarness};

const MARGIN_US: u64 = 100_000;

fn len_us(id: SoundId) -> u64 {
    let data = sound_data().expect("音のデータ");
    data[usize::from(id.0)].samples.len() as u64 * 1_000_000 / 22_050
}

fn boot_cmds() -> [AudioCmd; 4] {
    [
        AudioCmd::SetVolume {
            bus: AudioBus::Bgm,
            level: 6,
        },
        AudioCmd::SetVolume {
            bus: AudioBus::Se,
            level: 8,
        },
        AudioCmd::StopBgm { fade_ms: 0 },
        AudioCmd::PlayBgm {
            id: BGM_TITLE,
            crossfade_ms: 0,
        },
    ]
}

#[test]
fn first_update_sends_volumes_and_title_bgm_once() {
    let mut h = UiHarness::new_title();
    h.frames(1);
    assert_eq!(h.audio, boot_cmds());
    h.frames(30);
    assert_eq!(h.audio, boot_cmds());
}

#[test]
fn title_confirm_plays_confirm_then_game_bgm_in_the_same_frame() {
    let mut h = UiHarness::new_title();
    h.frames(31);
    h.key_down(Action::Confirm);
    h.frames(1);
    assert_eq!(h.requests, [AppRequest::StartNewGame]);
    let ui: Vec<AudioCmd> = h.audio[4..]
        .iter()
        .copied()
        .filter(|c| !matches!(c, AudioCmd::SetBgmDuck(_)))
        .collect();
    assert_eq!(
        ui,
        [
            AudioCmd::PlaySe(SE_CONFIRM),
            AudioCmd::PlayBgm {
                id: BGM_GAME_1,
                crossfade_ms: 0
            }
        ]
    );
}

#[test]
fn reset_stops_and_restarts_title_bgm() {
    let mut h = harness_with_line();
    let n = h.audio.len();
    h.reset_to_title();
    assert_eq!(
        h.audio[n..],
        [
            AudioCmd::StopBgm { fade_ms: 0 },
            AudioCmd::PlayBgm {
                id: BGM_TITLE,
                crossfade_ms: 0
            }
        ]
    );
    h.frames(5);
    assert_eq!(h.audio.len(), n + 2, "起動時の命令は出し直さない");
}

#[test]
fn line_created_plays_line_new_without_confirm() {
    let mut h = UiHarness::with_game(super::plan(), false);
    h.frames(1);
    let n = h.audio.len();
    h.game
        .as_mut()
        .expect("ゲーム")
        .enqueue(create_line(&[0, 1]));
    h.frames(1);
    assert_eq!(
        h.audio[n..],
        [AudioCmd::PlaySe(tm_assets::sound::SE_LINE_NEW)]
    );
}

/// 1 フレームぶんの記録。
struct Frame {
    now: u64,
    audio: Vec<AudioCmd>,
    fanfare: usize,
    stations: usize,
}

fn step(h: &mut UiHarness) -> Frame {
    let (a, e) = (h.audio.len(), h.ui_events.len());
    h.frames(1);
    Frame {
        now: h.now.0,
        audio: h.audio[a..].to_vec(),
        fanfare: h.ui_events[e..]
            .iter()
            .filter(|ev| **ev == UiEvent::FanfareFinished)
            .count(),
        stations: h.game.as_ref().expect("ゲーム").view().stations().len(),
    }
}

/// 会社レベルが上がるまで買い、ジングルの後の `se_station_new` と `FanfareFinished` の時刻を確かめる。
#[test]
fn level_up_jingles_station_se_and_fanfare_timing() {
    let mut h = UiHarness::from_game(super::cheap_fast_game(), false);
    h.game
        .as_mut()
        .expect("ゲーム")
        .enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    let mut spawned_cycles = 0;
    let mut saw_game_2 = false;
    for _ in 0..30 {
        if h.game.as_ref().expect("ゲーム").view().company().level.0 >= 6 && spawned_cycles > 0 {
            break;
        }
        let before = h.game.as_ref().expect("ゲーム").view().stations().len();
        buy_all(&mut h);
        let mut f = step(&mut h);
        for _ in 0..4 {
            if f.audio.iter().any(|c| matches!(c, AudioCmd::PlayJingle(_))) {
                break;
            }
            f = step(&mut h);
        }
        let mut busy = f.now;
        for (i, c) in f.audio.iter().enumerate() {
            match *c {
                AudioCmd::PlayJingle(j) => busy += len_us(j) + MARGIN_US,
                AudioCmd::PlayBgm { id, crossfade_ms } => {
                    assert!(
                        matches!(f.audio[i - 1], AudioCmd::PlayJingle(_)),
                        "PlayBgm はジングルの直後"
                    );
                    assert_eq!(crossfade_ms, 0);
                    saw_game_2 |= id == BGM_GAME_2;
                }
                AudioCmd::PlaySe(id) => {
                    assert_ne!(id, SE_STATION_NEW, "ジングルと同時には鳴らさない")
                }
                _ => {}
            }
        }
        if busy == f.now {
            continue;
        }
        let spawned = f.stations > before;
        let mut station_at = None;
        let mut fanfare_at = Vec::new();
        while f.now < busy + 10_000_000 {
            f = step(&mut h);
            if f.audio.contains(&AudioCmd::PlaySe(SE_STATION_NEW)) {
                assert!(station_at.is_none(), "1 回だけ");
                station_at = Some(f.now);
            }
            if f.fanfare > 0 {
                fanfare_at.extend(std::iter::repeat_n(f.now, f.fanfare));
            }
        }
        let settled = match station_at {
            Some(t) => {
                assert!(spawned);
                assert!(t >= busy && t < busy + FRAME_US, "{t} は {busy} の直後");
                (t + len_us(SE_STATION_NEW)).max(busy)
            }
            None => {
                assert!(!spawned, "駅が出たら鳴らす");
                busy
            }
        };
        assert_eq!(fanfare_at.len(), 1, "FanfareFinished は 1 回");
        let due = settled + MARGIN_US;
        assert!(
            fanfare_at[0] >= due && fanfare_at[0] < due + FRAME_US,
            "{} は {due} の直後",
            fanfare_at[0]
        );
        spawned_cycles += usize::from(spawned);
    }
    assert!(spawned_cycles > 0, "駅の出る会社レベルアップを通った");
    assert!(saw_game_2, "Lv5 で bgm_game_2 に変わる");
    assert!(
        !h.audio.iter().any(|c| matches!(c, AudioCmd::SetBgmDuck(_))),
        "ジングルではダッキングしない"
    );
}

fn confetti(h: &UiHarness) -> bool {
    h.draw_text()
        .lines()
        .any(|l| l.starts_with("Fill ") && l.contains(" 4x4 "))
}

/// ゴールの `FanfareFinished` は anim のお祝いの演出(紙ふぶき)が終わってから出る [13 §6.3 #14]。
#[test]
fn goal_fanfare_follows_the_celebration() {
    let mut h = UiHarness::with_game(super::plan(), false);
    h.frames(2);
    assert!(!confetti(&h));
    let start = h.now.0;
    let mut out = Vec::new();
    h.ui.observe(
        h.now,
        &tm_core::GameEvent::GoalReached {
            total_earned: tm_core::Money::ONE,
        },
        &mut out,
    );
    assert_eq!(out, [AudioCmd::PlayJingle(tm_assets::sound::JINGLE_GOAL)]);
    let mut anim_end = None;
    let mut fanfare_at = None;
    for _ in 0..600 {
        let e = h.ui_events.len();
        h.frames(1);
        if anim_end.is_none() && !confetti(&h) {
            anim_end = Some(h.now.0);
        }
        if h.ui_events[e..].contains(&UiEvent::FanfareFinished) {
            fanfare_at = Some(h.now.0);
            break;
        }
    }
    let (anim_end, fanfare_at) = (anim_end.expect("演出が終わる"), fanfare_at.expect("出る"));
    let celebration = len_us(tm_assets::sound::JINGLE_GOAL).max(3_000_000);
    assert!(anim_end <= fanfare_at, "{anim_end} の後に出す");
    assert!(
        fanfare_at - anim_end <= 2 * MARGIN_US + 2 * FRAME_US,
        "演出の終わり {anim_end} から遅れすぎ {fanfare_at}"
    );
    let due =
        start + celebration.max(len_us(tm_assets::sound::JINGLE_GOAL) + MARGIN_US) + MARGIN_US;
    assert!(fanfare_at >= due && fanfare_at < due + FRAME_US);
}

/// 止める説明でダッキングしている間も曲の時間を数え、2 秒のクロスフェードで次の曲へ移る [14 PL-D-31]。
#[test]
fn rotation_counts_while_ducked_by_a_pausing_explanation() {
    let mut h = UiHarness::with_seed(DEFAULT_SEED, true, MonoTime(0));
    let mut out = Vec::new();
    h.ui.observe(
        h.now,
        &GameEvent::CompanyLevelUp {
            new_level: CompanyLevel(5),
            all_features_unlocked: false,
        },
        &mut out,
    );
    assert_eq!(
        out,
        [
            AudioCmd::PlayJingle(JINGLE_COMPANY_UP),
            AudioCmd::PlayBgm {
                id: BGM_GAME_2,
                crossfade_ms: 0
            }
        ]
    );
    assert!(
        h.ui.is_paused(h.tutorial.as_ref()),
        "最初の説明は止める説明"
    );
    let due = h.now.0 + len_us(JINGLE_COMPANY_UP) + MARGIN_US + 360_000_000;
    let mut log = Vec::new();
    while h.now.0 < due + 2 * FRAME_US {
        let a = h.audio.len();
        h.frames(1);
        assert!(h.ui.is_paused(h.tutorial.as_ref()), "説明は出たまま");
        log.extend(
            h.audio[a..]
                .iter()
                .filter(|c| matches!(c, AudioCmd::PlayBgm { .. } | AudioCmd::SetBgmDuck(_)))
                .map(|&c| (h.now.0, c)),
        );
    }
    assert_eq!(
        log.iter().map(|&(_, c)| c).collect::<Vec<_>>(),
        [AudioCmd::PlayBgm {
            id: BGM_GAME_1,
            crossfade_ms: 2000
        }],
        "ダッキングしたまま次の曲へ移る"
    );
    assert!(
        log[0].0 >= due && log[0].0 < due + FRAME_US,
        "{} は {due} の直後",
        log[0].0
    );
}
