//! ランダムなイベント列で save → RON → restore した双子が、元と同じ directive・permits・save になる(13 §10.3)。

use super::*;
use crate::save::{SavedSide, SavedStep, TutorialSave};
use crate::stages::stage_pages;

struct R(u64);

impl R {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pct(&mut self, p: u64) -> bool {
        self.below(100) < p
    }
}

fn games() -> Vec<Game> {
    let g0 = game();
    let mut g1 = game();
    let mut ev = Vec::new();
    g1.enqueue(create(&[0, 1]));
    for _ in 0..3000 {
        g1.step(&mut ev);
    }
    let mut g2 = g1.clone();
    g2.enqueue(extend(2));
    for _ in 0..3000 {
        g2.step(&mut ev);
    }
    vec![g0, g1, g2]
}

const PARAMS: [ParamKey; 6] = [
    ParamKey::S1Spawn,
    ParamKey::S2Fare,
    ParamKey::R2LineSlots,
    ParamKey::T1Speed,
    ParamKey::S4Platform,
    ParamKey::T5Cars,
];

const ANCHORS: [MenuAnchor; 9] = [
    MenuAnchor::UpgradePanel,
    MenuAnchor::UpgradeTab(ParamCategory::Station),
    MenuAnchor::UpgradeTab(ParamCategory::Train),
    MenuAnchor::BuildMode,
    MenuAnchor::BuildCreate,
    MenuAnchor::BuildExtend,
    MenuAnchor::LineConfirm,
    MenuAnchor::SettingsMenu,
    MenuAnchor::QuitConfirm { step: 1 },
];

#[derive(Clone, Copy, Debug)]
enum Ev {
    Game(u8, u64),
    Ui(u8, u64),
    Show(bool),
    View(usize),
}

fn random_ev(r: &mut R, games: usize) -> Ev {
    match r.below(10) {
        0..=2 => Ev::Game(r.below(8) as u8, r.next()),
        3..=6 => Ev::Ui(r.below(5) as u8, r.next()),
        7 => Ev::Show(r.pct(70)),
        _ => Ev::View(r.below(games as u64) as usize),
    }
}

fn apply(t: &mut Tutorial, e: Ev, gs: &[Game]) {
    match e {
        Ev::Game(k, x) => {
            let ev = match k {
                0 => arrived(x % 2),
                1 => purchased(
                    PARAMS[(x % 6) as usize],
                    if x & 64 == 0 {
                        BuyMode::One
                    } else {
                        BuyMode::Max
                    },
                ),
                2 => line_created(2 + (x % 3) as u32),
                3 => extended(),
                4 => level(1 + (x % 5) as u8),
                5 => GameEvent::StationSpawned {
                    id: StationId((x % 12) as u32),
                    pos: GridPos { x: 1, y: 1 },
                },
                6 => goal(),
                _ => arrived(0),
            };
            t.observe_game(&ev);
        }
        Ev::Ui(k, x) => {
            let a = ANCHORS[(x % ANCHORS.len() as u64) as usize];
            let ev = match k {
                0 => UiEvent::MenuOpened(a),
                1 => UiEvent::MenuClosed(a),
                2 => on_station((x % 4) as u32),
                3 => ACK,
                _ => FANFARE_DONE,
            };
            t.observe_ui(&ev);
        }
        Ev::Show(on) => t.set_show_explanations(on),
        Ev::View(i) => t.observe_view(gs[i].view()),
    }
}

fn check_directive(t: &Tutorial) {
    let Some(d) = t.directive() else { return };
    match d.kind {
        DirectiveKind::Explanation => {
            let pages = stage_pages(d.stage);
            assert!(d.page < d.page_count, "{d:?}");
            assert_eq!(usize::from(d.page_count), pages.len());
            assert_eq!(pages[usize::from(d.page)], d.message);
            assert_eq!(d.pauses_game, d.stage.0 != 15);
        }
        DirectiveKind::Guide => {
            assert_eq!((d.page, d.page_count, d.pauses_game), (0, 1, false));
        }
    }
    let mut m = t.shown_messages().to_vec();
    m.sort();
    m.dedup();
    assert_eq!(m.len(), t.shown_messages().len(), "shown が重複");
}

