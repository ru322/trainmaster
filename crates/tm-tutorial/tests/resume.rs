//! 続きから(13 §10.3 の「続きから」の行)。自動の子どもを 2 人同じシードで回し、片方だけ途中で
//! `save` → `restore` しても、§6.8 の規則が効かない時点なら同じ directive の列になる。

mod common;

use common::{Kid, LIMIT_TICKS, Options, Rng, SEC};
use tm_tutorial::Tutorial;

fn lockstep(seed: u64, show: bool) -> u32 {
    let opts = Options {
        show,
        drop_fanfare_percent: 0,
        resume_every: 0,
        ..Options::RANDOM
    };
    let mut a = Kid::new(seed, opts);
    let mut b = Kid::new(seed, opts);
    let mut rng = Rng::new(seed.wrapping_mul(31));
    let mut resumes = 0;
    while !a.t.is_finished() {
        assert!(a.ticks() <= LIMIT_TICKS, "seed {seed}: 終わらない");
        if b.at_a_plain_point() && rng.below(20 * SEC) == 0 {
            let text = ron::to_string(&b.t.save()).expect("書ける");
            b.t = Tutorial::restore(ron::from_str(&text).expect("読める"), b.show);
            resumes += 1;
        }
        a.frame().unwrap_or_else(|e| panic!("seed {seed} a: {e}"));
        b.frame().unwrap_or_else(|e| panic!("seed {seed} b: {e}"));
        assert_eq!(
            a.t.directive(),
            b.t.directive(),
            "seed {seed} frame {} resumes {resumes}",
            a.frame
        );
        assert_eq!(a.show, b.show);
    }
    assert!(b.t.is_finished());
    assert_eq!(a.t.shown_messages(), b.t.shown_messages());
    assert_eq!(a.t.shown_stages(), b.t.shown_stages());
    assert_eq!(a.game.state_hash(), b.game.state_hash());
    resumes
}

#[test]
fn restoring_at_plain_points_changes_nothing() {
    for seed in 1..=2 {
        let n = lockstep(seed, true);
        assert!(n >= 10, "seed {seed}: {n} 回");
    }
    assert!(lockstep(3, false) >= 10);
}

#[test]
fn restoring_anywhere_still_finishes() {
    let mut k = Kid::new(
        4,
        Options {
            resume_every: 5 * SEC,
            ..Options::RANDOM
        },
    );
    k.run_to_end().unwrap_or_else(|e| panic!("{e}"));
    assert!(k.log.resumes >= 100, "{}", k.log.resumes);
}

#[test]
#[ignore = "test:long(50 シード)"]
fn restoring_at_plain_points_changes_nothing_50_seeds() {
    for seed in 1..=50 {
        lockstep(seed, seed % 5 != 0);
    }
}
