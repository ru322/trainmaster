mod common;

use common::{H, PAD, set};
use tm_input::{Action, ActionSet, Bindings, InputDevice, InputMapper, RepeatRule};
use tm_platform_api::{Key, MonoTime, PadId, PhysicalInput};

fn down(h: &mut H, k: Key) {
    h.feed(PhysicalInput::KeyDown(k));
}

fn up(h: &mut H, k: Key) {
    h.feed(PhysicalInput::KeyUp(k));
}

#[test]
fn action_set_basics() {
    let mut s = ActionSet::default();
    assert!(s.is_empty());
    for (i, a) in [
        Action::MoveUp,
        Action::MoveDown,
        Action::MoveLeft,
        Action::MoveRight,
        Action::Confirm,
        Action::Cancel,
        Action::BuyMax,
        Action::TabPrev,
        Action::TabNext,
        Action::BuildMode,
        Action::ToggleBend,
        Action::JumpRecommend,
        Action::Menu,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(a as usize, i);
        let mut one = ActionSet::default();
        one.insert(a);
        assert_eq!(one.0, 1 << i);
        assert!(one.contains(a));
        assert!(!s.contains(a));
        s.insert(a);
        assert!(s.contains(a));
    }
    assert!(!s.is_empty());
}

#[test]
fn quiet_frame_is_empty() {
    let mut h = H::new();
    let f = h.step(16);
    assert!(f.pressed.is_empty() && f.held.is_empty());
    assert!(f.released.is_empty() && f.repeated.is_empty());
    assert_eq!(f.last_device, InputDevice::Keyboard);
}

#[test]
fn pressed_only_in_first_frame_held_until_release() {
    let mut h = H::new();
    down(&mut h, Key::X);
    let f = h.step(16);
    assert_eq!(f.pressed, set(&[Action::Cancel]));
    let f = h.step(16);
    assert!(f.pressed.is_empty());
    assert_eq!(f.held, set(&[Action::Cancel]));
    up(&mut h, Key::X);
    let f = h.step(16);
    assert_eq!(f.released, set(&[Action::Cancel]));
    assert!(f.held.is_empty());
    let f = h.step(16);
    assert!(f.released.is_empty());
}

#[test]
fn several_sources_keep_the_action_held() {
    let mut h = H::new();
    down(&mut h, Key::Z);
    h.step(16);
    down(&mut h, Key::Enter);
    let f = h.step(16);
    assert!(f.pressed.is_empty());
    up(&mut h, Key::Enter);
    let f = h.step(16);
    assert!(f.released.is_empty());
    assert_eq!(f.held, set(&[Action::Confirm]));
    up(&mut h, Key::Z);
    let f = h.step(16);
    assert_eq!(f.released, set(&[Action::Confirm]));
    assert!(f.held.is_empty());
}

#[test]
fn key_and_pad_button_are_the_same_action() {
    let mut h = H::with_pad();
    down(&mut h, Key::Z);
    h.btn_down(tm_platform_api::PadButton::South);
    let f = h.frame();
    assert_eq!(f.pressed, set(&[Action::Confirm]));
    up(&mut h, Key::Z);
    assert!(h.frame().released.is_empty());
    h.btn_up(tm_platform_api::PadButton::South);
    assert_eq!(h.frame().released, set(&[Action::Confirm]));
}

#[test]
fn tap_within_one_frame_is_pressed_and_released_not_held() {
    let mut h = H::new();
    down(&mut h, Key::C);
    up(&mut h, Key::C);
    let f = h.frame();
    assert_eq!(f.pressed, set(&[Action::BuyMax]));
    assert_eq!(f.released, set(&[Action::BuyMax]));
    assert!(f.held.is_empty());
    let f = h.step(16);
    assert!(f.pressed.is_empty() && f.released.is_empty());
}

#[test]
fn tap_does_not_start_repeat() {
    let mut h = H::new();
    down(&mut h, Key::Up);
    up(&mut h, Key::Up);
    h.frame();
    for _ in 0..50 {
        assert!(h.step(20).repeated.is_empty());
    }
}

#[test]
fn duplicate_key_down_counts_once() {
    let mut h = H::new();
    down(&mut h, Key::Z);
    down(&mut h, Key::Z);
    let f = h.frame();
    assert_eq!(f.pressed, set(&[Action::Confirm]));
    up(&mut h, Key::Z);
    let f = h.frame();
    assert_eq!(f.released, set(&[Action::Confirm]));
    assert!(f.held.is_empty());
}

