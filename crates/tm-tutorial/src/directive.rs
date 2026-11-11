//! UI に渡す案内・説明の指示 [10 §8.4]。

use tm_assets::text::TextId;

use crate::anchor::UiAnchor;

/// 01 §10.2 の段階の番号。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TutorialStageId(pub u8);

/// directive の種類 [10 §8.4 TD-17]。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectiveKind {
    /// 止める説明。決定でページを進め、出したときに se_popup を鳴らす。
    Explanation,
    /// 説明の後の、ゲームを止めない操作の案内。se_popup は鳴らさない。
    Guide,
}

/// UI が表示する説明か案内 1 つ分。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TutorialDirective {
    /// 段階の番号。
    pub stage: TutorialStageId,
    /// 種類。
    pub kind: DirectiveKind,
    /// いまのページの文。
    pub message: TextId,
    /// ページ(0 始まり)。
    pub page: u8,
    /// ページ数(1..=4)。
    pub page_count: u8,
    /// 光らせる場所。
    pub highlight: Option<UiAnchor>,
    /// true の間はゲームを止める。
    pub pauses_game: bool,
}
