//! SDL の `Event` から `PhysicalInput` への変換と、ゲームパッドの抜き差し [14 §3.6]。

use std::collections::BTreeMap;

use sdl2::controller::{Axis, Button};
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Scancode;
use tm_platform_api::{Key, PadAxis, PadButton, PadId, PhysicalInput};

/// ゲームパッドを開く・閉じる操作。テストでは偽物に差し替える。
pub(crate) trait PadBackend {
    /// デバイスの番号のパッドを開き、インスタンス ID を返す。開かなかったら `None`。
    fn open(&mut self, device_index: u32) -> Option<u32>;
    /// インスタンス ID のパッドを閉じる。
    fn close(&mut self, instance_id: u32);
    /// ジョイスティックが差された(マッピングのないものをログに出す)。
    fn joystick_added(&mut self, device_index: u32);
}

/// 押されているキーとボタンを覚えて、変換する。
#[derive(Default)]
pub(crate) struct InputState {
    keys: Vec<Key>,
    pads: BTreeMap<u32, Vec<PadButton>>,
}

impl InputState {
    pub(crate) fn handle(
        &mut self,
        event: &Event,
        backend: &mut impl PadBackend,
        out: &mut Vec<PhysicalInput>,
    ) {
        match *event {
            Event::Quit { .. } => out.push(PhysicalInput::QuitRequested),
            Event::KeyDown {
                scancode: Some(sc),
                repeat: false,
                ..
            } => {
                if let Some(key) = key_from_scancode(sc)
                    && !self.keys.contains(&key)
                {
                    self.keys.push(key);
                    out.push(PhysicalInput::KeyDown(key));
                }
            }
            Event::KeyUp {
                scancode: Some(sc), ..
            } => {
                if let Some(key) = key_from_scancode(sc)
                    && let Some(i) = self.keys.iter().position(|k| *k == key)
                {
                    self.keys.remove(i);
                    out.push(PhysicalInput::KeyUp(key));
                }
            }
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => self.release_all(out),
            Event::ControllerDeviceAdded { which, .. } => self.pad_added(which, backend, out),
            Event::JoyDeviceAdded { which, .. } => backend.joystick_added(which),
            Event::ControllerDeviceRemoved { which, .. } => {
                if let Some(buttons) = self.pads.remove(&which) {
                    let pad = PadId(which);
                    for button in buttons {
                        out.push(PhysicalInput::PadButtonUp { pad, button });
                    }
                    backend.close(which);
                    out.push(PhysicalInput::PadDisconnected(pad));
                }
            }
            Event::ControllerButtonDown { which, button, .. } => {
                if let (Some(pressed), Some(button)) =
                    (self.pads.get_mut(&which), pad_button(button))
                    && !pressed.contains(&button)
                {
                    pressed.push(button);
                    out.push(PhysicalInput::PadButtonDown {
                        pad: PadId(which),
                        button,
                    });
                }
            }
            Event::ControllerButtonUp { which, button, .. } => {
                if let (Some(pressed), Some(button)) =
                    (self.pads.get_mut(&which), pad_button(button))
                    && let Some(i) = pressed.iter().position(|b| *b == button)
                {
                    pressed.remove(i);
                    out.push(PhysicalInput::PadButtonUp {
                        pad: PadId(which),
                        button,
                    });
                }
            }
            Event::ControllerAxisMotion {
                which, axis, value, ..
            } if self.pads.contains_key(&which) => out.push(PhysicalInput::PadAxis {
                pad: PadId(which),
                axis: pad_axis(axis),
                value,
            }),
            _ => {}
        }
    }

    /// `device_index`(デバイスの番号)のパッドを開く。同じインスタンス ID を開いていれば何もしない。
    pub(crate) fn pad_added(
        &mut self,
        device_index: u32,
        backend: &mut impl PadBackend,
        out: &mut Vec<PhysicalInput>,
    ) {
        if let Some(id) = backend.open(device_index)
            && !self.pads.contains_key(&id)
        {
            self.pads.insert(id, Vec::new());
            out.push(PhysicalInput::PadConnected(PadId(id)));
        }
    }

    /// 押されているキーとボタンをすべて離す。
    pub(crate) fn release_all(&mut self, out: &mut Vec<PhysicalInput>) {
        out.extend(self.keys.drain(..).map(PhysicalInput::KeyUp));
        for (&id, pressed) in &mut self.pads {
            out.extend(pressed.drain(..).map(|button| PhysicalInput::PadButtonUp {
                pad: PadId(id),
                button,
            }));
        }
    }
}

