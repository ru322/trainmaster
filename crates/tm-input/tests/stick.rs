mod common;

use common::{H, PAD, set};
use tm_input::{Action, Bindings, InputDevice};
use tm_platform_api::{PadAxis, PhysicalInput};

fn held(h: &mut H) -> tm_input::ActionSet {
    h.frame().held
}

#[test]
fn threshold_presses_and_below_does_not() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 16383);
    assert!(held(&mut h).is_empty());
    h.axis(PadAxis::LeftX, 16384);
    let f = h.frame();
    assert_eq!(f.pressed, set(&[Action::MoveRight]));
    assert_eq!(f.held, set(&[Action::MoveRight]));
    assert_eq!(f.last_device, InputDevice::Gamepad);
}

#[test]
fn four_directions_with_sdl_y_up_negative() {
    for (axis, v, a) in [
        (PadAxis::LeftY, -20000, Action::MoveUp),
        (PadAxis::LeftY, 20000, Action::MoveDown),
        (PadAxis::LeftX, -20000, Action::MoveLeft),
        (PadAxis::LeftX, 20000, Action::MoveRight),
    ] {
        let mut h = H::with_pad();
        h.axis(axis, v);
        assert_eq!(h.frame().held, set(&[a]), "{axis:?} {v}");
    }
}

#[test]
fn hysteresis_between_release_and_threshold() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 20000);
    h.frame();
    h.axis(PadAxis::LeftX, 12288);
    assert_eq!(held(&mut h), set(&[Action::MoveRight]));
    h.axis(PadAxis::LeftX, 12287);
    let f = h.frame();
    assert!(f.held.is_empty());
    assert_eq!(f.released, set(&[Action::MoveRight]));
    h.axis(PadAxis::LeftX, 14000);
    assert!(h.frame().pressed.is_empty());
    h.axis(PadAxis::LeftX, 16384);
    assert_eq!(h.frame().pressed, set(&[Action::MoveRight]));
}

#[test]
fn jitter_around_threshold_does_not_flicker() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 16384);
    h.frame();
    for v in [16000, 16500, 15000, 16384, 13000, 16383] {
        h.axis(PadAxis::LeftX, v);
        let f = h.frame();
        assert!(f.pressed.is_empty() && f.released.is_empty(), "{v}");
        assert_eq!(f.held, set(&[Action::MoveRight]));
    }
}

#[test]
fn diagonal_picks_larger_axis_and_tie_goes_horizontal() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 20000);
    h.axis(PadAxis::LeftY, 30000);
    let f = h.frame();
    assert_eq!(f.held, set(&[Action::MoveDown]));

    let mut h = H::with_pad();
    h.axis(PadAxis::LeftY, -30000);
    h.axis(PadAxis::LeftX, -20000);
    assert_eq!(held(&mut h), set(&[Action::MoveUp]));

    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 20000);
    h.axis(PadAxis::LeftY, 20000);
    h.frame();
    h.axis(PadAxis::LeftX, -20000);
    assert_eq!(held(&mut h), set(&[Action::MoveLeft]));
}

#[test]
fn diagonal_does_not_flicker_between_axes() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 20000);
    h.frame();
    h.axis(PadAxis::LeftY, 19000);
    h.axis(PadAxis::LeftY, 19900);
    let f = h.frame();
    assert_eq!(f.held, set(&[Action::MoveRight]));
    assert!(f.pressed.is_empty());
}

#[test]
fn switching_to_the_other_axis_when_it_dominates() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 20000);
    h.frame();
    h.axis(PadAxis::LeftY, 25000);
    let f = h.frame();
    assert_eq!(f.held, set(&[Action::MoveDown]));
    assert_eq!(f.pressed, set(&[Action::MoveDown]));
    assert_eq!(f.released, set(&[Action::MoveRight]));
}

#[test]
fn other_axis_below_threshold_never_switches() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 14000);
    h.axis(PadAxis::LeftX, 16384);
    h.frame();
    h.axis(PadAxis::LeftY, 16383);
    assert_eq!(held(&mut h), set(&[Action::MoveRight]));
    h.axis(PadAxis::LeftX, 13000);
    h.axis(PadAxis::LeftY, 16383);
    assert_eq!(held(&mut h), set(&[Action::MoveRight]));
}

#[test]
fn reversing_direction_swaps_the_actions() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 30000);
    h.frame();
    h.axis(PadAxis::LeftX, -30000);
    let f = h.frame();
    assert_eq!(f.released, set(&[Action::MoveRight]));
    assert_eq!(f.pressed, set(&[Action::MoveLeft]));
    assert_eq!(f.held, set(&[Action::MoveLeft]));
}

