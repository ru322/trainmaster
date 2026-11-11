//! 16 APP-V-08 の机上の分: `snapshot`・RON に書く・RON を読む・`restore` の時間と RON の大きさ。
//! 規模は 01 の上限(駅 27、路線 6、列車 48、会社 Lv24)と、組み込みの表の中盤。

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use tm_core::{Game, GameSnapshot};

#[path = "../tests/common/mod.rs"]
#[allow(dead_code, reason = "テストと共有する補助のうち使わないものがある")]
mod common;

/// 上限の規模まで作って 10 分走らせた状態(待ち人数と乗客が多い)。
fn full() -> Game {
    let mut g = common::full_build(1);
    let mut ev = Vec::new();
    for _ in 0..36_000 {
        ev.clear();
        g.step(&mut ev);
    }
    g
}

/// 組み込みの表で、おすすめの手順で 5 分遊んだ状態。
fn midgame() -> Game {
    common::record(2, &common::builtin(), 18_000, common::driver_recommend).0
}

fn snapshot(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");
    for (name, g) in [("full", full()), ("midgame", midgame())] {
        let bal = Arc::clone(&g.config().balance);
        let snap = g.snapshot();
        let compact = ron::to_string(&snap).expect("RON に書ける");
        let pretty =
            ron::ser::to_string_pretty(&snap, ron::ser::PrettyConfig::new().depth_limit(2))
                .expect("RON に書ける");
        let mut log = tm_core::ReplayLog::new(common::header(g.config().seed, &bal));
        log.start = Some(snap.clone());
        eprintln!(
            "snapshot/{name}: 駅 {}、路線 {}、列車 {}、RON {} バイト(depth_limit(2) で {} バイト、start 入りのリプレイ {} バイト)",
            snap.stations.len(),
            snap.lines.len(),
            snap.trains.len(),
            compact.len(),
            pretty.len(),
            log.to_ron().len()
        );
        group.bench_function(format!("{name}/snapshot"), |b| {
            b.iter(|| black_box(g.snapshot()))
        });
        group.bench_function(format!("{name}/snapshot_to_ron"), |b| {
            b.iter(|| black_box(ron::to_string(&g.snapshot()).expect("RON に書ける")))
        });
        group.bench_function(format!("{name}/restore"), |b| {
            b.iter(|| black_box(Game::restore(Arc::clone(&bal), &snap).expect("作り直せる")))
        });
        group.bench_function(format!("{name}/ron_to_restore"), |b| {
            b.iter(|| {
                let s: GameSnapshot = ron::from_str(&compact).expect("RON を読める");
                black_box(Game::restore(Arc::clone(&bal), &s).expect("作り直せる"))
            })
        });
    }
    group.finish();
}

criterion_group!(benches, snapshot);
criterion_main!(benches);
