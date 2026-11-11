use super::*;
use crate::balance::BalanceTable;

/// 16 §2.7 の固定シード。
const SEED_A: u64 = 1;
const SEED_B: u64 = 42;
const SEED_C: u64 = 0xDEAD_BEEF_0123_4567;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/station_plan.txt");

/// 17 GW-15 の 40 × 18 のマップに置く駅の数(会社 Lv 24)。
const STATIONS_27: u32 = 27;

fn builtin_plan(seed: u64) -> (Vec<GridPos>, Vec<Placement>) {
    plan_n(seed, BalanceTable::builtin().max_stations())
}

fn plan_n(seed: u64, count: u32) -> (Vec<GridPos>, Vec<Placement>) {
    let bal = BalanceTable::builtin();
    let spec = bal.spec();
    plan_traced(seed, &spec.map, &spec.company, count)
}

fn render_golden() -> String {
    let bal = BalanceTable::builtin();
    let spec = bal.spec();
    let mut s = String::from(
        "# 11 §11.3 の station_plan(BalanceTable::builtin、max_stations 駅、出現順)。更新は TM_UPDATE_GOLDEN=1\n",
    );
    for seed in [SEED_A, SEED_B, SEED_C] {
        s += &format!("{seed:#x}:");
        for p in plan_stations(seed, &spec.map, &spec.company, bal.max_stations()) {
            s += &format!(" ({},{})", p.x, p.y);
        }
        s.push('\n');
    }
    s
}

#[test]
fn golden_three_seeds() {
    let actual = render_golden();
    if std::env::var_os("TM_UPDATE_GOLDEN").is_some_and(|v| v == "1") {
        std::fs::write(GOLDEN, &actual).expect("ゴールデンを書けない");
        return;
    }
    let expected = std::fs::read_to_string(GOLDEN).expect("tests/golden/station_plan.txt がない");
    assert_eq!(actual, expected, "station_plan のゴールデンが変わった");
}

#[test]
fn reproducible_and_seed_dependent() {
    assert_eq!(builtin_plan(SEED_B), builtin_plan(SEED_B));
    assert_ne!(builtin_plan(SEED_A).0, builtin_plan(SEED_B).0);
    let (full, _) = builtin_plan(SEED_C);
    let bal = BalanceTable::builtin();
    let short = plan_stations(SEED_C, &bal.spec().map, &bal.spec().company, 5);
    assert_eq!(short, full[..5], "前の駅の位置は後の駅の数によらない");
}

/// 01 §6.1 の性質: 置ける範囲、重複なし、最初の駅は中央の範囲、間隔、いちばん近い駅まで `max_nearest` 以下。
fn check_rules(seed: u64, plan: &[GridPos], trace: &[Placement], count: u32) {
    let bal = BalanceTable::builtin();
    let (map, company) = (&bal.spec().map, &bal.spec().company);
    let (w, h, m) = (
        i32::from(map.width),
        i32::from(map.height),
        i32::from(map.margin),
    );
    let (aw, ah) = (i32::from(map.initial_area.0), i32::from(map.initial_area.1));
    let (ix, iy) = ((w - aw) / 2, (h - ah) / 2);
    assert_eq!(plan.len(), count as usize);
    for (i, p) in plan.iter().enumerate() {
        assert!(
            (m..w - m).contains(&p.x) && (m..h - m).contains(&p.y),
            "seed {seed}: 外周に置いた {p:?}"
        );
        if i < usize::from(company.stations_base) {
            assert!(
                (ix..ix + aw).contains(&p.x) && (iy..iy + ah).contains(&p.y),
                "seed {seed}: 最初の駅が中央の外 {p:?}"
            );
        }
        let prev = &plan[..i];
        let min_d = prev.iter().map(|q| p.chebyshev(*q)).min();
        let spacing = match trace[i] {
            Placement::Stage(0) => map.min_spacing,
            Placement::Stage(k) => map.relax_spacings[usize::from(k) - 1],
            other => panic!("seed {seed}: 駅 {i} が {other:?}"),
        };
        if let Some(d) = min_d {
            assert!(
                d >= u32::from(spacing),
                "seed {seed}: 駅 {i} の間隔 {d} < {spacing}"
            );
            assert!(
                d <= u32::from(map.max_nearest),
                "seed {seed}: 駅 {i} のいちばん近い駅まで {d}"
            );
        }
    }
}

#[test]
fn rules_hold_for_many_seeds() {
    let count = BalanceTable::builtin().max_stations();
    for seed in (0..3000u64).chain([SEED_C, u64::MAX]) {
        let (plan, trace) = builtin_plan(seed);
        check_rules(seed, &plan, &trace, count);
    }
}

/// 17 GW-15: 今のマップの規則のまま 27 駅を置ける。
#[test]
fn rules_hold_for_27_stations() {
    for seed in (0..1000u64).chain([SEED_C, u64::MAX]) {
        let (plan, trace) = plan_n(seed, STATIONS_27);
        check_rules(seed, &plan, &trace, STATIONS_27);
    }
}

fn relax_counts(seeds: core::ops::Range<u64>, count: u32) -> (u64, u64, u64) {
    let (mut relaxed_seeds, mut relaxed_stations, mut fallback) = (0, 0, 0);
    for seed in seeds {
        let (_, trace) = plan_n(seed, count);
        let relaxed = trace
            .iter()
            .filter(|t| !matches!(t, Placement::Stage(0)))
            .count() as u64;
        relaxed_seeds += u64::from(relaxed > 0);
        relaxed_stations += relaxed;
        fallback += trace.iter().filter(|t| **t == Placement::Fallback).count() as u64;
    }
    (relaxed_seeds, relaxed_stations, fallback)
}

