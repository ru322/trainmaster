//! 12 §8.7 のベンチマーク: `compute`・`compute_to`・`build_next_hops` と、辺の列からの再計算全体。
//! 規模は n = 86、152、192(既定の方式)と 194、216、256(予備案 1)。

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use tm_pathfind::{
    AllPairsShortestPath, Cost, FloydWarshall, Graph, NodeId, RepeatedDijkstra, ReverseDijkstra,
    RouteTable, build_next_hops,
};

#[path = "../src/testgen/shape.rs"]
mod shape;

const SEED: u64 = 1;
/// tm-core の `FALLBACK_NODE_THRESHOLD`(12 RT-D-16)。一致は tm-core のテストで確かめる。
const THRESHOLD: u32 = 192;

struct Case {
    n: u32,
    hubs: u32,
    edges: Vec<(u32, u32, u64)>,
}

impl Case {
    fn graph(&self) -> Graph {
        let mut g = Graph::new(self.n);
        for &(u, v, w) in &self.edges {
            g.add_edge(NodeId(u), NodeId(v), Cost(w));
        }
        g
    }

    fn outs(&self) -> Vec<NodeId> {
        (0..self.hubs).map(|h| NodeId(2 * h + 1)).collect()
    }

    /// ゲームの再計算と同じ選び方(n ≤ 192 は全点対、超えたら出口の列だけ)。
    fn recompute(&self) -> RouteTable {
        let g = self.graph();
        if self.n <= THRESHOLD {
            RepeatedDijkstra.compute(&g)
        } else {
            ReverseDijkstra.compute_to(&g, &self.outs())
        }
    }
}

fn cases() -> Vec<Case> {
    [
        (27, 4, 8),
        (19, 6, 19),
        (24, 6, 24),
        (25, 6, 24),
        (27, 6, 27),
        (32, 6, 32),
    ]
    .into_iter()
    .map(|(hubs, routes, stops)| {
        let (n, edges) = shape::transit_like(hubs, routes, stops, SEED);
        Case { n, hubs, edges }
    })
    .collect()
}

fn apsp(c: &mut Criterion) {
    let cases = cases();
    let mut group = c.benchmark_group("compute");
    for case in &cases {
        let g = case.graph();
        group.bench_with_input(BenchmarkId::new("RepeatedDijkstra", case.n), &g, |b, g| {
            b.iter(|| RepeatedDijkstra.compute(black_box(g)));
        });
        group.bench_with_input(BenchmarkId::new("FloydWarshall", case.n), &g, |b, g| {
            b.iter(|| FloydWarshall.compute(black_box(g)));
        });
        let outs = case.outs();
        group.bench_with_input(BenchmarkId::new("ReverseDijkstra", case.n), &g, |b, g| {
            b.iter(|| ReverseDijkstra.compute_to(black_box(g), black_box(&outs)));
        });
    }
    group.finish();

    let mut group = c.benchmark_group("build_next_hops");
    for case in &cases {
        let (n, g) = (case.n, case.graph());
        let t = RepeatedDijkstra.compute(&g);
        let dist: Vec<_> = (0..n)
            .flat_map(|u| (0..n).map(move |v| (u, v)))
            .map(|(u, v)| t.cost(NodeId(u), NodeId(v)))
            .collect();
        group.bench_with_input(BenchmarkId::from_parameter(n), &dist, |b, d| {
            b.iter(|| build_next_hops(black_box(&g), black_box(d)));
        });
    }
    group.finish();

    let mut group = c.benchmark_group("recompute");
    for case in &cases {
        group.bench_with_input(BenchmarkId::from_parameter(case.n), case, |b, case| {
            b.iter(|| black_box(case).recompute());
        });
    }
    group.finish();
}

criterion_group!(benches, apsp);
criterion_main!(benches);
