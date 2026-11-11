//! `PhysicalInput` から `ActionFrame` への変換 [13 §2.3]。

use tm_platform_api::{Key, MonoTime, PadButton, PadId, PhysicalInput};

use crate::action::ACTION_COUNT;
use crate::stick::{Dir, StickFilter};
use crate::{Action, ActionFrame, ActionSet, Bindings, InputDevice, RepeatRule};

const KEY_COUNT: usize = 20;
const BUTTON_COUNT: usize = 14;
const MAX_PADS: usize = 8;

// 網羅的な match にして、変種が増えたらコンパイルエラーにする。
fn key_index(k: Key) -> usize {
    match k {
        Key::Up => 0,
        Key::Down => 1,
        Key::Left => 2,
        Key::Right => 3,
        Key::Enter => 4,
        Key::Escape => 5,
        Key::Backspace => 6,
        Key::Space => 7,
        Key::Tab => 8,
        Key::A => 9,
        Key::B => 10,
        Key::C => 11,
        Key::D => 12,
        Key::E => 13,
        Key::Q => 14,
        Key::R => 15,
        Key::S => 16,
        Key::W => 17,
        Key::X => 18,
        Key::Z => 19,
    }
}

fn button_index(b: PadButton) -> usize {
    match b {
        PadButton::South => 0,
        PadButton::East => 1,
        PadButton::West => 2,
        PadButton::North => 3,
        PadButton::DPadUp => 4,
        PadButton::DPadDown => 5,
        PadButton::DPadLeft => 6,
        PadButton::DPadRight => 7,
        PadButton::Start => 8,
        PadButton::Back => 9,
        PadButton::LeftShoulder => 10,
        PadButton::RightShoulder => 11,
        PadButton::LeftStick => 12,
        PadButton::RightStick => 13,
    }
}

#[derive(Clone, Debug)]
struct PadState {
    id: PadId,
    buttons: u16,
    suppressed: u16,
    stick: StickFilter,
}

/// 物理入力を `ActionFrame` に変換する。時刻は `end_frame` の引数で受ける。
#[derive(Clone, Debug)]
pub struct InputMapper {
    key_map: [Option<Action>; KEY_COUNT],
    button_map: [Option<Action>; BUTTON_COUNT],
    repeat_rules: [Option<RepeatRule>; ACTION_COUNT],
    stick_threshold: i16,
    stick_release: i16,
    keys_down: u32,
    keys_suppressed: u32,
    pads: Vec<PadState>,
    hold_count: [u8; ACTION_COUNT],
    edge_down: ActionSet,
    edge_up: ActionSet,
    repeat_next: [Option<MonoTime>; ACTION_COUNT],
    last_device: InputDevice,
}

impl InputMapper {
    /// 割り当て表から作る。
    pub fn new(bindings: Bindings) -> Self {
        let mut key_map = [None; KEY_COUNT];
        for &(k, a) in &bindings.keys {
            key_map[key_index(k)].get_or_insert(a);
        }
        let mut button_map = [None; BUTTON_COUNT];
        for &(b, a) in &bindings.pad_buttons {
            button_map[button_index(b)].get_or_insert(a);
        }
        let mut repeat_rules = [None; ACTION_COUNT];
        for &(a, r) in &bindings.repeats {
            repeat_rules[a.index()].get_or_insert(r);
        }
        Self {
            key_map,
            button_map,
            repeat_rules,
            stick_threshold: bindings.stick_threshold,
            stick_release: bindings.stick_release,
            keys_down: 0,
            keys_suppressed: 0,
            pads: Vec::new(),
            hold_count: [0; ACTION_COUNT],
            edge_down: ActionSet::default(),
            edge_up: ActionSet::default(),
            repeat_next: [None; ACTION_COUNT],
            last_device: InputDevice::Keyboard,
        }
    }

    /// 物理入力を 1 つ取り込む。
    pub fn feed(&mut self, ev: &PhysicalInput) {
        match *ev {
            PhysicalInput::KeyDown(k) => self.key_down(k),
            PhysicalInput::KeyUp(k) => self.key_up(k),
            PhysicalInput::PadButtonDown { pad, button } => self.button_down(pad, button),
            PhysicalInput::PadButtonUp { pad, button } => self.button_up(pad, button),
            PhysicalInput::PadAxis { pad, axis, value } => self.axis(pad, axis, value),
            PhysicalInput::PadConnected(id) => self.connect(id),
            PhysicalInput::PadDisconnected(id) => self.disconnect(id),
            PhysicalInput::QuitRequested => {}
        }
    }

