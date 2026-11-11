//! 割り当て表 [01 §11.2、10 §8.3]。

use tm_platform_api::{Key, PadButton};

use crate::Action;

/// 押しっぱなしのリピートの規則。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepeatRule {
    /// 最初のリピートまでの時間(ミリ秒)。
    pub delay_ms: u16,
    /// 2 回目以降の間隔(ミリ秒)。
    pub interval_ms: u16,
}

/// 物理入力と `Action` の対応。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    /// キーの割り当て。1 つのキーは 1 つの `Action` だけ(重複は先の指定を使う)。
    pub keys: Vec<(Key, Action)>,
    /// パッドのボタンの割り当て。キーと同じ規則。
    pub pad_buttons: Vec<(PadButton, Action)>,
    /// 左スティックを押したとみなす値。
    pub stick_threshold: i16,
    /// 左スティックを離したとみなす値(ヒステリシス。これ未満がデッドゾーン)。
    pub stick_release: i16,
    /// リピートする `Action` だけ並べる。
    pub repeats: Vec<(Action, RepeatRule)>,
}

impl Bindings {
    /// 第 1 版の既定の割り当て [01 §11.2、10 §8.3、13 §2.2]。
    pub fn default_v1() -> Self {
        use Action as A;
        let keys = vec![
            (Key::Up, A::MoveUp),
            (Key::W, A::MoveUp),
            (Key::Down, A::MoveDown),
            (Key::S, A::MoveDown),
            (Key::Left, A::MoveLeft),
            (Key::A, A::MoveLeft),
            (Key::Right, A::MoveRight),
            (Key::D, A::MoveRight),
            (Key::Z, A::Confirm),
            (Key::Enter, A::Confirm),
            (Key::Space, A::Confirm),
            (Key::X, A::Cancel),
            (Key::Backspace, A::Cancel),
            (Key::C, A::BuyMax),
            (Key::Q, A::TabPrev),
            (Key::E, A::TabNext),
            (Key::B, A::BuildMode),
            (Key::Tab, A::ToggleBend),
            (Key::R, A::JumpRecommend),
            (Key::Escape, A::Menu),
        ];
        let pad_buttons = vec![
            (PadButton::DPadUp, A::MoveUp),
            (PadButton::DPadDown, A::MoveDown),
            (PadButton::DPadLeft, A::MoveLeft),
            (PadButton::DPadRight, A::MoveRight),
            (PadButton::South, A::Confirm),
            (PadButton::East, A::Cancel),
            (PadButton::North, A::BuyMax),
            (PadButton::LeftShoulder, A::TabPrev),
            (PadButton::RightShoulder, A::TabNext),
            (PadButton::West, A::BuildMode),
            (PadButton::RightStick, A::ToggleBend),
            (PadButton::Back, A::JumpRecommend),
            (PadButton::Start, A::Menu),
        ];
        let mv = RepeatRule {
            delay_ms: 300,
            interval_ms: 100,
        };
        let repeats = vec![
            (A::MoveUp, mv),
            (A::MoveDown, mv),
            (A::MoveLeft, mv),
            (A::MoveRight, mv),
            (
                A::Confirm,
                RepeatRule {
                    delay_ms: 200,
                    interval_ms: 200,
                },
            ),
        ];
        Self {
            keys,
            pad_buttons,
            stick_threshold: 16384,
            stick_release: 12288,
            repeats,
        }
    }
}