#[test]
fn stray_key_up_is_ignored() {
    let mut h = H::new();
    down(&mut h, Key::Z);
    h.frame();
    up(&mut h, Key::Enter);
    let f = h.step(16);
    assert!(f.released.is_empty());
    assert_eq!(f.held, set(&[Action::Confirm]));
    up(&mut h, Key::Z);
    up(&mut h, Key::Z);
    h.frame();
    down(&mut h, Key::Enter);
    assert_eq!(h.step(16).held, set(&[Action::Confirm]));
}

#[test]
fn quit_requested_is_ignored() {
    let mut h = H::new();
    h.feed(PhysicalInput::QuitRequested);
    let f = h.frame();
    assert_eq!(f, Default::default());
}

#[test]
fn move_repeats_after_300ms_then_every_100ms() {
    let mut h = H::new();
    down(&mut h, Key::Right);
    let mut hits = vec![];
    let f = h.frame();
    assert!(f.repeated.is_empty());
    for _ in 0..70 {
        let f = h.step(10);
        if f.repeated.contains(Action::MoveRight) {
            hits.push(h.now_ms);
            assert!(!f.pressed.contains(Action::MoveRight));
        }
    }
    assert_eq!(hits, vec![300, 400, 500, 600, 700]);
}

#[test]
fn confirm_repeats_every_200ms() {
    let mut h = H::new();
    down(&mut h, Key::Z);
    h.frame();
    let mut hits = vec![];
    for _ in 0..90 {
        if h.step(10).repeated.contains(Action::Confirm) {
            hits.push(h.now_ms);
        }
    }
    assert_eq!(hits, vec![200, 400, 600, 800]);
}

#[test]
fn repeat_waits_for_the_deadline() {
    let mut h = H::new();
    down(&mut h, Key::Left);
    h.frame();
    assert!(h.step(299).repeated.is_empty());
    assert!(h.step(1).repeated.contains(Action::MoveLeft));
    assert!(h.step(99).repeated.is_empty());
    assert!(h.step(1).repeated.contains(Action::MoveLeft));
}

#[test]
fn long_frame_gives_one_repeat_and_does_not_catch_up() {
    let mut h = H::new();
    down(&mut h, Key::Down);
    h.frame();
    let f = h.step(1000);
    assert_eq!(f.repeated, set(&[Action::MoveDown]));
    assert!(h.step(99).repeated.is_empty());
    assert!(h.step(1).repeated.contains(Action::MoveDown));
}

#[test]
fn non_repeating_actions_never_repeat() {
    let mut h = H::new();
    for k in [
        Key::X,
        Key::Escape,
        Key::C,
        Key::Q,
        Key::E,
        Key::B,
        Key::Tab,
        Key::R,
    ] {
        down(&mut h, k);
    }
    h.frame();
    for _ in 0..20 {
        let f = h.step(500);
        assert!(f.repeated.is_empty());
        assert_eq!(f.held.0.count_ones(), 8);
    }
}

#[test]
fn repeat_restarts_after_release_and_press() {
    let mut h = H::new();
    down(&mut h, Key::Up);
    h.frame();
    h.step(300);
    up(&mut h, Key::Up);
    h.step(10);
    down(&mut h, Key::Up);
    let f = h.step(10);
    assert!(f.pressed.contains(Action::MoveUp));
    assert!(h.step(299).repeated.is_empty());
    assert!(h.step(1).repeated.contains(Action::MoveUp));
}

#[test]
fn repeat_timer_clears_when_released() {
    let mut h = H::new();
    down(&mut h, Key::Up);
    h.frame();
    up(&mut h, Key::Up);
    h.step(10);
    h.step(1000);
    down(&mut h, Key::Up);
    h.step(10);
    assert!(h.step(100).repeated.is_empty());
}

#[test]
fn second_source_pressed_later_does_not_restart_repeat() {
    let mut h = H::new();
    down(&mut h, Key::Up);
    h.frame();
    h.step(200);
    down(&mut h, Key::W);
    h.step(10);
    assert!(h.step(90).repeated.contains(Action::MoveUp));
}

#[test]
fn custom_repeat_rule_is_used() {
    let mut b = Bindings::default_v1();
    b.repeats = vec![(
        Action::Cancel,
        RepeatRule {
            delay_ms: 50,
            interval_ms: 30,
        },
    )];
    let mut h = H::with(b);
    down(&mut h, Key::X);
    h.frame();
    assert!(h.step(49).repeated.is_empty());
    assert!(h.step(1).repeated.contains(Action::Cancel));
    assert!(h.step(30).repeated.contains(Action::Cancel));
    down(&mut h, Key::Right);
    h.frame();
    assert!(!h.step(1000).repeated.contains(Action::MoveRight));
}

