//! APP-T-09〜11: リセット、リセットで保つもの、新しいシード(16 §2.4)。

mod common;

use common::{Rig, SEED_A, config, memory, with_memory};
use tm_app::SeedPolicy;
use tm_assets::sound::{BGM_TITLE, SE_RESET};
use tm_platform_api::{AudioCmd, HeadlessClock, Key};

#[test]
fn app_t_09_reset_to_title() {
    let mut r = Rig::new(HeadlessClock::Exact60, config(SeedPolicy::Fixed(SEED_A)));
    r.start_game();
    r.disable_explanations();
    r.frames(30);
    let mark = r.audio_len();
    r.reset_holding_confirm();
    assert!(r.app.game().is_none() && r.app.tutorial().is_none());
    let frame = r.frame_index() - 1;
    let cmds: Vec<AudioCmd> = r
        .audio_since(mark)
        .into_iter()
        .filter(|(f, _)| *f == frame)
        .map(|(_, c)| c)
        .collect();
    let at = |want: AudioCmd| cmds.iter().position(|c| *c == want);
    let se = at(AudioCmd::PlaySe(SE_RESET)).expect("se_reset");
    let stop = at(AudioCmd::StopBgm { fade_ms: 0 }).expect("StopBgm");
    let play = at(AudioCmd::PlayBgm {
        id: BGM_TITLE,
        crossfade_ms: 0,
    })
    .expect("PlayBgm(BGM_TITLE)");
    let duck = at(AudioCmd::SetBgmDuck(false)).expect("SetBgmDuck(false)");
    assert!(se < stop && stop < play && play < duck, "{cmds:?}");

    r.frames(90);
    assert!(r.app.game().is_none(), "押したままの決定では始まらない");
    r.down(Key::Enter);
    r.frame();
    assert!(
        r.app.game().is_some(),
        "押したままの Z は数えないので、Enter の押し下げが決定になる(mapper.reset)"
    );
    r.up(Key::Enter);
    r.up(Key::Z);
    r.frame();

    r.disable_explanations();
    r.reset_holding_confirm();
    r.frames(60);
    r.up(Key::Z);
    r.frames(2);
    assert!(r.app.game().is_none());
    r.down(Key::Z);
    r.frame();
    assert!(r.app.game().is_some(), "離して押し直すと始まる");
    assert_eq!(r.app.stats().sessions_started, 3);
    r.assert_audio_ok();
}

#[test]
fn app_t_10_reset_keeps_settings() {
    let mem = memory();
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        with_memory(config(SeedPolicy::Fixed(SEED_A)), &mem),
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
    r.tap(Key::Escape);
    assert!(!r.app.ui().show_explanations());
    let music: Vec<u8> = r
        .audio_since(0)
        .into_iter()
        .filter_map(|(_, c)| match c {
            AudioCmd::SetVolume {
                bus: tm_platform_api::AudioBus::Bgm,
                level,
            } => Some(level),
            _ => None,
        })
        .collect();
    assert_eq!(music.last(), Some(&3), "{music:?}");
    r.frames(10);

    let mark = r.audio_len();
    r.reset_holding_confirm();
    r.up(Key::Z);
    r.frames(40);
    r.start_game();
    r.frames(10);
    assert!(
        r.audio_since(mark)
            .iter()
            .all(|(_, c)| !matches!(c, AudioCmd::SetVolume { .. })),
        "音量を既定値で出し直さない"
    );
    assert!(!r.app.ui().show_explanations());
    let t = r.app.tutorial().expect("新しいゲーム");
    assert!(t.directive().is_none(), "新しい Tutorial は「しない」");
    assert_eq!(mem.lock().unwrap().len(), 1, "リセットで 1 本書き出す");
    assert_eq!(r.app.stats().replays_written, 1);
    r.assert_audio_ok();
}

#[test]
fn app_t_11_new_seed_for_each_game() {
    let mut r = Rig::new(
        HeadlessClock::Exact60,
        config(SeedPolicy::Script(vec![1, 2])),
    );
    r.start_game();
    assert_eq!(r.app.seed(), Some(1));
    assert_eq!(r.game().config().seed, 1);
    r.disable_explanations();
    r.reset_holding_confirm();
    r.up(Key::Z);
    r.frames(40);
    r.start_game();
    assert_eq!(r.app.seed(), Some(2));
    assert_eq!(r.game().config().seed, 2);
    r.assert_audio_ok();
}