/// §6.8 の規則と「せつめいを 出す」の切り替え(§6.7)を当てた後の save。
fn expected_after_restore(s: &TutorialSave, show: bool) -> TutorialSave {
    let mut e = s.clone();
    if let SavedStep::Explain { .. } = e.step {
        e.step = SavedStep::Explain { page: 0 };
    }
    if let SavedSide::Showing { .. } = e.side {
        e.side = SavedSide::NotYet;
    }
    if !show {
        e.replay_pending = false;
        if let SavedStep::Explain { .. } = e.step {
            e.step = SavedStep::Guide { sub: 0 };
            e.unshown = true;
        }
    } else if matches!(e.step, SavedStep::Guide { .. }) && e.unshown {
        e.replay_pending = true;
    }
    e
}

/// §6.8 の規則が効かない時点。
fn plain(t: &Tutorial) -> bool {
    !t.fanfare_pending
        && t.loc == UiLocation::new()
        && !matches!(t.side, SideState::Showing { .. })
        && !matches!(t.step, Step::Explain { page } if page > 0)
}

#[test]
fn restored_twins_follow_the_original() {
    let gs = games();
    let mut twins_checked = 0u64;
    let mut restores = 0u64;
    for seed in 1..=400u64 {
        let mut r = R(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut a = Tutorial::new(r.pct(80));
        let mut b: Option<Tutorial> = None;
        for _ in 0..600 {
            if r.pct(3) {
                let show = a.show;
                let s = a.save();
                let text = ron::to_string(&s).expect("書ける");
                let s2: TutorialSave = ron::from_str(&text).expect("読める");
                assert_eq!(s, s2);
                let rshow = if r.pct(80) { show } else { !show };
                let rt = Tutorial::restore(s2.clone(), rshow);
                assert_eq!(rt.save(), expected_after_restore(&s, rshow), "seed {seed}");
                assert_eq!(Tutorial::restore(rt.save(), rshow).save(), rt.save());
                assert_eq!(rt.shown_messages(), a.shown_messages());
                assert_eq!(rt.shown_stages(), a.shown_stages());
                restores += 1;
                if b.is_none() && plain(&a) {
                    let mut bt = Tutorial::restore(s2, show);
                    let gi = r.below(gs.len() as u64) as usize;
                    a.observe_view(gs[gi].view());
                    bt.observe_view(gs[gi].view());
                    b = Some(bt);
                }
            }
            let e = random_ev(&mut r, gs.len());
            apply(&mut a, e, &gs);
            check_directive(&a);
            if let Some(bt) = b.as_mut() {
                apply(bt, e, &gs);
                assert_eq!(a.directive(), bt.directive(), "seed {seed} {e:?}");
                assert_eq!(permitted(&a), permitted(bt), "seed {seed}");
                assert_eq!(a.is_finished(), bt.is_finished());
                twins_checked += 1;
            }
        }
        if let Some(bt) = b {
            assert_eq!(a.shown_messages(), bt.shown_messages());
            assert_eq!(a.shown_stages(), bt.shown_stages());
            assert_eq!(a.save(), bt.save(), "seed {seed}");
        }
    }
    assert!(twins_checked > 50_000, "{twins_checked}");
    assert!(restores > 5_000, "{restores}");
}

/// restore の直後、最初の observe_view より前にイベントが来ても段階を先へ飛ばさない。
#[test]
fn events_before_the_first_view_do_not_skip_stages() {
    let gs = games();
    let mut r = R(77);
    for _ in 0..3000 {
        let mut a = Tutorial::new(true);
        for _ in 0..r.below(200) {
            let e = random_ev(&mut r, gs.len());
            apply(&mut a, e, &gs);
        }
        if a.fanfare_pending {
            continue;
        }
        let mut t = Tutorial::restore(a.save(), a.show);
        a.observe_ui(&on_station(0));
        t.observe_ui(&on_station(0));
        assert!(
            t.stage_idx <= a.stage_idx,
            "{} > {}",
            t.stage_idx,
            a.stage_idx
        );
        check_directive(&t);
    }
}