#[test]
fn time_is_in_microseconds_from_the_argument() {
    let mut m = InputMapper::new(Bindings::default_v1());
    m.feed(&PhysicalInput::KeyDown(Key::Up));
    m.end_frame(MonoTime(5_000_000));
    assert!(m.end_frame(MonoTime(5_299_999)).repeated.is_empty());
    assert!(
        m.end_frame(MonoTime(5_300_000))
            .repeated
            .contains(Action::MoveUp)
    );
}

#[test]
fn last_device_follows_the_latest_press() {
    let mut h = H::with_pad();
    assert_eq!(h.frame().last_device, InputDevice::Keyboard);
    h.btn_down(tm_platform_api::PadButton::South);
    assert_eq!(h.frame().last_device, InputDevice::Gamepad);
    down(&mut h, Key::Z);
    assert_eq!(h.frame().last_device, InputDevice::Keyboard);
    h.btn_up(tm_platform_api::PadButton::South);
    assert_eq!(h.frame().last_device, InputDevice::Keyboard);
}

#[test]
fn connecting_a_pad_changes_nothing() {
    let mut h = H::new();
    h.feed(PhysicalInput::PadConnected(PAD));
    let f = h.frame();
    assert_eq!(f, Default::default());
}

#[test]
fn disconnect_releases_held_buttons() {
    let mut h = H::with_pad();
    h.btn_down(tm_platform_api::PadButton::South);
    h.btn_down(tm_platform_api::PadButton::DPadLeft);
    h.step(16);
    h.feed(PhysicalInput::PadDisconnected(PAD));
    let f = h.step(16);
    assert_eq!(f.released, set(&[Action::Confirm, Action::MoveLeft]));
    assert!(f.held.is_empty());
}

#[test]
fn disconnect_keeps_action_held_by_keyboard() {
    let mut h = H::with_pad();
    h.btn_down(tm_platform_api::PadButton::South);
    down(&mut h, Key::Z);
    h.step(16);
    h.feed(PhysicalInput::PadDisconnected(PAD));
    let f = h.step(16);
    assert!(f.released.is_empty());
    assert_eq!(f.held, set(&[Action::Confirm]));
}

#[test]
fn last_pad_removal_returns_to_keyboard() {
    let mut h = H::with_pad();
    h.feed(PhysicalInput::PadConnected(PadId(2)));
    h.btn_down(tm_platform_api::PadButton::South);
    assert_eq!(h.frame().last_device, InputDevice::Gamepad);
    h.feed(PhysicalInput::PadDisconnected(PAD));
    assert_eq!(h.frame().last_device, InputDevice::Gamepad);
    h.feed(PhysicalInput::PadDisconnected(PadId(2)));
    assert_eq!(h.frame().last_device, InputDevice::Keyboard);
}

#[test]
fn disconnect_of_unknown_pad_changes_nothing() {
    let mut h = H::with_pad();
    h.btn_down(tm_platform_api::PadButton::South);
    h.frame();
    h.feed(PhysicalInput::PadDisconnected(PadId(99)));
    let f = h.frame();
    assert_eq!(f.last_device, InputDevice::Gamepad);
    assert!(f.released.is_empty());
    assert_eq!(f.held, set(&[Action::Confirm]));
}

#[test]
fn duplicate_connect_keeps_pad_state() {
    let mut h = H::with_pad();
    h.btn_down(tm_platform_api::PadButton::South);
    h.frame();
    h.feed(PhysicalInput::PadConnected(PAD));
    h.feed(PhysicalInput::PadDisconnected(PAD));
    assert_eq!(h.frame().released, set(&[Action::Confirm]));
}

#[test]
fn reconnect_with_new_id_works_and_old_id_is_dead() {
    let mut h = H::with_pad();
    h.feed(PhysicalInput::PadDisconnected(PAD));
    h.btn_down(tm_platform_api::PadButton::South);
    assert!(h.frame().pressed.is_empty());
    h.feed(PhysicalInput::PadConnected(PadId(7)));
    h.feed(PhysicalInput::PadButtonDown {
        pad: PadId(7),
        button: tm_platform_api::PadButton::South,
    });
    assert_eq!(h.frame().pressed, set(&[Action::Confirm]));
}

#[test]
fn ninth_pad_is_ignored() {
    let mut h = H::new();
    for i in 1..=8 {
        h.feed(PhysicalInput::PadConnected(PadId(i)));
    }
    h.feed(PhysicalInput::PadConnected(PadId(9)));
    h.feed(PhysicalInput::PadButtonDown {
        pad: PadId(9),
        button: tm_platform_api::PadButton::South,
    });
    assert!(h.frame().pressed.is_empty());
    h.feed(PhysicalInput::PadButtonDown {
        pad: PadId(8),
        button: tm_platform_api::PadButton::South,
    });
    assert_eq!(h.frame().pressed, set(&[Action::Confirm]));
}

