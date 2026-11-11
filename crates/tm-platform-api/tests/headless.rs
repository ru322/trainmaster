#![cfg(feature = "headless")]

mod common;

use common::{atlas, bundle, rect, sound};
use tm_platform_api::{
    AssetBundle, AssetSummary, AudioBus, AudioCmd, AudioViolation, Color, DrawCmd, DrawList,
    HeadlessClock, HeadlessConfig, HeadlessPlatform, Key, LOGICAL_HEIGHT, LOGICAL_WIDTH, PadId,
    PhysicalInput, Platform, PlatformError, RecordPolicy, SoundId, SoundKind,
};

fn platform(cfg: HeadlessConfig) -> HeadlessPlatform {
    HeadlessPlatform::new(cfg)
}

fn polled(p: &mut HeadlessPlatform) -> Vec<PhysicalInput> {
    let mut out = Vec::new();
    p.poll_input(&mut out);
    out
}

fn next_frame(p: &mut HeadlessPlatform) {
    p.present(&DrawList::default()).unwrap();
}

#[test]
fn exact60_reaches_one_second_in_60_frames() {
    let mut p = platform(HeadlessConfig::default());
    assert_eq!(p.now().0, 0);
    let mut previous = 0;
    for _ in 0..60 {
        next_frame(&mut p);
        let now = p.now().0;
        assert!(matches!(now - previous, 16_666 | 16_667));
        previous = now;
    }
    assert_eq!(p.now().0, 1_000_000);
    for _ in 0..60 {
        next_frame(&mut p);
    }
    assert_eq!(p.now().0, 2_000_000);
}

#[test]
fn fixed_clock_uses_the_given_interval() {
    let cfg = HeadlessConfig {
        clock: HeadlessClock::FixedUs(33_333),
        ..HeadlessConfig::default()
    };
    let mut p = platform(cfg);
    next_frame(&mut p);
    next_frame(&mut p);
    assert_eq!(p.now().0, 66_666);
}

#[test]
fn advance_clock_affects_the_next_now() {
    let mut p = platform(HeadlessConfig::default());
    let before = p.now();
    p.advance_clock_us(500_000);
    assert_eq!(p.now().saturating_since(before), 500_000);
    next_frame(&mut p);
    assert_eq!(p.now().0, 16_666 + 500_000);
    p.advance_clock_us(1);
    assert_eq!(p.now().0, 16_666 + 500_001);
}

#[test]
fn schedule_returns_inputs_in_registration_order_at_their_frame() {
    let mut p = platform(HeadlessConfig::default());
    p.schedule(1, PhysicalInput::KeyDown(Key::Up));
    p.schedule(0, PhysicalInput::KeyDown(Key::A));
    p.schedule(1, PhysicalInput::PadConnected(PadId(3)));
    p.schedule(1, PhysicalInput::KeyUp(Key::Up));
    assert_eq!(polled(&mut p), [PhysicalInput::KeyDown(Key::A)]);
    assert_eq!(polled(&mut p), []);
    next_frame(&mut p);
    assert_eq!(
        polled(&mut p),
        [
            PhysicalInput::KeyDown(Key::Up),
            PhysicalInput::PadConnected(PadId(3)),
            PhysicalInput::KeyUp(Key::Up),
        ]
    );
    assert_eq!(polled(&mut p), []);
}

#[test]
fn overdue_inputs_are_returned_together_with_later_ones() {
    let mut p = platform(HeadlessConfig::default());
    p.schedule(0, PhysicalInput::KeyDown(Key::A));
    next_frame(&mut p);
    next_frame(&mut p);
    p.schedule(2, PhysicalInput::KeyDown(Key::B));
    assert_eq!(
        polled(&mut p),
        [
            PhysicalInput::KeyDown(Key::A),
            PhysicalInput::KeyDown(Key::B)
        ]
    );
}

