//! 11 §11.5 の 2・3 のペースの検査(`#[ignore]`、`mise run test:long`)。合否は 11 SIM-D-20 (i) の必須だけ。
//! (ii) 目安と (iii) 参考、結果の CSV、所要時間(03 BV25)は標準エラーに出す。

use std::sync::Arc;
use std::time::Instant;

use tm_core::BalanceTable;
use tm_simrun::report::minutes;
use tm_simrun::scenario::TPS;
use tm_simrun::{Idle, Pause, Player, Report, Route, Scenario, csv_header, run_many};

const MIN: u64 = 60 * TPS;
const FEATURES_LIMIT_MIN: u32 = 30;
const GOAL_LIMIT_MIN: u32 = 24 * 60;
const SEEDS: std::ops::RangeInclusive<u64> = 1..=10;
/// 1 日 1〜2 時間 × 7 日(17 §B.2。分)。
const WEEK_MIN: (f64, f64) = (6.9 * 60.0, 13.0 * 60.0);

fn builtin() -> Arc<BalanceTable> {
    Arc::new(BalanceTable::builtin())
}

fn print_rows(bal: &BalanceTable, reports: &[Report]) {
    eprintln!("{}", csv_header(bal.spec().company.max_level));
    for r in reports {
        eprintln!("{}", r.csv_row());
    }
}

/// SIM-D-20 (i) の共通の必須: 拒否 0、台帳の等式、(b)(c) はおすすめと一致。
fn check_common(r: &Report) {
    let g = r.game.as_ref().expect("実際のゲーム");
    assert_eq!(g.rejected, 0, "{}", r.csv_row());
    assert!(g.ledger.is_conserved(), "{}", r.csv_row());
    if r.player.follows_recommendation() {
        assert_eq!(g.rec_mismatch, 0, "{:?}", g.first_mismatch);
    }
}

/// 中央値(偶数個なら真ん中 2 つの平均。17 §B.2 と同じ)。
fn median(mut v: Vec<u64>) -> Option<u64> {
    v.sort_unstable();
    let n = v.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(v[n / 2]),
        _ => Some((v[n / 2 - 1] + v[n / 2]) / 2),
    }
}

fn mins(v: Option<u64>) -> String {
    v.map_or("-".into(), minutes)
}

fn late(reports: &[Report], limit_min: u32, at: impl Fn(&Report) -> Option<u64>) -> Vec<String> {
    let deadline = u64::from(limit_min) * MIN;
    reports
        .iter()
        .filter(|r| at(r).is_none_or(|t| t > deadline))
        .map(Report::csv_row)
        .collect()
}

#[test]
#[ignore = "90 組 × 30 分。mise run test:long(release-check)で回す"]
fn all_features_within_30_minutes() {
    let started = Instant::now();
    let bal = builtin();
    let mut jobs = Vec::new();
    for player in Player::ALL {
        for pause in Pause::ALL {
            for seed in SEEDS {
                jobs.push((seed, Scenario::new(player, pause)));
            }
        }
    }
    let reports = run_many(&jobs, &bal, FEATURES_LIMIT_MIN);
    print_rows(&bal, &reports);
    reports.iter().for_each(check_common);
    let all = |f: &dyn Fn(&Report) -> bool| {
        median(
            reports
                .iter()
                .filter(|r| f(r))
                .filter_map(|r| r.milestones.all_features)
                .collect(),
        )
    };
    for player in Player::ALL {
        for pause in Pause::ALL {
            let m = all(&|r| r.player == player && r.pause == pause);
            eprintln!(
                "全機能の中央値 ({}) {}: {} 分",
                player.name(),
                pause.name(),
                mins(m)
            );
        }
    }
    let m = all(&|_| true);
    let ok = m.is_some_and(|t| t <= 15 * MIN);
    eprintln!(
        "全機能の中央値(全体): {} 分(目安 15 分以内: {})",
        mins(m),
        if ok { "OK" } else { "外れ" }
    );
    eprintln!("所要時間(BV25): {:.1} s", started.elapsed().as_secs_f64());
    let late = late(&reports, FEATURES_LIMIT_MIN, |r| r.milestones.all_features);
    assert!(
        late.is_empty(),
        "30 分に全機能がそろわない組:\n{}",
        late.join("\n")
    );
}