#[test]
fn pads_are_counted_per_source() {
    let mut h = H::with_pad();
    h.feed(PhysicalInput::PadConnected(PadId(2)));
    h.btn_down(tm_platform_api::PadButton::South);
    h.feed(PhysicalInput::PadButtonDown {
        pad: PadId(2),
        button: tm_platform_api::PadButton::South,
    });
    assert_eq!(h.frame().pressed, set(&[Action::Confirm]));
    h.btn_up(tm_platform_api::PadButton::South);
    assert!(h.frame().released.is_empty());
    h.feed(PhysicalInput::PadDisconnected(PadId(2)));
    assert_eq!(h.frame().released, set(&[Action::Confirm]));
}

#[test]
fn reset_swallows_held_keys_until_released() {
    let mut h = H::new();
    down(&mut h, Key::Z);
    h.step(16);
    h.m.reset();
    let f = h.step(16);
    assert!(f.pressed.is_empty() && f.held.is_empty() && f.released.is_empty());
    down(&mut h, Key::Z);
    assert!(h.step(16).held.is_empty());
    assert!(h.step(1000).repeated.is_empty());
    up(&mut h, Key::Z);
    let f = h.step(16);
    assert!(f.released.is_empty() && f.held.is_empty());
    down(&mut h, Key::Z);
    let f = h.step(16);
    assert_eq!(f.pressed, set(&[Action::Confirm]));
    assert_eq!(f.held, set(&[Action::Confirm]));
}

#[test]
fn reset_does_not_block_other_keys() {
    let mut h = H::new();
    down(&mut h, Key::Z);
    h.m.reset();
    down(&mut h, Key::X);
    let f = h.step(16);
    assert_eq!(f.held, set(&[Action::Cancel]));
}

#[test]
fn reset_drops_pending_edges_and_repeat_timers() {
    let mut h = H::new();
    down(&mut h, Key::Up);
    up(&mut h, Key::C);
    down(&mut h, Key::C);
    up(&mut h, Key::C);
    h.m.reset();
    let f = h.frame();
    assert!(f.pressed.is_empty() && f.released.is_empty());
    up(&mut h, Key::Up);
    h.step(10);
    down(&mut h, Key::Up);
    h.step(10);
    assert!(h.step(299).repeated.is_empty());
}

#[test]
fn reset_swallows_held_pad_buttons_and_keeps_pads_and_device() {
    let mut h = H::with_pad();
    h.btn_down(tm_platform_api::PadButton::South);
    h.step(16);
    h.m.reset();
    let f = h.step(16);
    assert!(f.held.is_empty() && f.pressed.is_empty());
    assert_eq!(f.last_device, InputDevice::Gamepad);
    h.btn_up(tm_platform_api::PadButton::South);
    assert!(h.step(16).released.is_empty());
    h.btn_down(tm_platform_api::PadButton::South);
    assert_eq!(h.step(16).pressed, set(&[Action::Confirm]));
}

#[test]
fn reset_then_disconnect_does_not_underflow() {
    let mut h = H::with_pad();
    h.btn_down(tm_platform_api::PadButton::South);
    h.m.reset();
    h.feed(PhysicalInput::PadDisconnected(PAD));
    let f = h.frame();
    assert!(f.released.is_empty());
    assert_eq!(f.last_device, InputDevice::Keyboard);
}

#[test]
fn identical_input_gives_identical_frames() {
    let run = || {
        let mut h = H::with_pad();
        let mut out = vec![];
        for i in 0..40u64 {
            match i % 7 {
                0 => down(&mut h, Key::Up),
                3 => up(&mut h, Key::Up),
                5 => h.btn_down(tm_platform_api::PadButton::South),
                6 => h.btn_up(tm_platform_api::PadButton::South),
                _ => {}
            }
            out.push(h.step(37));
        }
        out
    };
    assert_eq!(run(), run());
}

#[test]
fn reset_ignores_duplicate_down_of_a_held_pad_button() {
    let mut h = H::with_pad();
    h.btn_down(tm_platform_api::PadButton::South);
    h.m.reset();
    h.btn_down(tm_platform_api::PadButton::South);
    let f = h.frame();
    assert!(f.pressed.is_empty() && f.held.is_empty());
}

#[test]
fn opposite_directions_are_both_reported_held() {
    let mut h = H::new();
    down(&mut h, Key::Left);
    down(&mut h, Key::Right);
    let f = h.frame();
    assert_eq!(f.pressed, set(&[Action::MoveLeft, Action::MoveRight]));
    up(&mut h, Key::Left);
    let f = h.step(16);
    assert_eq!(f.held, set(&[Action::MoveRight]));
    assert_eq!(f.released, set(&[Action::MoveLeft]));
    assert!(h.step(300).repeated.contains(Action::MoveRight));
}
