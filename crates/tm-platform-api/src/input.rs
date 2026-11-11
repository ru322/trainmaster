//! 物理入力 [10 §8.2]。

/// ゲームパッドの SDL のインスタンス ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PadId(pub u32);

/// ゲームで使うキー。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// 上キー。
    Up,
    /// 下キー。
    Down,
    /// 左キー。
    Left,
    /// 右キー。
    Right,
    /// Enter キー。
    Enter,
    /// Esc キー。
    Escape,
    /// Backspace キー。
    Backspace,
    /// スペースキー。
    Space,
    /// Tab キー。
    Tab,
    /// A キー。
    A,
    /// B キー。
    B,
    /// C キー。
    C,
    /// D キー。
    D,
    /// E キー。
    E,
    /// Q キー。
    Q,
    /// R キー。
    R,
    /// S キー。
    S,
    /// W キー。
    W,
    /// X キー。
    X,
    /// Z キー。
    Z,
}

/// ゲームパッドのボタン(SDL の GameController の名前に対応)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PadButton {
    /// 下側の面ボタン(A / ×)。
    South,
    /// 右側の面ボタン(B / ○)。
    East,
    /// 左側の面ボタン(X / □)。
    West,
    /// 上側の面ボタン(Y / △)。
    North,
    /// 十字キーの上。
    DPadUp,
    /// 十字キーの下。
    DPadDown,
    /// 十字キーの左。
    DPadLeft,
    /// 十字キーの右。
    DPadRight,
    /// スタート。
    Start,
    /// バック。
    Back,
    /// 左肩ボタン。
    LeftShoulder,
    /// 右肩ボタン。
    RightShoulder,
    /// 左スティックの押し込み。
    LeftStick,
    /// 右スティックの押し込み。
    RightStick,
}

/// ゲームパッドの軸。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PadAxis {
    /// 左スティックの横。
    LeftX,
    /// 左スティックの縦。
    LeftY,
    /// 右スティックの横。
    RightX,
    /// 右スティックの縦。
    RightY,
    /// 左トリガー。
    TriggerLeft,
    /// 右トリガー。
    TriggerRight,
}

/// プラットフォームが返す物理入力。OS のキーリピートの `KeyDown` は含まない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalInput {
    /// キーが押された。
    KeyDown(Key),
    /// キーが離された。
    KeyUp(Key),
    /// パッドのボタンが押された。
    PadButtonDown {
        /// パッド。
        pad: PadId,
        /// ボタン。
        button: PadButton,
    },
    /// パッドのボタンが離された。
    PadButtonUp {
        /// パッド。
        pad: PadId,
        /// ボタン。
        button: PadButton,
    },
    /// パッドの軸が動いた。
    PadAxis {
        /// パッド。
        pad: PadId,
        /// 軸。
        axis: PadAxis,
        /// 値。`-32768..=32767`。
        value: i16,
    },
    /// パッドがつながった。
    PadConnected(PadId),
    /// パッドが抜かれた。
    PadDisconnected(PadId),
    /// 終了の要求。
    QuitRequested,
}