pub(crate) fn key_from_scancode(sc: Scancode) -> Option<Key> {
    Some(match sc {
        Scancode::Up => Key::Up,
        Scancode::Down => Key::Down,
        Scancode::Left => Key::Left,
        Scancode::Right => Key::Right,
        Scancode::Return | Scancode::KpEnter => Key::Enter,
        Scancode::Escape => Key::Escape,
        Scancode::Backspace => Key::Backspace,
        Scancode::Space => Key::Space,
        Scancode::Tab => Key::Tab,
        Scancode::A => Key::A,
        Scancode::B => Key::B,
        Scancode::C => Key::C,
        Scancode::D => Key::D,
        Scancode::E => Key::E,
        Scancode::Q => Key::Q,
        Scancode::R => Key::R,
        Scancode::S => Key::S,
        Scancode::W => Key::W,
        Scancode::X => Key::X,
        Scancode::Z => Key::Z,
        _ => return None,
    })
}

fn pad_button(button: Button) -> Option<PadButton> {
    Some(match button {
        Button::A => PadButton::South,
        Button::B => PadButton::East,
        Button::X => PadButton::West,
        Button::Y => PadButton::North,
        Button::DPadUp => PadButton::DPadUp,
        Button::DPadDown => PadButton::DPadDown,
        Button::DPadLeft => PadButton::DPadLeft,
        Button::DPadRight => PadButton::DPadRight,
        Button::Start => PadButton::Start,
        Button::Back => PadButton::Back,
        Button::LeftShoulder => PadButton::LeftShoulder,
        Button::RightShoulder => PadButton::RightShoulder,
        Button::LeftStick => PadButton::LeftStick,
        Button::RightStick => PadButton::RightStick,
        Button::Guide
        | Button::Misc1
        | Button::Paddle1
        | Button::Paddle2
        | Button::Paddle3
        | Button::Paddle4
        | Button::Touchpad => return None,
    })
}