#[test]
fn extreme_values_do_not_overflow() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, i16::MIN);
    assert_eq!(held(&mut h), set(&[Action::MoveLeft]));
    h.axis(PadAxis::LeftX, i16::MAX);
    assert_eq!(held(&mut h), set(&[Action::MoveRight]));
    h.axis(PadAxis::LeftX, 0);
    assert!(held(&mut h).is_empty());
}

#[test]
fn right_stick_and_triggers_are_ignored() {
    let mut h = H::with_pad();
    for axis in [
        PadAxis::RightX,
        PadAxis::RightY,
        PadAxis::TriggerLeft,
        PadAxis::TriggerRight,
    ] {
        h.axis(axis, i16::MAX);
        h.axis(axis, i16::MIN);
    }
    let f = h.frame();
    assert!(f.held.is_empty() && f.pressed.is_empty());
    assert_eq!(f.last_device, InputDevice::Keyboard);
}

#[test]
fn stick_and_dpad_share_the_action() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 20000);
    h.btn_down(tm_platform_api::PadButton::DPadRight);
    assert_eq!(h.frame().pressed, set(&[Action::MoveRight]));
    h.axis(PadAxis::LeftX, 0);
    let f = h.frame();
    assert!(f.released.is_empty());
    assert_eq!(f.held, set(&[Action::MoveRight]));
}

#[test]
fn stick_direction_repeats_like_buttons() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftY, -32768);
    h.frame();
    assert!(h.step(299).repeated.is_empty());
    assert_eq!(h.step(1).repeated, set(&[Action::MoveUp]));
}

#[test]
fn drift_inside_deadzone_keeps_device_on_keyboard() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 9000);
    h.axis(PadAxis::LeftY, -9000);
    let f = h.frame();
    assert_eq!(f.last_device, InputDevice::Keyboard);
    assert!(f.held.is_empty());
}

#[test]
fn disconnect_releases_stick_direction() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 30000);
    h.frame();
    h.feed(PhysicalInput::PadDisconnected(PAD));
    let f = h.frame();
    assert_eq!(f.released, set(&[Action::MoveRight]));
    assert!(f.held.is_empty());
    assert_eq!(f.last_device, InputDevice::Keyboard);
}

#[test]
fn two_pads_sticks_are_independent() {
    let mut h = H::with_pad();
    h.feed(PhysicalInput::PadConnected(tm_platform_api::PadId(2)));
    h.axis(PadAxis::LeftX, 30000);
    h.feed(PhysicalInput::PadAxis {
        pad: tm_platform_api::PadId(2),
        axis: PadAxis::LeftX,
        value: 30000,
    });
    assert_eq!(h.frame().pressed, set(&[Action::MoveRight]));
    h.axis(PadAxis::LeftX, 0);
    assert!(h.frame().released.is_empty());
    h.feed(PhysicalInput::PadAxis {
        pad: tm_platform_api::PadId(2),
        axis: PadAxis::LeftX,
        value: 0,
    });
    assert_eq!(h.frame().released, set(&[Action::MoveRight]));
}

#[test]
fn axis_from_unknown_pad_is_ignored() {
    let mut h = H::new();
    h.axis(PadAxis::LeftX, 30000);
    assert!(h.frame().held.is_empty());
}

#[test]
fn reset_swallows_a_held_stick_until_neutral() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 30000);
    h.step(16);
    h.m.reset();
    let f = h.step(16);
    assert!(f.held.is_empty() && f.pressed.is_empty());
    assert_eq!(f.last_device, InputDevice::Gamepad);
    h.axis(PadAxis::LeftX, 31000);
    h.axis(PadAxis::LeftX, 13000);
    h.axis(PadAxis::LeftY, -30000);
    let f = h.step(16);
    assert!(f.held.is_empty() && f.released.is_empty());
    h.axis(PadAxis::LeftY, 0);
    h.axis(PadAxis::LeftX, 12287);
    assert!(h.step(16).held.is_empty());
    h.axis(PadAxis::LeftX, 20000);
    assert_eq!(h.step(16).pressed, set(&[Action::MoveRight]));
}

#[test]
fn reset_with_neutral_stick_does_not_suppress() {
    let mut h = H::with_pad();
    h.m.reset();
    h.axis(PadAxis::LeftX, 20000);
    assert_eq!(h.frame().pressed, set(&[Action::MoveRight]));
}

#[test]
fn custom_thresholds_are_used() {
    let mut b = Bindings::default_v1();
    b.stick_threshold = 8000;
    b.stick_release = 4000;
    let mut h = H::with(b);
    h.feed(PhysicalInput::PadConnected(PAD));
    h.axis(PadAxis::LeftX, 7999);
    assert!(held(&mut h).is_empty());
    h.axis(PadAxis::LeftX, 8000);
    assert_eq!(held(&mut h), set(&[Action::MoveRight]));
    h.axis(PadAxis::LeftX, 4000);
    assert_eq!(held(&mut h), set(&[Action::MoveRight]));
    h.axis(PadAxis::LeftX, 3999);
    assert!(held(&mut h).is_empty());
}

