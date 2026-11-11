//! 入力の持ち越し防止(`require_release`)と、出てから一定の時間 決定を受けないガード [13 §3.4、UI-D-13]。

use tm_input::{Action, ActionFrame, ActionSet};
use tm_platform_api::MonoTime;
use tm_tutorial::TutorialDirective;

/// 確認ダイアログと止める説明の各ページが決定を受けない長さ [UI-D-13]。
pub(crate) const GUARD_US: u64 = 300_000;

/// 出てから `GUARD_US` の間は決定を受けない。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Guard {
    until: MonoTime,
}

impl Guard {
    /// `now` に出たもののガード。
    pub(crate) fn from(now: MonoTime) -> Guard {
        Guard {
            until: MonoTime(now.0 + GUARD_US),
        }
    }

    /// 決定を受けてよいか。
    pub(crate) fn open(self, now: MonoTime) -> bool {
        now >= self.until
    }
}

/// `require_release` と、止める説明のページのガード。
#[derive(Debug, Default)]
pub(crate) struct InputCtx {
    require_release: ActionSet,
    layer: Option<u32>,
    page: Option<(u8, u8)>,
    page_guard: Option<Guard>,
}

impl InputCtx {
    /// 離されるまで `require_release` の Action の `pressed` と `repeated` を除いた入力。
    pub(crate) fn filter(&mut self, input: &ActionFrame) -> ActionFrame {
        self.require_release.0 &= input.held.0;
        let mut f = *input;
        f.pressed.0 &= !self.require_release.0;
        f.repeated.0 &= !self.require_release.0;
        f
    }

    /// 画面(層)の番号が変わったら、そのときの `held` を `require_release` に入れる。
    pub(crate) fn end_frame(&mut self, layer: u32, held: ActionSet) {
        if self.layer != Some(layer) {
            self.require_release.0 |= held.0;
        }
        self.layer = Some(layer);
    }

    /// 見えている止める説明のページ。ページが変わったら(見え始めたときを含む)ガードを始める。
    pub(crate) fn watch_page(&mut self, now: MonoTime, shown: Option<&TutorialDirective>) {
        let key = shown.map(|d| (d.stage.0, d.page));
        if key != self.page {
            self.page = key;
            self.page_guard = key.map(|_| Guard::from(now));
        }
    }

    /// 見えている説明のページを決定で読み進めてよいか。
    pub(crate) fn page_ready(&self, now: MonoTime) -> bool {
        self.page_guard.is_some_and(|g| g.open(now))
    }

    /// 離すまで無視している Action か(テスト用)。
    #[cfg(test)]
    pub(crate) fn waiting_release(&self, a: Action) -> bool {
        self.require_release.contains(a)
    }
}

/// `Move*` だけを残した入力(止める説明の間に下の画面へ渡す。13 §3.4 の 4.)。
pub(crate) fn only_moves(input: &ActionFrame) -> ActionFrame {
    let mut keep = ActionSet::default();
    for a in [
        Action::MoveUp,
        Action::MoveDown,
        Action::MoveLeft,
        Action::MoveRight,
    ] {
        keep.insert(a);
    }
    ActionFrame {
        pressed: ActionSet(input.pressed.0 & keep.0),
        held: ActionSet(input.held.0 & keep.0),
        released: ActionSet(input.released.0 & keep.0),
        repeated: ActionSet(input.repeated.0 & keep.0),
        last_device: input.last_device,
    }
}

/// `a` を除いた入力。
pub(crate) fn without(input: &ActionFrame, a: Action) -> ActionFrame {
    let mut one = ActionSet::default();
    one.insert(a);
    let mut f = *input;
    f.pressed.0 &= !one.0;
    f.held.0 &= !one.0;
    f.repeated.0 &= !one.0;
    f
}

#[cfg(test)]
mod tests {
    use tm_tutorial::{DirectiveKind, TutorialStageId};

    use super::*;

    fn frame(pressed: &[Action], held: &[Action]) -> ActionFrame {
        let mut f = ActionFrame::default();
        for &a in pressed {
            f.pressed.insert(a);
        }
        for &a in held {
            f.held.insert(a);
        }
        f
    }

    #[test]
    fn held_on_layer_change_is_ignored_until_released() {
        let mut c = InputCtx::default();
        c.end_frame(1, ActionSet::default());
        let f = c.filter(&frame(&[Action::Confirm], &[Action::Confirm]));
        assert!(f.pressed.contains(Action::Confirm));
        c.end_frame(2, f.held);
        let mut rep = frame(&[], &[Action::Confirm]);
        rep.repeated.insert(Action::Confirm);
        let f = c.filter(&rep);
        assert!(!f.repeated.contains(Action::Confirm), "持ち越さない");
        assert!(f.held.contains(Action::Confirm));
        c.end_frame(2, f.held);
        let _ = c.filter(&frame(&[], &[]));
        c.end_frame(2, ActionSet::default());
        assert!(!c.waiting_release(Action::Confirm));
        let f = c.filter(&frame(&[Action::Confirm], &[Action::Confirm]));
        assert!(f.pressed.contains(Action::Confirm), "離して押し直すと効く");
    }

    #[test]
    fn same_layer_keeps_repeats() {
        let mut c = InputCtx::default();
        c.end_frame(1, ActionSet::default());
        c.end_frame(1, frame(&[], &[Action::Confirm]).held);
        let mut rep = frame(&[], &[Action::Confirm]);
        rep.repeated.insert(Action::Confirm);
        assert!(c.filter(&rep).repeated.contains(Action::Confirm));
    }

    #[test]
    fn guard_is_300ms() {
        let g = Guard::from(MonoTime(1_000));
        assert!(!g.open(MonoTime(300_999)));
        assert!(g.open(MonoTime(301_000)));
    }

    #[test]
    fn page_guard_restarts_on_each_page() {
        let mut d = TutorialDirective {
            stage: TutorialStageId(1),
            kind: DirectiveKind::Explanation,
            message: tm_assets::txt::TUT_S01_P1,
            page: 0,
            page_count: 2,
            highlight: None,
            pauses_game: true,
        };
        let mut c = InputCtx::default();
        c.watch_page(MonoTime(0), Some(&d));
        assert!(!c.page_ready(MonoTime(299_999)));
        c.watch_page(MonoTime(300_000), Some(&d));
        assert!(c.page_ready(MonoTime(300_000)));
        d.page = 1;
        c.watch_page(MonoTime(400_000), Some(&d));
        assert!(!c.page_ready(MonoTime(600_000)));
        assert!(c.page_ready(MonoTime(700_000)));
        c.watch_page(MonoTime(800_000), None);
        assert!(!c.page_ready(MonoTime(900_000)));
    }

    #[test]
    fn only_moves_drops_other_actions() {
        let f = only_moves(&frame(
            &[Action::MoveLeft, Action::Confirm, Action::BuyMax],
            &[Action::MoveLeft, Action::Confirm],
        ));
        assert_eq!(f.pressed, frame(&[Action::MoveLeft], &[]).pressed);
        assert_eq!(f.held, frame(&[], &[Action::MoveLeft]).held);
        let w = without(
            &frame(&[Action::Confirm, Action::MoveUp], &[]),
            Action::Confirm,
        );
        assert!(!w.pressed.contains(Action::Confirm) && w.pressed.contains(Action::MoveUp));
    }
}
