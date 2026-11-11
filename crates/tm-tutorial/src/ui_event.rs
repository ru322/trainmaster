//! UI が生成するチュートリアル向けの出来事 [10 §8.4]。

use crate::anchor::{MenuAnchor, UiAnchor};

/// UI で起きた、チュートリアルの進行に関係する出来事。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEvent {
    /// メニューを開いた。
    MenuOpened(MenuAnchor),
    /// メニューを閉じた。
    MenuClosed(MenuAnchor),
    /// カーソルが対象に乗った。
    CursorOn(UiAnchor),
    /// 説明の 1 ページを読んで決定を押した。最後のページなら説明を閉じる。
    MessageAcknowledged,
    /// 会社レベルアップのジングルなどの演出がすべて終わった。
    FanfareFinished,
}
