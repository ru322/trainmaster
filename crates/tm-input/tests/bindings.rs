mod common;

use common::{H, PAD, set};
use tm_input::{Action, Bindings, InputDevice, RepeatRule};
use tm_platform_api::{Key, PadButton, PhysicalInput};

const KEYS: [(Key, Action); 20] = [
    (Key::Up, Action::MoveUp),
    (Key::W, Action::MoveUp),
    (Key::Down, Action::MoveDown),
    (Key::S, Action::MoveDown),
    (Key::Left, Action::MoveLeft),
    (Key::A, Action::MoveLeft),
    (Key::Right, Action::MoveRight),
    (Key::D, Action::MoveRight),
    (Key::Z, Action::Confirm),
    (Key::Enter, Action::Confirm),
    (Key::Space, Action::Confirm),
    (Key::X, Action::Cancel),
    (Key::Backspace, Action::Cancel),
    (Key::C, Action::BuyMax),
    (Key::Q, Action::TabPrev),
    (Key::E, Action::TabNext),
    (Key::B, Action::BuildMode),
    (Key::Tab, Action::ToggleBend),
    (Key::R, Action::JumpRecommend),
    (Key::Escape, Action::Menu),
];

const BUTTONS: [(PadButton, Action); 13] = [
    (PadButton::DPadUp, Action::MoveUp),
    (PadButton::DPadDown, Action::MoveDown),
    (PadButton::DPadLeft, Action::MoveLeft),
    (PadButton::DPadRight, Action::MoveRight),
    (PadButton::South, Action::Confirm),
    (PadButton::East, Action::Cancel),
    (PadButton::North, Action::BuyMax),
    (PadButton::LeftShoulder, Action::TabPrev),
    (PadButton::RightShoulder, Action::TabNext),
    (PadButton::West, Action::BuildMode),
    (PadButton::RightStick, Action::ToggleBend),
    (PadButton::Back, Action::JumpRecommend),
    (PadButton::Start, Action::Menu),
];

#[test]
fn every_default_key_maps_to_its_action() {
    for (k, a) in KEYS {
        let mut h = H::new();
        h.feed(PhysicalInput::KeyDown(k));
        let f = h.frame();
        assert_eq!(f.pressed, set(&[a]), "{k:?}");
        assert_eq!(f.held, set(&[a]), "{k:?}");
        h.feed(PhysicalInput::KeyUp(k));
        let f = h.frame();
        assert_eq!(f.released, set(&[a]), "{k:?}");
        assert!(f.held.is_empty(), "{k:?}");
    }
}

#[test]
fn every_default_button_maps_to_its_action() {
    for (b, a) in BUTTONS {
        let mut h = H::with_pad();
        h.btn_down(b);
        let f = h.frame();
        assert_eq!(f.pressed, set(&[a]), "{b:?}");
        assert_eq!(f.held, set(&[a]), "{b:?}");
        assert_eq!(f.last_device, InputDevice::Gamepad);
        h.btn_up(b);
        let f = h.frame();
        assert_eq!(f.released, set(&[a]), "{b:?}");
    }
}

#[test]
fn unlisted_button_does_nothing() {
    let mut h = H::with_pad();
    h.btn_down(PadButton::LeftStick);
    let f = h.frame();
    assert!(f.pressed.is_empty() && f.held.is_empty());
}

#[test]
fn default_table_has_exactly_the_documented_entries() {
    let b = Bindings::default_v1();
    assert_eq!(b.keys, KEYS.to_vec());
    assert_eq!(b.pad_buttons, BUTTONS.to_vec());
    assert_eq!(b.stick_threshold, 16384);
    assert_eq!(b.stick_release, 12288);
}

#[test]
fn escape_and_start_are_menu_and_never_cancel() {
    let mut h = H::with_pad();
    h.feed(PhysicalInput::KeyDown(Key::Escape));
    h.btn_down(PadButton::Start);
    let f = h.frame();
    assert_eq!(f.pressed, set(&[Action::Menu]));
    assert!(!f.held.contains(Action::Cancel));
    let b = Bindings::default_v1();
    for (k, a) in &b.keys {
        assert!(*k != Key::Escape || *a == Action::Menu);
    }
    for (p, a) in &b.pad_buttons {
        assert!(*p != PadButton::Start || *a == Action::Menu);
    }
}

#[test]
fn default_repeat_rules() {
    let b = Bindings::default_v1();
    let mv = RepeatRule {
        delay_ms: 300,
        interval_ms: 100,
    };
    let cf = RepeatRule {
        delay_ms: 200,
        interval_ms: 200,
    };
    let mut want = vec![
        (Action::MoveUp, mv),
        (Action::MoveDown, mv),
        (Action::MoveLeft, mv),
        (Action::MoveRight, mv),
        (Action::Confirm, cf),
    ];
    let mut got = b.repeats.clone();
    want.sort_by_key(|(a, _)| *a);
    got.sort_by_key(|(a, _)| *a);
    assert_eq!(got, want);
}

#[test]
fn first_binding_wins_for_duplicates() {
    let b = Bindings {
        keys: vec![(Key::Z, Action::Confirm), (Key::Z, Action::Cancel)],
        pad_buttons: vec![
            (PadButton::South, Action::BuyMax),
            (PadButton::South, Action::Menu),
        ],
        stick_threshold: 16384,
        stick_release: 12288,
        repeats: vec![],
    };
    let mut h = H::with(b);
    h.feed(PhysicalInput::PadConnected(PAD));
    h.feed(PhysicalInput::KeyDown(Key::Z));
    h.btn_down(PadButton::South);
    let f = h.frame();
    assert_eq!(f.pressed, set(&[Action::Confirm, Action::BuyMax]));
}

#[test]
fn empty_bindings_ignore_everything_but_track_device() {
    let b = Bindings {
        keys: vec![],
        pad_buttons: vec![],
        stick_threshold: 16384,
        stick_release: 12288,
        repeats: vec![],
    };
    let mut h = H::with(b);
    h.feed(PhysicalInput::KeyDown(Key::Z));
    let f = h.frame();
    assert!(f.pressed.is_empty() && f.held.is_empty());
    h.feed(PhysicalInput::KeyUp(Key::Z));
    assert!(h.frame().released.is_empty());
}
