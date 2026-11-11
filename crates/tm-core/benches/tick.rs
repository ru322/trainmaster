//! 02 §9.4・11 §10.1 のベンチマーク: 1 tick(平均と最悪)、`GameView::recommendation()` の 1 回、
//! 経路の再計算の全体。規模は 01 の上限(駅 19、路線 6、列車 48)。最悪の場面の一部は feature `test-support` で入る。

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use tm_core::{Game, GameEvent};

#[path = "../tests/common/mod.rs"]
#[allow(dead_code, reason = "テストと共有する補助のうち使わないものがある")]
mod common;

/// 上限の規模まで作って 1 分走らせた状態。
fn full() -> Game {
    let mut g = common::full_build(1);
    let mut ev = Vec::new();
    for _ in 0..3600 {
        ev.clear();
        g.step(&mut ev);
    }
    g
}

/// 組み込みの表で、おすすめの手順で 5 分遊んだ状態(序盤〜中盤)。
fn midgame() -> Game {
    common::record(2, &common::builtin(), 18_000, common::driver_recommend).0
}

fn arrivals(ev: &[GameEvent]) -> usize {
    ev.iter()
        .filter(|e| matches!(e, GameEvent::TrainArrived { .. }))
        .count()
}

/// 続く 1 分のうち、同じ tick に着く列車がいちばん多い tick の直前の状態。
fn busiest_tick(g: &Game) -> (Game, usize) {
    let mut probe = g.clone();
    let mut best = (probe.clone(), 0);
    let mut ev = Vec::new();
    for _ in 0..3600 {
        let before = probe.clone();
        ev.clear();
        probe.step(&mut ev);
        let n = arrivals(&ev);
        if n > best.1 {
            best = (before, n);
        }
    }
    best
}

fn tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick");
    for (name, game) in [("average_full", full()), ("average_midgame", midgame())] {
        let mut g = game;
        let mut ev = Vec::new();
        group.bench_function(name, |b| {
            b.iter(|| {
                ev.clear();
                g.step(&mut ev);
                black_box(&ev);
            })
        });
    }
    let (busiest, n) = busiest_tick(&full());
    eprintln!("tick/worst_observed: {n} 本が同じ tick に着く");
    group.bench_function("worst_observed", |b| {
        b.iter_batched(
            || (busiest.clone(), Vec::new()),
            |(mut g, mut ev)| {
                g.step(&mut ev);
                (g, ev)
            },
            BatchSize::SmallInput,
        )
    });
    #[cfg(feature = "test-support")]
    {
        let mut aligned = full();
        aligned.put_all_trains_before_arrival();
        eprintln!(
            "tick/worst_all_arrive: {} 本が同じ tick に着く",
            aligned.view().trains().len()
        );
        group.bench_function("worst_all_arrive", |b| {
            b.iter_batched(
                || (aligned.clone(), Vec::new()),
                |(mut g, mut ev)| {
                    g.step(&mut ev);
                    (g, ev)
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn recommendation(c: &mut Criterion) {
    let mut group = c.benchmark_group("recommendation");
    for (name, g) in [("full", full()), ("midgame", midgame())] {
        group.bench_function(name, |b| b.iter(|| black_box(g.view().recommendation())));
    }
    group.finish();
}

#[cfg(feature = "test-support")]
fn routes(c: &mut Criterion) {
    let g = full();
    c.bench_function("routes/recompute_full_tick", |b| {
        b.iter_batched(
            || {
                let mut g = g.clone();
                g.mark_routes_dirty();
                (g, Vec::new())
            },
            |(mut g, mut ev)| {
                g.step(&mut ev);
                (g, ev)
            },
            BatchSize::SmallInput,
        )
    });
}

#[cfg(not(feature = "test-support"))]
fn routes(c: &mut Criterion) {
    // 公開 API だけで再計算を起こす: 最後の路線を作る tick(152 ノードの表を作る)。
    let (before, cmd) = before_last_line();
    c.bench_function("routes/create_last_line_tick", |b| {
        b.iter_batched(
            || {
                let mut g = before.clone();
                g.enqueue(cmd.clone());
                (g, Vec::new())
            },
            |(mut g, mut ev)| {
                g.step(&mut ev);
                (g, ev)
            },
            BatchSize::SmallInput,
        )
    });
}

#[cfg(not(feature = "test-support"))]
fn before_last_line() -> (Game, tm_core::Command) {
    let bal = common::cheap();
    let mut g = common::new_game(1, &bal);
    let mut ev = Vec::new();
    loop {
        let cmds = common::driver_full_build(&g.view());
        if let Some(c) = cmds
            .iter()
            .find(|c| matches!(c, tm_core::Command::CreateLine { .. }))
            && g.view().lines().len() + 1 == bal.max_lines() as usize
        {
            return (g, c.clone());
        }
        cmds.into_iter().for_each(|c| g.enqueue(c));
        ev.clear();
        g.step(&mut ev);
        assert!(g.now().0 < 20_000);
    }
}

criterion_group!(benches, tick, recommendation, routes);
criterion_main!(benches);