#[test]
fn poll_input_appends_without_clearing() {
    let mut p = platform(HeadlessConfig::default());
    p.schedule(0, PhysicalInput::KeyDown(Key::B));
    let mut out = vec![PhysicalInput::KeyDown(Key::A)];
    p.poll_input(&mut out);
    assert_eq!(
        out,
        [
            PhysicalInput::KeyDown(Key::A),
            PhysicalInput::KeyDown(Key::B)
        ]
    );
}

#[test]
fn tap_schedules_down_then_up_after_the_hold() {
    let mut p = platform(HeadlessConfig::default());
    p.tap(1, Key::Enter, 2);
    assert_eq!(polled(&mut p), []);
    next_frame(&mut p);
    assert_eq!(polled(&mut p), [PhysicalInput::KeyDown(Key::Enter)]);
    next_frame(&mut p);
    assert_eq!(polled(&mut p), []);
    next_frame(&mut p);
    assert_eq!(polled(&mut p), [PhysicalInput::KeyUp(Key::Enter)]);
}

#[test]
fn quit_requested_after_the_given_number_of_frames() {
    let cfg = HeadlessConfig {
        quit_after_frames: Some(2),
        ..HeadlessConfig::default()
    };
    let mut p = platform(cfg);
    p.schedule(2, PhysicalInput::KeyDown(Key::A));
    next_frame(&mut p);
    assert_eq!(polled(&mut p), []);
    next_frame(&mut p);
    assert_eq!(
        polled(&mut p),
        [PhysicalInput::KeyDown(Key::A), PhysicalInput::QuitRequested]
    );
}

#[test]
fn present_fails_at_the_configured_frame() {
    let cfg = HeadlessConfig {
        fail_present_at: Some(2),
        ..HeadlessConfig::default()
    };
    let mut p = platform(cfg);
    let list = DrawList::default();
    assert_eq!(p.present(&list), Ok(()));
    assert_eq!(p.present(&list), Ok(()));
    assert!(matches!(p.present(&list), Err(PlatformError::Render(_))));
    assert_eq!(p.present(&list), Ok(()));
}

#[test]
fn load_assets_rejects_broken_bundles() {
    let ok_sound = || sound("se_a", SoundKind::Se, 10, None);
    let out_of_range_atlas = bundle(
        vec![atlas(4, 4)],
        vec![rect(0, 0, 0, 2, 2), rect(1, 0, 0, 2, 2)],
        vec![ok_sound()],
    );
    let looping_se = bundle(
        vec![atlas(4, 4)],
        vec![rect(0, 0, 0, 2, 2)],
        vec![sound("se_a", SoundKind::Se, 10, Some(0))],
    );
    for broken in [out_of_range_atlas, looping_se] {
        let mut p = platform(HeadlessConfig::default());
        assert!(matches!(
            p.load_assets(&broken),
            Err(PlatformError::Assets(_))
        ));
        assert_eq!(p.assets_summary(), None);
    }
}

#[test]
fn load_assets_succeeds_once() {
    let mut p = platform(HeadlessConfig::default());
    assert_eq!(p.assets_summary(), None);
    p.load_assets(AssetBundle::test_minimal()).unwrap();
    assert_eq!(
        p.assets_summary(),
        Some(AssetSummary {
            atlases: 1,
            sprites: 2,
            sounds: 2
        })
    );
    assert!(matches!(
        p.load_assets(AssetBundle::test_minimal()),
        Err(PlatformError::Assets(_))
    ));
}

const BGM: SoundId = SoundId(0);
const SE: SoundId = SoundId(1);

