use std::sync::Arc;

use tm_core::BalanceTable;

use crate::scenario::{Pause, Runner, Scenario};
use crate::strategy::Player;

use super::*;

fn snapshot_text() -> String {
    let bal = Arc::new(BalanceTable::builtin());
    let mut r = Runner::new(1, bal, Scenario::new(Player::B, Pause::NONE));
    let mut snap = None;
    while snap.is_none() {
        r.tick_observing(|g| {
            if g.now().0 == 3600 {
                snap = Some(g.snapshot());
            }
        });
    }
    ron::to_string(&snap.expect("写し")).expect("RON")
}

fn save_file(body_fnv: Option<u64>) -> (String, GameSnapshot) {
    let snap: GameSnapshot = ron::from_str(&snapshot_text()).expect("写し");
    let body = format!(
        "SaveBody(game: {}, tutorial: TutorialSave(stage: 9, shown: [1, 2], latch: Some((a: 1))))\n",
        ron::to_string(&snap).expect("RON")
    );
    let mut h = StateHasher::new();
    h.write_bytes(body.as_bytes());
    let header = format!(
        "SaveHeader(magic: \"TMSAVE\", save_format: 1, rules_version: 2, config_hash: 1, game_version: \"x\", seq: 3, tick: {}, state_hash: 0, body_fnv: {})",
        snap.tick,
        body_fnv.unwrap_or(h.finish())
    );
    (format!("{header}\n{body}"), snap)
}

#[test]
fn reads_bare_snapshot() {
    let text = snapshot_text();
    let l = read_snapshot(&text).expect("読める");
    assert_eq!(
        l.snapshot,
        ron::from_str::<GameSnapshot>(&text).expect("写し")
    );
    assert!(l.warnings.is_empty());
}

#[test]
fn reads_save_file_and_checks_body_fnv() {
    let (text, snap) = save_file(None);
    let l = read_snapshot(&text).expect("読める");
    assert_eq!(l.snapshot, snap);
    assert!(l.warnings.is_empty(), "{:?}", l.warnings);
    let (bad, _) = save_file(Some(7));
    let l = read_snapshot(&bad).expect("本体は読める");
    assert_eq!(l.snapshot, snap);
    assert_eq!(l.warnings.len(), 1);
}

#[test]
fn rejects_other_text() {
    assert!(read_snapshot("").is_err());
    assert!(read_snapshot("(a: 1)\n(b: 2)").is_err());
    let (text, _) = save_file(None);
    assert!(read_snapshot(&text.replacen("TMSAVE", "XXSAVE", 1)).is_err());
    let cut = &text[..text.len() / 2];
    assert!(read_snapshot(cut).is_err());
}
