//! UI の出来事(`UiEvent`)をチュートリアルに渡す。カーソルの下と directive の変化から `CursorOn` を出す [13 §3.13]。

use tm_core::{GameView, GridPos};
use tm_tutorial::{
    DirectiveKind, MenuAnchor, Tutorial, TutorialDirective, TutorialStageId, UiAnchor, UiEvent,
};

use crate::draw::map::MapCache;
use crate::scene::{PlayMode, PlayScene};

/// directive が変わって `CursorOn` を送り直す回数の上限(送り直しで段階が進むと、また変わる)。
const RESEND_MAX: usize = 4;

/// マップのカーソルの下と、最後に見た directive。
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Bridge {
    under: Option<UiAnchor>,
    key: Option<(TutorialStageId, DirectiveKind, u8)>,
}

/// マップのマスの上の anchor。駅、駅でない線路は `LineId` 最小の路線、何もないマスは `None`。
pub(crate) fn anchor_at(view: GameView<'_>, map: &MapCache, pos: GridPos) -> Option<UiAnchor> {
    view.station_at(pos)
        .map(UiAnchor::Station)
        .or_else(|| map.first_line_at(pos).map(UiAnchor::Line))
}

fn key(d: Option<&TutorialDirective>) -> Option<(TutorialStageId, DirectiveKind, u8)> {
    d.map(|d| (d.stage, d.kind, d.page))
}

/// いまの場所: パネルを開いていれば選んでいる項目、そうでなければマップのカーソルの下。
fn place(p: &PlayScene) -> Option<UiAnchor> {
    match &p.mode {
        PlayMode::Panel(panel) => Some(UiAnchor::Menu(MenuAnchor::UpgradeItem(panel.param()))),
        PlayMode::Map | PlayMode::Build(_) => p.bridge.under,
    }
}

/// このフレームの出来事を `log` とチュートリアルに渡す。渡した後に directive が変わっていれば、
/// いまの場所の `CursorOn` を送り直す(すでに駅の上にいても #2 が進むように)。
pub(crate) fn deliver(
    p: &mut PlayScene,
    view: GameView<'_>,
    mut tutorial: Option<&mut Tutorial>,
    mut log: Option<&mut Vec<UiEvent>>,
) {
    let under = p.map.as_ref().and_then(|m| anchor_at(view, m, p.cursor));
    if under != p.bridge.under {
        p.bridge.under = under;
        if let Some(a) = under {
            p.events.push(UiEvent::CursorOn(a));
        }
    }
    let mut pass = |ev: UiEvent, t: &mut Option<&mut Tutorial>| {
        if let Some(log) = log.as_deref_mut() {
            log.push(ev);
        }
        if let Some(t) = t.as_deref_mut() {
            t.observe_ui(&ev);
        }
    };
    for ev in p.events.drain(..) {
        pass(ev, &mut tutorial);
    }
    for _ in 0..RESEND_MAX {
        let now = key(p.directive(tutorial.as_deref()).as_ref());
        if now == p.bridge.key {
            break;
        }
        p.bridge.key = now;
        if let Some(a) = place(p) {
            pass(UiEvent::CursorOn(a), &mut tutorial);
        }
    }
}
