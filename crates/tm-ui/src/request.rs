//! アプリへの要求とアプリの状態 [10 §8.5、13 §3.2]。

/// アプリ(tm-app)への要求。コアの状態は変えない(コアへの命令は `Command`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppRequest {
    /// タイトルの「初めから」。tm-app がシードを取って `Game` を作る。
    StartNewGame,
    /// タイトルの「続きから」。tm-app がセーブから `Game` と `Tutorial` を作る。
    ContinueGame,
    /// 設定 → 最初からやり直す → 2 回とも「はい」。
    ResetToTitle,
    /// 設定 → ゲームを終わる → 2 回とも「はい」。tm-app が保存を待ってから終える。
    QuitGame,
}

/// tm-app が毎フレーム `update` の前に渡す状態 [13 §3.2]。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AppStatus {
    /// 保存する設定か(`TM_SAVE_DIR=off` なら false)。
    pub save_enabled: bool,
    /// 読めるセーブがある(タイトルの「続きから」)。
    pub has_save: bool,
    /// いま書き込み中(保存中の印)。
    pub saving: bool,
    /// 「ゲームを終わる」で電源を切る(実機)。
    pub power_off_on_quit: bool,
}