#[test]
fn other_axis_exactly_at_threshold_switches_when_it_dominates() {
    let mut h = H::with_pad();
    h.axis(PadAxis::LeftX, 16384);
    h.axis(PadAxis::LeftX, 13000);
    h.frame();
    h.axis(PadAxis::LeftY, 16384);
    assert_eq!(held(&mut h), set(&[Action::MoveDown]));
}

const DIRS: [(Action, PadAxis, i16); 4] = [
    (Action::MoveUp, PadAxis::LeftY, -1),
    (Action::MoveDown, PadAxis::LeftY, 1),
    (Action::MoveLeft, PadAxis::LeftX, -1),
    (Action::MoveRight, PadAxis::LeftX, 1),
];

fn push(h: &mut H, d: (Action, PadAxis, i16), magnitude: i16) {
    h.axis(d.1, d.2 * magnitude);
}

fn others(d: (Action, PadAxis, i16)) -> Vec<(Action, PadAxis, i16)> {
    DIRS.into_iter().filter(|o| o.1 != d.1).collect()
}

#[test]
fn every_direction_has_hysteresis() {
    for d in DIRS {
        let mut h = H::with_pad();
        push(&mut h, d, 16383);
        assert!(held(&mut h).is_empty(), "{d:?}");
        push(&mut h, d, 16384);
        let f = h.frame();
        assert_eq!(f.pressed, set(&[d.0]), "{d:?}");
        push(&mut h, d, 12288);
        let f = h.frame();
        assert_eq!(f.held, set(&[d.0]), "{d:?}");
        assert!(f.released.is_empty(), "{d:?}");
        push(&mut h, d, 12287);
        let f = h.frame();
        assert_eq!(f.released, set(&[d.0]), "{d:?}");
        assert!(f.held.is_empty(), "{d:?}");
        push(&mut h, d, 14000);
        let f = h.frame();
        assert!(f.pressed.is_empty() && f.held.is_empty(), "{d:?}");
        push(&mut h, d, 16384);
        assert_eq!(h.frame().pressed, set(&[d.0]), "{d:?}");
    }
}

#[test]
fn every_direction_switches_to_a_dominant_other_axis() {
    for d in DIRS {
        for o in others(d) {
            let mut h = H::with_pad();
            push(&mut h, d, 20000);
            h.frame();
            push(&mut h, o, 25000);
            let f = h.frame();
            assert_eq!(f.held, set(&[o.0]), "{d:?} -> {o:?}");
            assert_eq!(f.pressed, set(&[o.0]), "{d:?} -> {o:?}");
            assert_eq!(f.released, set(&[d.0]), "{d:?} -> {o:?}");
        }
    }
}

#[test]
fn every_direction_keeps_when_other_axis_is_not_larger() {
    for d in DIRS {
        for o in others(d) {
            let mut h = H::with_pad();
            push(&mut h, d, 20000);
            h.frame();
            push(&mut h, o, 19999);
            let f = h.frame();
            assert_eq!(f.held, set(&[d.0]), "{d:?} / {o:?}");
            assert!(
                f.pressed.is_empty() && f.released.is_empty(),
                "{d:?} / {o:?}"
            );
            push(&mut h, o, 20000);
            let f = h.frame();
            assert_eq!(f.held, set(&[d.0]), "tie {d:?} / {o:?}");
            assert!(
                f.pressed.is_empty() && f.released.is_empty(),
                "tie {d:?} / {o:?}"
            );
        }
    }
}

#[test]
fn every_direction_goes_neutral_then_picks_the_other_axis() {
    for d in DIRS {
        for o in others(d) {
            let mut h = H::with_pad();
            push(&mut h, d, 20000);
            push(&mut h, o, 18000);
            h.frame();
            push(&mut h, d, 12000);
            let f = h.frame();
            assert_eq!(f.held, set(&[o.0]), "{d:?} -> {o:?}");
            assert_eq!(f.released, set(&[d.0]), "{d:?} -> {o:?}");
        }
    }
}

#[test]
fn reset_suppression_ends_exactly_below_release() {
    for d in DIRS {
        let mut h = H::with_pad();
        push(&mut h, d, 30000);
        h.step(16);
        h.m.reset();
        h.frame();
        push(&mut h, d, 12288);
        push(&mut h, d, 16384);
        let f = h.frame();
        assert!(f.held.is_empty() && f.pressed.is_empty(), "{d:?}");
        push(&mut h, d, 12287);
        h.frame();
        push(&mut h, d, 16384);
        assert_eq!(h.frame().pressed, set(&[d.0]), "{d:?}");
    }
}
