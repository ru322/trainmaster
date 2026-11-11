//! 抽象アクション [10 §8.3]。

/// 抽象アクション [01 §11.1]。ビット番号は宣言順。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Action {
    /// カーソルを上へ。
    MoveUp = 0,
    /// カーソルを下へ。
    MoveDown,
    /// カーソルを左へ。
    MoveLeft,
    /// カーソルを右へ。
    MoveRight,
    /// 決定。
    Confirm,
    /// もどる。
    Cancel,
    /// 買えるだけ買う。
    BuyMax,
    /// 前のタブ。
    TabPrev,
    /// 次のタブ。
    TabNext,
    /// ろせんモードの開始・終了。
    BuildMode,
    /// 線路の曲がり方の切り替え。
    ToggleBend,
    /// おすすめへ移る。
    JumpRecommend,
    /// せっていメニューの開閉。
    Menu,
}

pub(crate) const ACTION_COUNT: usize = Action::Menu as usize + 1;

impl Action {
    pub(crate) const ALL: [Action; ACTION_COUNT] = [
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
    ];

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// `Action` の集合。ビット番号は `Action` の宣言順。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionSet(pub u16);

impl ActionSet {
    /// `a` を含むか。
    pub fn contains(self, a: Action) -> bool {
        self.0 & (1 << a.index()) != 0
    }

    /// `a` を加える。
    pub fn insert(&mut self, a: Action) {
        self.0 |= 1 << a.index();
    }

    /// 空か。
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// 最後に操作した機器(画面のボタンの絵の切り替えに使う)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputDevice {
    /// キーボード。
    #[default]
    Keyboard,
    /// ゲームパッド。
    Gamepad,
}

/// 1 フレーム分のアクションの状態。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionFrame {
    /// このフレームで押された(押して同じフレームで離したタップも含む)。
    pub pressed: ActionSet,
    /// フレームの終わりに押されている。
    pub held: ActionSet,
    /// このフレームで離された。
    pub released: ActionSet,
    /// 押しっぱなしによるリピート(`pressed` のフレームは含まない)。
    pub repeated: ActionSet,
    /// 最後に操作した機器。パッドが 0 台になると `Keyboard` に戻る。
    pub last_device: InputDevice,
}
