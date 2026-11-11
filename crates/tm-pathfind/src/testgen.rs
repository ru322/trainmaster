//! ランダムグラフの生成器(12 §8.6)と proptest の設定。

mod shape;

use std::ops::RangeInclusive;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use crate::{Cost, Graph, MAX_EDGE_COST, NodeId};

pub(crate) use shape::transit_like;

/// CI で毎回同じケースを回すための既定のシード(16 §2.7 の SEED_A)。`PROPTEST_RNG_SEED` で上書きできる(RT-V-02)。
pub(crate) const FIXED_SEED: u64 = 1;

pub(crate) fn config(cases: Option<u32>) -> Config {
    let mut c = Config {
        failure_persistence: None,
        ..Config::default()
    };
    if let Some(cases) = cases {
        c.cases = cases;
    }
    if matches!(c.rng_seed, RngSeed::Random) {
        c.rng_seed = RngSeed::Fixed(FIXED_SEED);
    }
    c
}

#[derive(Clone, Debug)]
pub(crate) struct GraphSpec {
    pub n: u32,
    pub edges: Vec<(u32, u32, u64)>,
}

impl GraphSpec {
    pub(crate) fn build(&self) -> Graph {
        let mut g = Graph::new(self.n);
        for &(u, v, w) in &self.edges {
            g.add_edge(NodeId(u), NodeId(v), Cost(w));
        }
        g
    }
}

fn family(
    n: RangeInclusive<u32>,
    edges_per_node: u32,
    cost: RangeInclusive<u64>,
) -> BoxedStrategy<GraphSpec> {
    n.prop_flat_map(move |n| {
        let node = 0..n.max(1);
        vec(
            (node.clone(), node, cost.clone()),
            0..=(edges_per_node * n) as usize,
        )
        .prop_map(move |edges| GraphSpec { n, edges })
    })
    .boxed()
}

/// 族 A: 同点がとても多い。
pub(crate) fn family_a() -> BoxedStrategy<GraphSpec> {
    family(0..=40, 4, 1..=3)
}

/// 族 B: コスト 0 の辺は小さい番号から大きい番号へだけ(0 の閉路なし)。
pub(crate) fn family_b() -> BoxedStrategy<GraphSpec> {
    family(0..=40, 4, 0..=3)
        .prop_map(|mut s| {
            for e in &mut s.edges {
                if e.2 == 0 {
                    if e.0 > e.1 {
                        (e.0, e.1) = (e.1, e.0);
                    } else if e.0 == e.1 {
                        e.2 = 1;
                    }
                }
            }
            s
        })
        .boxed()
}

/// 族 C: 族 A の各辺に同じコストの逆向きの辺を足した対称なグラフ。
pub(crate) fn family_c() -> BoxedStrategy<GraphSpec> {
    family_a()
        .prop_map(|mut s| {
            let rev: Vec<_> = s.edges.iter().map(|&(u, v, w)| (v, u, w)).collect();
            s.edges.extend(rev);
            s
        })
        .boxed()
}

/// 族 D: 大きいコスト(あふれの検査)。
pub(crate) fn family_d() -> BoxedStrategy<GraphSpec> {
    family(0..=20, 4, 1..=MAX_EDGE_COST)
}

/// 族 E: コスト 0 の閉路を許す。
pub(crate) fn family_e() -> BoxedStrategy<GraphSpec> {
    family(0..=40, 4, 0..=2)
}

/// 族 F: 大きいグラフ。
pub(crate) fn family_f() -> BoxedStrategy<GraphSpec> {
    family(100..=256, 8, 1..=1000)
}

/// 族 F のうちノード数 193〜256(ゲームが予備案 1 で計算する範囲。12 RT-D-16)。
pub(crate) fn family_f_upper() -> BoxedStrategy<GraphSpec> {
    family(193..=256, 8, 1..=1000)
}

/// コスト 0 の閉路がない族(P-05、P-06 の対象)。
pub(crate) fn no_zero_cycle() -> BoxedStrategy<GraphSpec> {
    prop_oneof![family_a(), family_b(), family_c(), family_d()].boxed()
}

/// 族 A〜E のどれか。
pub(crate) fn any_family() -> BoxedStrategy<GraphSpec> {
    prop_oneof![family_a(), family_b(), family_c(), family_d(), family_e()].boxed()
}

/// D-02 の全探索用の小さいグラフ(0 の閉路と自己ループを含む)。
pub(crate) fn tiny() -> BoxedStrategy<GraphSpec> {
    family(0..=6, 3, 0..=3)
}

/// 12 §3.6 と同じ形の固定シードのグラフ。
pub(crate) fn transit_graph(hubs: u32, routes: u32, stops: u32, seed: u64) -> Graph {
    let (n, edges) = transit_like(hubs, routes, stops, seed);
    GraphSpec { n, edges }.build()
}