/// 40 × 18・27 駅の緩和は実測で 1 駅あたり 0.35%(5,000 シード)・0.37%(10 万シード。VS-02)。最後の手段は 0。
#[test]
fn relaxation_is_rare() {
    let n = BalanceTable::builtin().max_stations();
    let (_, relaxed, fallback) = relax_counts(0..20_000, n);
    assert_eq!(fallback, 0);
    assert!(
        relaxed * 200 <= 20_000 * u64::from(n),
        "2 万シード × {n} 駅で緩和した駅 {relaxed}(0.5% 以下)"
    );
    let (_, relaxed, fallback) = relax_counts(0..5_000, STATIONS_27);
    assert_eq!(fallback, 0);
    assert!(
        (300..=700).contains(&relaxed),
        "5,000 シード × 27 駅で緩和した駅 {relaxed}"
    );
}

/// VS-02: 10 万シードの集計。`cargo test -p tm-core --release vs02 -- --ignored --nocapture`。
#[test]
#[ignore = "VS-02 の記録用(10 万シード)"]
fn vs02_relaxation_100k() {
    assert_eq!(BalanceTable::builtin().max_stations(), STATIONS_27);
    for count in [STATIONS_27] {
        for n in [5_000, 20_000, 100_000] {
            let (seeds, stations, fallback) = relax_counts(0..n, count);
            println!(
                "VS-02: {count} 駅 × {n} シードのうち緩和 {seeds} シード({:.3}%)、緩和した駅 {stations}({:.3}%/駅)、最後の手段 {fallback}",
                seeds as f64 * 100.0 / n as f64,
                stations as f64 * 100.0 / (n * u64::from(count)) as f64
            );
            assert_eq!(fallback, 0);
        }
    }
}

fn tight_map(min_spacing: u8, relax: Vec<u8>) -> MapRules {
    MapRules {
        width: 7,
        height: 7,
        margin: 1,
        initial_area: (3, 3),
        min_spacing,
        max_nearest: 10,
        relax_spacings: relax,
        attempts_per_stage: 3,
    }
}

fn company(stations_base: u8) -> CompanyRules {
    CompanyRules {
        max_level: 16,
        cap_base: 4,
        cap_step: 3,
        gauge_num: 7,
        gauge_den: 10,
        stations_base,
        stations_per_level: 1,
    }
}

/// SIM-D-11: 最初の駅の範囲の左上は `(w − aw) / 2`、`(h − ah) / 2` の切り捨て(差が奇数の場合)。
#[test]
fn initial_area_corner_rounds_down() {
    let map = MapRules {
        width: 11,
        height: 9,
        initial_area: (4, 4),
        min_spacing: 1,
        attempts_per_stage: 1000,
        ..tight_map(1, vec![])
    };
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for seed in 0..200 {
        let (plan, _) = plan_traced(seed, &map, &company(3), 3);
        xs.extend(plan.iter().map(|p| p.x));
        ys.extend(plan.iter().map(|p| p.y));
    }
    let range = |v: &[i32]| (*v.iter().min().unwrap(), *v.iter().max().unwrap());
    assert_eq!(range(&xs), (3, 6), "x ∈ [⌊7/2⌋, ⌊7/2⌋ + 4)");
    assert_eq!(range(&ys), (2, 5), "y ∈ [⌊5/2⌋, ⌊5/2⌋ + 4)");
}

#[test]
fn relaxes_spacing_when_strict_spacing_fails() {
    let (plan, trace) = plan_traced(SEED_A, &tight_map(9, vec![2]), &company(2), 2);
    assert_eq!(trace, [Placement::Stage(0), Placement::Stage(1)]);
    assert!(plan[0].chebyshev(plan[1]) >= 2);
}

#[test]
fn fallback_scans_initial_area_then_place() {
    let (plan, trace) = plan_traced(SEED_B, &tight_map(9, vec![]), &company(10), 11);
    assert_eq!(trace[0], Placement::Stage(0));
    assert!(trace[1..].iter().all(|t| *t == Placement::Fallback));
    let init: Vec<GridPos> = (2..5)
        .flat_map(|y| (2..5).map(move |x| GridPos { x, y }))
        .collect();
    for (i, p) in plan.iter().enumerate().take(9) {
        assert!(init.contains(p), "駅 {i} が中央の範囲の外 {p:?}");
        if i > 0 {
            let first = init.iter().find(|q| !plan[..i].contains(q)).unwrap();
            assert_eq!(p, first, "y、x の昇順の最初の空き");
        }
    }
    assert_eq!(
        plan[9],
        GridPos { x: 1, y: 1 },
        "中央が埋まったら置ける範囲"
    );
    assert_eq!(plan[10], GridPos { x: 2, y: 1 }, "4 駅目以降は置ける範囲");
}

#[test]
fn acceptable_boundaries() {
    let existing = [GridPos { x: 5, y: 5 }];
    assert!(acceptable(GridPos { x: 9, y: 5 }, &existing, 4, 10));
    assert!(!acceptable(GridPos { x: 8, y: 7 }, &existing, 4, 10));
    assert!(acceptable(GridPos { x: 15, y: 5 }, &existing, 4, 10));
    assert!(!acceptable(GridPos { x: 16, y: 5 }, &existing, 4, 10));
    assert!(acceptable(GridPos { x: 30, y: 1 }, &[], 4, 10));
}