    /// 1 フレームを締めて `ActionFrame` を返す。
    pub fn end_frame(&mut self, now: MonoTime) -> ActionFrame {
        let mut held = ActionSet::default();
        let mut repeated = ActionSet::default();
        let pressed = self.edge_down;
        let released = self.edge_up;
        for a in Action::ALL {
            let i = a.index();
            let is_held = self.hold_count[i] > 0;
            if is_held {
                held.insert(a);
            }
            if let Some(rule) = self.repeat_rules[i] {
                if pressed.contains(a) {
                    self.repeat_next[i] = Some(offset(now, rule.delay_ms));
                } else if is_held && self.repeat_next[i].is_some_and(|t| t <= now) {
                    repeated.insert(a);
                    self.repeat_next[i] = Some(offset(now, rule.interval_ms));
                }
            }
        }
        self.edge_down = ActionSet::default();
        self.edge_up = ActionSet::default();
        ActionFrame {
            pressed,
            held,
            released,
            repeated,
            last_device: self.last_device,
        }
    }

    /// 押されている状態を捨てる。いま押されている入力は離すまで数えない [13 §2.7]。
    pub fn reset(&mut self) {
        self.hold_count = [0; ACTION_COUNT];
        self.edge_down = ActionSet::default();
        self.edge_up = ActionSet::default();
        self.keys_suppressed |= self.keys_down;
        self.keys_down = 0;
        for p in &mut self.pads {
            p.suppressed |= p.buttons;
            p.buttons = 0;
            p.stick.suppress();
        }
    }

    fn press(&mut self, a: Action) {
        let c = &mut self.hold_count[a.index()];
        *c += 1;
        if *c == 1 {
            self.edge_down.insert(a);
        }
    }

    fn release(&mut self, a: Action) {
        let c = &mut self.hold_count[a.index()];
        *c -= 1;
        if *c == 0 {
            self.edge_up.insert(a);
        }
    }

    fn key_down(&mut self, k: Key) {
        let bit = 1u32 << key_index(k);
        if (self.keys_down | self.keys_suppressed) & bit != 0 {
            return;
        }
        self.keys_down |= bit;
        self.last_device = InputDevice::Keyboard;
        if let Some(a) = self.key_map[key_index(k)] {
            self.press(a);
        }
    }

    fn key_up(&mut self, k: Key) {
        let bit = 1u32 << key_index(k);
        if self.keys_suppressed & bit != 0 {
            self.keys_suppressed &= !bit;
        } else if self.keys_down & bit != 0 {
            self.keys_down &= !bit;
            if let Some(a) = self.key_map[key_index(k)] {
                self.release(a);
            }
        }
    }

    fn pad_index(&self, id: PadId) -> Option<usize> {
        self.pads.iter().position(|p| p.id == id)
    }

    fn button_down(&mut self, id: PadId, b: PadButton) {
        let Some(pi) = self.pad_index(id) else {
            return;
        };
        let bit = 1u16 << button_index(b);
        let pad = &mut self.pads[pi];
        if (pad.buttons | pad.suppressed) & bit != 0 {
            return;
        }
        pad.buttons |= bit;
        self.last_device = InputDevice::Gamepad;
        if let Some(a) = self.button_map[button_index(b)] {
            self.press(a);
        }
    }

    fn button_up(&mut self, id: PadId, b: PadButton) {
        let Some(pi) = self.pad_index(id) else {
            return;
        };
        let bit = 1u16 << button_index(b);
        let pad = &mut self.pads[pi];
        if pad.suppressed & bit != 0 {
            pad.suppressed &= !bit;
        } else if pad.buttons & bit != 0 {
            pad.buttons &= !bit;
            if let Some(a) = self.button_map[button_index(b)] {
                self.release(a);
            }
        }
    }

    fn axis(&mut self, id: PadId, axis: tm_platform_api::PadAxis, value: i16) {
        let Some(pi) = self.pad_index(id) else {
            return;
        };
        let before = self.pads[pi].stick.dir();
        self.pads[pi]
            .stick
            .feed(axis, value, self.stick_threshold, self.stick_release);
        let after = self.pads[pi].stick.dir();
        self.change_dir(before, after);
    }

    fn change_dir(&mut self, before: Option<Dir>, after: Option<Dir>) {
        if before == after {
            return;
        }
        if let Some(d) = before {
            self.release(d.action());
        }
        if let Some(d) = after {
            self.press(d.action());
            self.last_device = InputDevice::Gamepad;
        }
    }

    fn connect(&mut self, id: PadId) {
        if self.pad_index(id).is_some() || self.pads.len() >= MAX_PADS {
            return;
        }
        let at = self.pads.partition_point(|p| p.id < id);
        self.pads.insert(
            at,
            PadState {
                id,
                buttons: 0,
                suppressed: 0,
                stick: StickFilter::default(),
            },
        );
    }

    fn disconnect(&mut self, id: PadId) {
        let Some(pi) = self.pad_index(id) else {
            return;
        };
        let pad = self.pads.remove(pi);
        let map = self.button_map;
        for (i, a) in map.into_iter().enumerate() {
            if let Some(a) = a
                && pad.buttons & (1 << i) != 0
            {
                self.release(a);
            }
        }
        self.change_dir(pad.stick.dir(), None);
        if self.pads.is_empty() {
            self.last_device = InputDevice::Keyboard;
        }
    }
}

fn offset(now: MonoTime, ms: u16) -> MonoTime {
    MonoTime(now.0.saturating_add(u64::from(ms) * 1000))
}