/// 01 §9 のペーシング表(分。M9 の 100 シード、(a)〜(c) × far/near の中央値の幅)。
const TABLE_9: [(&str, usize, f64, f64); 6] = [
    ("会社 Lv1", 1, 3.5, 3.9),
    ("会社 Lv4", 4, 7.9, 9.7),
    ("会社 Lv12", 12, 26.2, 35.6),
    ("会社 Lv16", 16, 112.3, 152.3),
    ("会社 Lv20", 20, 275.7, 360.3),
    ("会社 Lv24", 24, 495.9, 614.8),
];
const TABLE_9_GOAL: (f64, f64) = (496.6, 670.7);

fn to_min(t: u64) -> f64 {
    t as f64 / MIN as f64
}

fn within_30_percent(v: Option<u64>, (lo, hi): (f64, f64)) -> &'static str {
    match v {
        Some(t) if (lo * 0.7..=hi * 1.3).contains(&to_min(t)) => "±30% 内",
        Some(_) => "±30% 外",
        None => "未達",
    }
}

fn summarize_week(reports: &[Report]) {
    let mut in_week = 0;
    for player in Player::ALL {
        for route in Route::ALL {
            let rs: Vec<&Report> = reports
                .iter()
                .filter(|r| r.player == player && r.route == route)
                .collect();
            let goals: Vec<u64> = rs.iter().filter_map(|r| r.milestones.goal).collect();
            let (lo, hi) = (goals.iter().min().copied(), goals.iter().max().copied());
            let mid = median(goals.clone());
            let ok = mid.is_some_and(|t| (WEEK_MIN.0..=WEEK_MIN.1).contains(&to_min(t)));
            let n = goals
                .iter()
                .filter(|&&t| (WEEK_MIN.0..=WEEK_MIN.1).contains(&to_min(t)))
                .count();
            in_week += n;
            let saved = median(rs.iter().map(|r| r.saved_s * TPS).collect());
            let tiles = median(rs.iter().map(|r| u64::from(r.track_tiles)).collect());
            let feat = median(
                rs.iter()
                    .filter_map(|r| r.milestones.all_features)
                    .collect(),
            );
            eprintln!(
                "({}) {}: ゴール 中央値 {} 分 [{}〜{}](目安 6.9〜13 時間: {})、6.9〜13 時間 {n}/{}、全機能 中央値 {} 分、ためて待った時間 中央値 {} 分、線路のマス 中央値 {}",
                player.name(),
                route.name(),
                mins(mid),
                mins(lo),
                mins(hi),
                if ok { "OK" } else { "外れ" },
                rs.len(),
                mins(feat),
                mins(saved),
                tiles.unwrap_or(0),
            );
            let mut cells = Vec::new();
            for (name, k, a, b) in TABLE_9 {
                let m = median(rs.iter().filter_map(|r| r.milestones.level_at(k)).collect());
                cells.push(format!(
                    "{name} {} 分 {}",
                    mins(m),
                    within_30_percent(m, (a, b))
                ));
            }
            cells.push(format!("ゴール {}", within_30_percent(mid, TABLE_9_GOAL)));
            eprintln!("  参考(01 §9): {}", cells.join("、"));
            let reach: Vec<String> = (1..=8)
                .filter_map(|i| {
                    let at = |r: &&Report| match r.reach.get(i - 1) {
                        Some(x) => Some(u64::from(x.level) * 10),
                        None => r.milestones.goal.map(|_| u64::from(r.end_level) * 10),
                    };
                    let lv = median(rs.iter().filter_map(at).collect())? as f64 / 10.0;
                    Some(format!("{} 時間 Lv{lv}", i as f64 * 1.5))
                })
                .collect();
            eprintln!(
                "  90 分ごとの会社 Lv の中央値(ゴール後は最後の Lv): {}",
                reach.join("、")
            );
        }
    }
    eprintln!(
        "6.9〜13 時間に入った組: {in_week} / {}(17 §B.2 は 55 / 60)",
        reports.len()
    );
}

