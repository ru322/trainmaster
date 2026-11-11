//! 詰み検査(13 §6.9、§10.3)。自動の子どもがチュートリアル全体をゲームの時間 40 分以内に終える。

mod common;

use std::time::Instant;

use common::{Kid, MIN, OffPlay, Options, SEC, TogglePlan};

fn run_seed(seed: u64, opts: Options) {
    let wall = Instant::now();
    let mut k = Kid::new(seed, opts);
    k.run_to_end()
        .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    assert_eq!(k.log.acks > 0, opts.show, "seed {seed}");
    assert_eq!(k.log.resumes > 0, opts.resume_every > 0, "seed {seed}");
    eprintln!(
        "seed {seed}: game {}s, frames {}, acks {}, toggles {}, settings {}, refused {}, dropped fanfares {}, resumes {}, {:?}",
        k.ticks() / SEC,
        k.frame,
        k.log.acks,
        k.log.toggles,
        k.log.settings,
        k.log.refused,
        k.log.fanfares_dropped,
        k.log.resumes,
        wall.elapsed()
    );
}

#[test]
fn three_seeds_finish_within_40_minutes() {
    for seed in 1..=3 {
        run_seed(seed, Options::RANDOM);
    }
}

#[test]
#[ignore = "test:long(50 シード)"]
fn fifty_seeds_finish_within_40_minutes() {
    for seed in 1..=50 {
        run_seed(seed, Options::RANDOM);
    }
}

#[test]
fn with_explanations_off_from_the_start_it_still_finishes() {
    run_seed(
        7,
        Options {
            show: false,
            random_settings: false,
            ..Options::RANDOM
        },
    );
}

/// 13 §6.9 の原因 6: 序盤の directive の変わり目のどこで「しない」にして、何をして「する」に戻しても詰まない。
#[test]
fn toggling_at_every_early_step_never_sticks() {
    let quiet = Options {
        show: true,
        random_settings: false,
        deviate_percent: 0,
        drop_fanfare_percent: 0,
        plan: None,
        resume_every: 0,
    };
    let mut base = Kid::new(11, quiet);
    base.run_past_restrictions(20 * MIN)
        .unwrap_or_else(|e| panic!("{e}"));
    let points = base.log.changes.len();
    assert!(points >= 15, "{:?}", base.log.changes);
    let mut runs = 0;
    for at_change in 0..points {
        for (off, play) in [
            (0, OffPlay::Idle),
            (20 * SEC, OffPlay::Idle),
            (20 * SEC, OffPlay::Cursor),
            (90 * SEC, OffPlay::Play),
        ] {
            for seed in [11, 12] {
                let opts = Options {
                    plan: Some(TogglePlan {
                        at_change,
                        off,
                        play,
                    }),
                    ..quiet
                };
                let mut k = Kid::new(seed, opts);
                k.run_past_restrictions(20 * MIN).unwrap_or_else(|e| {
                    panic!("change {at_change} off {off} {play:?} seed {seed}: {e}")
                });
                assert_eq!(k.log.toggles, 2, "change {at_change}");
                runs += 1;
            }
        }
    }
    eprintln!("{runs} 通り");
}

/// 13 §6.9 の原因 4・5: FanfareFinished が来なくても、案内どおりに動かない子でも終わる。
#[test]
fn no_fanfare_and_disobedient_kid_still_finish() {
    run_seed(
        21,
        Options {
            random_settings: false,
            deviate_percent: 60,
            drop_fanfare_percent: 100,
            ..Options::RANDOM
        },
    );
}