fn pad_axis(axis: Axis) -> PadAxis {
    match axis {
        Axis::LeftX => PadAxis::LeftX,
        Axis::LeftY => PadAxis::LeftY,
        Axis::RightX => PadAxis::RightX,
        Axis::RightY => PadAxis::RightY,
        Axis::TriggerLeft => PadAxis::TriggerLeft,
        Axis::TriggerRight => PadAxis::TriggerRight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdl2::keyboard::Mod;

    #[derive(Default)]
    struct FakePads {
        devices: Vec<Option<u32>>,
        opened: Vec<u32>,
        closed: Vec<u32>,
        joysticks: Vec<u32>,
    }

    impl PadBackend for FakePads {
        fn open(&mut self, device_index: u32) -> Option<u32> {
            let id = self.devices.get(device_index as usize).copied().flatten()?;
            self.opened.push(id);
            Some(id)
        }
        fn close(&mut self, instance_id: u32) {
            self.closed.push(instance_id);
        }
        fn joystick_added(&mut self, device_index: u32) {
            self.joysticks.push(device_index);
        }
    }

    fn key_down(sc: Scancode, repeat: bool) -> Event {
        Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            keycode: None,
            scancode: Some(sc),
            keymod: Mod::NOMOD,
            repeat,
        }
    }

    fn key_up(sc: Scancode) -> Event {
        Event::KeyUp {
            timestamp: 0,
            window_id: 1,
            keycode: None,
            scancode: Some(sc),
            keymod: Mod::NOMOD,
            repeat: false,
        }
    }

    fn run(state: &mut InputState, pads: &mut FakePads, events: &[Event]) -> Vec<PhysicalInput> {
        let mut out = Vec::new();
        for e in events {
            state.handle(e, pads, &mut out);
        }
        out
    }

    fn button(ev_down: bool, which: u32, button: Button) -> Event {
        if ev_down {
            Event::ControllerButtonDown {
                timestamp: 0,
                which,
                button,
            }
        } else {
            Event::ControllerButtonUp {
                timestamp: 0,
                which,
                button,
            }
        }
    }

    #[test]
    fn scancode_table() {
        let table = [
            (Scancode::Up, Key::Up),
            (Scancode::Down, Key::Down),
            (Scancode::Left, Key::Left),
            (Scancode::Right, Key::Right),
            (Scancode::Return, Key::Enter),
            (Scancode::KpEnter, Key::Enter),
            (Scancode::Escape, Key::Escape),
            (Scancode::Backspace, Key::Backspace),
            (Scancode::Space, Key::Space),
            (Scancode::Tab, Key::Tab),
            (Scancode::A, Key::A),
            (Scancode::B, Key::B),
            (Scancode::C, Key::C),
            (Scancode::D, Key::D),
            (Scancode::E, Key::E),
            (Scancode::Q, Key::Q),
            (Scancode::R, Key::R),
            (Scancode::S, Key::S),
            (Scancode::W, Key::W),
            (Scancode::X, Key::X),
            (Scancode::Z, Key::Z),
        ];
        for (sc, key) in table {
            assert_eq!(key_from_scancode(sc), Some(key), "{sc:?}");
        }
        for sc in [Scancode::F, Scancode::F11, Scancode::F12, Scancode::LShift] {
            assert_eq!(key_from_scancode(sc), None, "{sc:?}");
        }
    }

    #[test]
    fn repeat_and_double_down_are_dropped() {
        let mut s = InputState::default();
        let mut p = FakePads::default();
        let out = run(
            &mut s,
            &mut p,
            &[
                key_down(Scancode::W, false),
                key_down(Scancode::W, true),
                key_down(Scancode::W, false),
                key_up(Scancode::W),
                key_up(Scancode::W),
                key_down(Scancode::F, false),
                key_up(Scancode::F),
            ],
        );
        assert_eq!(
            out,
            [PhysicalInput::KeyDown(Key::W), PhysicalInput::KeyUp(Key::W)]
        );
    }

    #[test]
    fn repeat_without_prior_down_is_dropped() {
        let out = run(
            &mut InputState::default(),
            &mut FakePads::default(),
            &[key_down(Scancode::Up, true), key_up(Scancode::Up)],
        );
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn quit_event() {
        let out = run(
            &mut InputState::default(),
            &mut FakePads::default(),
            &[Event::Quit { timestamp: 0 }],
        );
        assert_eq!(out, [PhysicalInput::QuitRequested]);
    }

    #[test]
    fn added_uses_device_index_and_others_use_instance_id() {
        let mut s = InputState::default();
        let mut p = FakePads {
            devices: vec![Some(7), Some(9)],
            ..Default::default()
        };
        let out = run(
            &mut s,
            &mut p,
            &[
                Event::ControllerDeviceAdded {
                    timestamp: 0,
                    which: 1,
                },
                button(true, 9, Button::A),
                button(true, 1, Button::A),
                Event::ControllerAxisMotion {
                    timestamp: 0,
                    which: 9,
                    axis: Axis::LeftX,
                    value: -32768,
                },
                button(false, 9, Button::A),
            ],
        );
        assert_eq!(p.opened, [9]);
        let pad = PadId(9);
        assert_eq!(
            out,
            [
                PhysicalInput::PadConnected(pad),
                PhysicalInput::PadButtonDown {
                    pad,
                    button: PadButton::South
                },
                PhysicalInput::PadAxis {
                    pad,
                    axis: PadAxis::LeftX,
                    value: -32768
                },
                PhysicalInput::PadButtonUp {
                    pad,
                    button: PadButton::South
                },
            ]
        );
    }

    #[test]
    fn added_twice_connects_once_and_unmapped_is_ignored() {
        let mut s = InputState::default();
        let mut p = FakePads {
            devices: vec![Some(3), None],
            ..Default::default()
        };
        let mut out = Vec::new();
        s.pad_added(0, &mut p, &mut out);
        let events = [
            Event::ControllerDeviceAdded {
                timestamp: 0,
                which: 0,
            },
            Event::ControllerDeviceAdded {
                timestamp: 0,
                which: 1,
            },
        ];
        out.extend(run(&mut s, &mut p, &events));
        assert_eq!(out, [PhysicalInput::PadConnected(PadId(3))]);
    }

    #[test]
    fn removed_releases_buttons_first() {
        let mut s = InputState::default();
        let mut p = FakePads {
            devices: vec![Some(4)],
            ..Default::default()
        };
        let pad = PadId(4);
        let out = run(
            &mut s,
            &mut p,
            &[
                Event::ControllerDeviceAdded {
                    timestamp: 0,
                    which: 0,
                },
                button(true, 4, Button::B),
                button(true, 4, Button::DPadLeft),
                button(true, 4, Button::Guide),
                Event::ControllerDeviceRemoved {
                    timestamp: 0,
                    which: 4,
                },
                button(false, 4, Button::B),
                Event::ControllerDeviceRemoved {
                    timestamp: 0,
                    which: 4,
                },
            ],
        );
        assert_eq!(p.closed, [4]);
        assert_eq!(
            out,
            [
                PhysicalInput::PadConnected(pad),
                PhysicalInput::PadButtonDown {
                    pad,
                    button: PadButton::East
                },
                PhysicalInput::PadButtonDown {
                    pad,
                    button: PadButton::DPadLeft
                },
                PhysicalInput::PadButtonUp {
                    pad,
                    button: PadButton::East
                },
                PhysicalInput::PadButtonUp {
                    pad,
                    button: PadButton::DPadLeft
                },
                PhysicalInput::PadDisconnected(pad),
            ]
        );
    }

    #[test]
    fn focus_lost_releases_everything() {
        let mut s = InputState::default();
        let mut p = FakePads {
            devices: vec![Some(2)],
            ..Default::default()
        };
        let out = run(
            &mut s,
            &mut p,
            &[
                key_down(Scancode::Left, false),
                key_down(Scancode::Space, false),
                Event::ControllerDeviceAdded {
                    timestamp: 0,
                    which: 0,
                },
                button(true, 2, Button::Start),
                Event::Window {
                    timestamp: 0,
                    window_id: 1,
                    win_event: WindowEvent::FocusLost,
                },
                key_up(Scancode::Left),
                button(false, 2, Button::Start),
            ],
        );
        let pad = PadId(2);
        assert_eq!(
            out[4..],
            [
                PhysicalInput::KeyUp(Key::Left),
                PhysicalInput::KeyUp(Key::Space),
                PhysicalInput::PadButtonUp {
                    pad,
                    button: PadButton::Start
                },
            ]
        );
    }

    #[test]
    fn events_of_pads_that_are_not_open_are_dropped() {
        let mut s = InputState::default();
        let mut p = FakePads {
            devices: vec![Some(5)],
            ..Default::default()
        };
        let axis = |which| Event::ControllerAxisMotion {
            timestamp: 0,
            which,
            axis: Axis::RightY,
            value: 1234,
        };
        let out = run(
            &mut s,
            &mut p,
            &[
                axis(5),
                button(true, 5, Button::A),
                Event::ControllerDeviceAdded {
                    timestamp: 0,
                    which: 0,
                },
                axis(6),
                button(true, 6, Button::A),
                Event::ControllerDeviceRemoved {
                    timestamp: 0,
                    which: 6,
                },
                axis(5),
            ],
        );
        assert_eq!(
            out,
            [
                PhysicalInput::PadConnected(PadId(5)),
                PhysicalInput::PadAxis {
                    pad: PadId(5),
                    axis: PadAxis::RightY,
                    value: 1234
                },
            ]
        );
        assert!(p.closed.is_empty());
    }

    #[test]
    fn joystick_added_is_reported_to_the_backend_only() {
        let mut p = FakePads::default();
        let out = run(
            &mut InputState::default(),
            &mut p,
            &[Event::JoyDeviceAdded {
                timestamp: 0,
                which: 3,
            }],
        );
        assert!(out.is_empty());
        assert_eq!(p.joysticks, [3]);
        assert!(p.opened.is_empty());
    }

    #[test]
    fn button_and_axis_tables() {
        let buttons = [
            (Button::A, PadButton::South),
            (Button::B, PadButton::East),
            (Button::X, PadButton::West),
            (Button::Y, PadButton::North),
            (Button::DPadUp, PadButton::DPadUp),
            (Button::DPadDown, PadButton::DPadDown),
            (Button::DPadLeft, PadButton::DPadLeft),
            (Button::DPadRight, PadButton::DPadRight),
            (Button::Start, PadButton::Start),
            (Button::Back, PadButton::Back),
            (Button::LeftShoulder, PadButton::LeftShoulder),
            (Button::RightShoulder, PadButton::RightShoulder),
            (Button::LeftStick, PadButton::LeftStick),
            (Button::RightStick, PadButton::RightStick),
        ];
        for (b, want) in buttons {
            assert_eq!(pad_button(b), Some(want), "{b:?}");
        }
        assert_eq!(pad_button(Button::Guide), None);
        assert_eq!(pad_button(Button::Touchpad), None);
        for (a, want) in [
            (Axis::LeftX, PadAxis::LeftX),
            (Axis::LeftY, PadAxis::LeftY),
            (Axis::RightX, PadAxis::RightX),
            (Axis::RightY, PadAxis::RightY),
            (Axis::TriggerLeft, PadAxis::TriggerLeft),
            (Axis::TriggerRight, PadAxis::TriggerRight),
        ] {
            assert_eq!(pad_axis(a), want, "{a:?}");
        }
        for b in [
            Button::Misc1,
            Button::Paddle1,
            Button::Paddle2,
            Button::Paddle3,
            Button::Paddle4,
        ] {
            assert_eq!(pad_button(b), None, "{b:?}");
        }
    }
}