#[test]
#[ignore = "60 組 × ゴールか 24 時間。mise run test:long(release-check)で回す"]
fn week_goal_within_24_hours() {
    let started = Instant::now();
    let bal = builtin();
    let mut jobs = Vec::new();
    for player in Player::ALL {
        for route in Route::ALL {
            for seed in SEEDS {
                let mut sc = Scenario::new(player, Pause::STANDARD).with_route(route);
                sc.stop_at_goal = true;
                sc.session_s = Some(90 * 60);
                jobs.push((seed, sc));
            }
        }
    }
    let reports = run_many(&jobs, &bal, GOAL_LIMIT_MIN);
    print_rows(&bal, &reports);
    reports.iter().for_each(check_common);
    summarize_week(&reports);
    eprintln!("所要時間(BV25): {:.1} s", started.elapsed().as_secs_f64());
    let mut bad = late(&reports, GOAL_LIMIT_MIN, |r| r.milestones.goal);
    bad.extend(late(&reports, FEATURES_LIMIT_MIN, |r| {
        r.milestones.all_features
    }));
    assert!(
        bad.is_empty(),
        "24 時間にゴールしないか 30 分に全機能がそろわない組:\n{}",
        bad.join("\n")
    );
}

/// 放置の窓の中(始めの 1 分は予約した建設と積んだ強化の分として除く)に会社 Lv が上がった時刻。
fn grown_while_idle(r: &Report, w: Idle) -> Vec<u64> {
    let from = u64::from(w.start_s) * TPS + MIN;
    let to = u64::from(w.end_s).saturating_mul(TPS);
    r.milestones
        .level
        .iter()
        .flatten()
        .copied()
        .filter(|&t| t > from && t <= to)
        .collect()
}

#[test]
#[ignore = "10 シード × 3 通り × 3〜13 時間。mise run test:long(release-check)で回す"]
fn idle_does_not_grow_company() {
    let started = Instant::now();
    let bal = builtin();
    let stop5 = Idle::after(5 * 60);
    let gap = Idle {
        start_s: 90 * 60,
        end_s: 690 * 60,
    };
    let base = Scenario::new(Player::B, Pause::STANDARD);
    let with_idle = |w: Idle| Scenario {
        idle: vec![w],
        ..base.clone()
    };
    let jobs = |sc: &Scenario| -> Vec<(u64, Scenario)> { SEEDS.map(|s| (s, sc.clone())).collect() };
    let stopped = run_many(&jobs(&with_idle(stop5)), &bal, 600);
    let sandwich = run_many(&jobs(&with_idle(gap)), &bal, 780);
    let straight = run_many(&jobs(&base), &bal, 180);
    for r in stopped.iter().chain(&sandwich).chain(&straight) {
        check_common(r);
    }
    print_rows(&bal, &stopped);
    print_rows(&bal, &sandwich);
    let levels = |rs: &[Report]| -> Vec<u8> { rs.iter().map(|r| r.end_level).collect() };
    let mid = |rs: &[Report]| {
        median(rs.iter().map(|r| u64::from(r.end_level) * 10).collect()).map(|v| v as f64 / 10.0)
    };
    eprintln!(
        "5 分でやめて 10 時間: 会社 Lv {:?}(17 §A.3 は Lv1〜2)",
        levels(&stopped)
    );
    eprintln!(
        "90 分 → 10 時間放置 → 90 分: 会社 Lv {:?}(中央値 {:?})、続けて 180 分: {:?}(中央値 {:?})",
        levels(&sandwich),
        mid(&sandwich),
        levels(&straight),
        mid(&straight)
    );
    eprintln!("所要時間(BV25): {:.1} s", started.elapsed().as_secs_f64());
    for (rs, w) in [(&stopped, stop5), (&sandwich, gap)] {
        for r in rs {
            let grown = grown_while_idle(r, w);
            assert!(
                grown.is_empty(),
                "放置中に会社 Lv が上がった {grown:?}: {}",
                r.csv_row()
            );
        }
    }
}