#[test]
fn check_audio_reports_wrong_kind_unknown_id_and_volume() {
    let mut p = platform(HeadlessConfig::default());
    p.load_assets(AssetBundle::test_minimal()).unwrap();
    next_frame(&mut p);
    let wrong_se = AudioCmd::PlaySe(BGM);
    let wrong_bgm = AudioCmd::PlayBgm {
        id: SE,
        crossfade_ms: 0,
    };
    let unknown = AudioCmd::PlayJingle(SoundId(2));
    let stop_unknown = AudioCmd::StopSe(SoundId(9));
    p.audio(&[
        AudioCmd::PlaySe(SE),
        wrong_se,
        AudioCmd::PlayBgm {
            id: BGM,
            crossfade_ms: 10,
        },
        wrong_bgm,
        unknown,
        stop_unknown,
        AudioCmd::StopBgm { fade_ms: 0 },
        AudioCmd::SetBgmDuck(true),
        AudioCmd::SetVolume {
            bus: AudioBus::Se,
            level: 10,
        },
        AudioCmd::SetVolume {
            bus: AudioBus::Bgm,
            level: 11,
        },
    ]);
    assert_eq!(
        p.audio_violations(),
        [
            AudioViolation::WrongKind {
                frame: 1,
                cmd: wrong_se
            },
            AudioViolation::WrongKind {
                frame: 1,
                cmd: wrong_bgm
            },
            AudioViolation::UnknownId {
                frame: 1,
                cmd: unknown
            },
            AudioViolation::UnknownId {
                frame: 1,
                cmd: stop_unknown
            },
            AudioViolation::VolumeOutOfRange {
                frame: 1,
                level: 11
            },
        ]
    );
    assert_eq!(p.audio_log().len(), 10);
    assert!(p.audio_log().iter().all(|&(frame, _)| frame == 1));
}

#[test]
fn check_audio_can_be_disabled_and_needs_a_table() {
    let cfg = HeadlessConfig {
        check_audio: false,
        ..HeadlessConfig::default()
    };
    let mut p = platform(cfg);
    p.load_assets(AssetBundle::test_minimal()).unwrap();
    p.audio(&[
        AudioCmd::PlaySe(BGM),
        AudioCmd::SetVolume {
            bus: AudioBus::Se,
            level: 99,
        },
    ]);
    assert_eq!(p.audio_violations(), []);
    assert_eq!(p.audio_log().len(), 2);

    let mut no_table = platform(HeadlessConfig::default());
    no_table.audio(&[AudioCmd::PlaySe(SoundId(500))]);
    assert_eq!(no_table.audio_violations(), []);
}

fn list_with(color: u8) -> DrawList {
    DrawList {
        cmds: vec![DrawCmd::Clear(Color {
            r: color,
            g: 0,
            b: 0,
            a: 255,
        })],
    }
}

#[test]
fn record_policies() {
    let mut none = platform(HeadlessConfig {
        record: RecordPolicy::None,
        ..HeadlessConfig::default()
    });
    none.present(&list_with(1)).unwrap();
    assert!(none.last_frame().is_none());
    assert!(none.frames().is_empty());

    let mut last = platform(HeadlessConfig::default());
    assert!(last.last_frame().is_none());
    last.present(&list_with(1)).unwrap();
    last.present(&list_with(2)).unwrap();
    assert_eq!(last.last_frame().unwrap().cmds, list_with(2).cmds);
    assert!(last.frames().is_empty());
    assert_eq!(last.frame_index(), 2);

    let mut all = platform(HeadlessConfig {
        record: RecordPolicy::All { max_frames: 2 },
        ..HeadlessConfig::default()
    });
    for c in 1..=3 {
        all.present(&list_with(c)).unwrap();
    }
    assert_eq!(all.frames().len(), 2);
    assert_eq!(all.frames()[1].cmds, list_with(2).cmds);
    assert_eq!(all.last_frame().unwrap().cmds, list_with(3).cmds);
}

#[test]
fn logical_size_is_fixed() {
    let p = platform(HeadlessConfig::default());
    assert_eq!(p.logical_size(), (LOGICAL_WIDTH, LOGICAL_HEIGHT));
    assert_eq!(p.logical_size(), (640, 360));
    assert_eq!(p.stats(), Default::default());
}
