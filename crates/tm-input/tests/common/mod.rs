#![allow(dead_code)]

use tm_input::{Action, ActionFrame, ActionSet, Bindings, InputMapper};
use tm_platform_api::{MonoTime, PadAxis, PadButton, PadId, PhysicalInput};

pub const PAD: PadId = PadId(1);

pub struct H {
    pub m: InputMapper,
    pub now_ms: u64,
}

impl H {
    pub fn new() -> Self {
        Self::with(Bindings::default_v1())
    }

    pub fn with(b: Bindings) -> Self {
        Self {
            m: InputMapper::new(b),
            now_ms: 0,
        }
    }

    pub fn with_pad() -> Self {
        let mut h = Self::new();
        h.feed(PhysicalInput::PadConnected(PAD));
        h
    }

    pub fn feed(&mut self, ev: PhysicalInput) {
        self.m.feed(&ev);
    }

    pub fn frame(&mut self) -> ActionFrame {
        self.m.end_frame(MonoTime(self.now_ms * 1000))
    }

    pub fn step(&mut self, ms: u64) -> ActionFrame {
        self.now_ms += ms;
        self.frame()
    }

    pub fn axis(&mut self, axis: PadAxis, value: i16) {
        self.feed(PhysicalInput::PadAxis {
            pad: PAD,
            axis,
            value,
        });
    }

    pub fn btn_down(&mut self, b: PadButton) {
        self.feed(PhysicalInput::PadButtonDown {
            pad: PAD,
            button: b,
        });
    }

    pub fn btn_up(&mut self, b: PadButton) {
        self.feed(PhysicalInput::PadButtonUp {
            pad: PAD,
            button: b,
        });
    }
}

pub fn set(actions: &[Action]) -> ActionSet {
    let mut s = ActionSet::default();
    for &a in actions {
        s.insert(a);
    }
    s
}
