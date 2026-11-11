use proptest::prelude::*;
use proptest::sample::{Index, subsequence};

use crate::brute;
use crate::testgen::{
    GraphSpec, any_family, config, family_c, family_f, family_f_upper, no_zero_cycle, tiny,
};
use crate::{
    AllPairsShortestPath, FloydWarshall, MAX_EDGE_COST, NodeId, RepeatedDijkstra, ReverseDijkstra,
    RouteTable,
};

fn dist(t: &RouteTable, u: u32, v: u32) -> Option<u64> {
    t.cost(NodeId(u), NodeId(v)).map(|c| c.0)
}

/// 最初に食い違った (from, to) と両方の値(D-01 の報告用)。
fn first_difference(a: &RouteTable, b: &RouteTable) -> Option<String> {
    if a.node_count() != b.node_count() {
        return Some(format!(
            "node_count {} != {}",
            a.node_count(),
            b.node_count()
        ));
    }
    let n = a.node_count();
    (0..n)
        .flat_map(|u| (0..n).map(move |t| (u, t)))
        .find_map(|(u, t)| {
            let (u, t) = (NodeId(u), NodeId(t));
            let x = (a.cost(u, t), a.next_hop(u, t));
            let y = (b.cost(u, t), b.next_hop(u, t));
            (x != y).then(|| format!("{u:?} -> {t:?}: {x:?} != {y:?}"))
        })
}

fn check_fw_eq_dijkstra(spec: &GraphSpec) -> Result<(), TestCaseError> {
    let g = spec.build();
    let fw = FloydWarshall.compute(&g);
    let dj = RepeatedDijkstra.compute(&g);
    if let Some(diff) = first_difference(&fw, &dj) {
        return Err(TestCaseError::fail(diff));
    }
    prop_assert_eq!(fw, dj);
    Ok(())
}

/// D-04: 目的地の列だけの表が全点対の表の同じ列と一致する。空・全部・1 個の目的地も確かめる。
fn check_target_columns(spec: &GraphSpec, picked: &[u32]) -> Result<(), TestCaseError> {
    let g = spec.build();
    let full = RepeatedDijkstra.compute(&g);
    let all: Vec<u32> = (0..spec.n).collect();
    let mut cases = vec![picked.to_vec(), Vec::new(), all];
    cases.extend(picked.first().map(|&t| vec![t]));
    for targets in cases {
        let targets: Vec<NodeId> = targets.into_iter().map(NodeId).collect();
        let part = ReverseDijkstra.compute_to(&g, &targets);
        prop_assert_eq!(part.node_count(), spec.n);
        for t in (0..spec.n).map(NodeId) {
            prop_assert_eq!(part.has_target(t), targets.contains(&t));
        }
        for &t in &targets {
            for u in (0..spec.n).map(NodeId) {
                let x = (full.cost(u, t), full.next_hop(u, t));
                let y = (part.cost(u, t), part.next_hop(u, t));
                prop_assert_eq!(x, y, "{:?} -> {:?}", u, t);
            }
        }
    }
    Ok(())
}

fn with_targets(specs: BoxedStrategy<GraphSpec>) -> BoxedStrategy<(GraphSpec, Vec<u32>)> {
    specs
        .prop_flat_map(|s| {
            let ids: Vec<u32> = (0..s.n).collect();
            let n = ids.len();
            (Just(s), subsequence(ids, 0..=n))
        })
        .boxed()
}

fn check_basic_laws(spec: &GraphSpec) -> Result<(), TestCaseError> {
    let t = RepeatedDijkstra.compute(&spec.build());
    let n = spec.n;
    for u in 0..n {
        prop_assert_eq!(dist(&t, u, u), Some(0));
        for to in 0..n {
            let hop = t.next_hop(NodeId(u), NodeId(to));
            prop_assert_eq!(hop.is_none(), u == to || dist(&t, u, to).is_none());
            for v in 0..n {
                if let (Some(a), Some(b)) = (dist(&t, u, v), dist(&t, v, to)) {
                    prop_assert!(dist(&t, u, to).is_some_and(|d| d <= a + b));
                }
            }
        }
    }
    for &(u, v, w) in &spec.edges {
        for to in 0..n {
            if let Some(dv) = dist(&t, v, to) {
                prop_assert!(dist(&t, u, to).is_some_and(|d| d <= w + dv));
            }
        }
    }
    Ok(())
}

fn check_paths(spec: &GraphSpec) -> Result<(), TestCaseError> {
    let t = RepeatedDijkstra.compute(&spec.build());
    let n = spec.n;
    let mut min_edges: Vec<Option<u64>> = vec![None; (n as usize) * (n as usize)];
    for &(a, b, w) in &spec.edges {
        let m = &mut min_edges[(a * n + b) as usize];
        *m = Some(m.map_or(w, |x| x.min(w)));
    }
    let min_edge = |a: u32, b: u32| min_edges[(a * n + b) as usize];
    for u in 0..n {
        for to in 0..n {
            let p: Vec<u32> = t.path(NodeId(u), NodeId(to)).map(|v| v.0).collect();
            let Some(d) = dist(&t, u, to) else {
                prop_assert!(p.is_empty());
                continue;
            };
            prop_assert_eq!(p.first(), Some(&u));
            prop_assert_eq!(p.last(), Some(&to));
            let mut sum = 0;
            for pair in p.windows(2) {
                let w = min_edge(pair[0], pair[1]);
                prop_assert!(w.is_some(), "{:?} は辺ではない", pair);
                sum += w.unwrap_or(0);
            }
            prop_assert_eq!(sum, d);
            let mut seen = p.clone();
            seen.sort();
            seen.dedup();
            prop_assert_eq!(seen.len(), p.len());

            let mut cur = u;
            let mut steps = 0;
            while cur != to {
                prop_assert!(
                    steps < n.saturating_sub(1),
                    "{} -> {} が n - 1 歩で着かない",
                    u,
                    to
                );
                cur = t
                    .next_hop(NodeId(cur), NodeId(to))
                    .map(|v| v.0)
                    .unwrap_or(cur);
                steps += 1;
            }
        }
    }
    Ok(())
}

/// 次ホップの候補(最短経路の条件を満たす辺の行き先)が (u, t) ごとに高々 1 つか。
fn has_no_ties(spec: &GraphSpec, t: &RouteTable) -> bool {
    let n = spec.n;
    (0..n).all(|u| {
        (0..n).all(|to| {
            let Some(dut) = dist(t, u, to) else {
                return true;
            };
            let mut cand: Vec<u32> = spec
                .edges
                .iter()
                .filter(|&&(a, v, w)| {
                    a == u && u != to && dist(t, v, to).is_some_and(|d| d + w == dut)
                })
                .map(|&(_, v, _)| v)
                .collect();
            cand.sort();
            cand.dedup();
            cand.len() <= 1
        })
    })
}

proptest! {
    #![proptest_config(config(None))]

    #[test]
    fn d01_floyd_warshall_equals_dijkstra(spec in any_family()) {
        check_fw_eq_dijkstra(&spec)?;
    }

    #[test]
    fn d02_brute_force_oracle(spec in tiny()) {
        let t = RepeatedDijkstra.compute(&spec.build());
        let n = spec.n;
        let bd = brute::shortest(&spec);
        let bdist = |u: u32, v: u32| bd[(u * n + v) as usize];
        for u in 0..n {
            for to in 0..n {
                prop_assert_eq!(dist(&t, u, to), bdist(u, to));
                let hop = t.next_hop(NodeId(u), NodeId(to)).map(|v| v.0);
                let Some(dut) = bdist(u, to).filter(|_| u != to) else {
                    prop_assert_eq!(hop, None);
                    continue;
                };
                let ok = |v: u32| {
                    spec.edges.iter().any(|&(a, b, w)| {
                        a == u && b == v && bdist(v, to).is_some_and(|d| w + d == dut)
                    })
                };
                let hop = hop.expect("到達できるのに次ホップがない");
                prop_assert!(ok(hop), "{} -> {}: 次ホップ {} が条件を満たさない", u, to, hop);
                prop_assert!((0..hop).all(|v| !ok(v)), "{} -> {}: {} より小さい候補がある", u, to, hop);
            }
        }
    }

    #[test]
    fn p01_to_p04_basic_laws(spec in any_family()) {
        check_basic_laws(&spec)?;
    }

    #[test]
    fn p05_p06_paths_follow_shortest_edges(spec in no_zero_cycle()) {
        check_paths(&spec)?;
    }

    #[test]
    fn d04_target_columns_match((spec, targets) in with_targets(any_family())) {
        check_target_columns(&spec, &targets)?;
    }

    #[test]
    fn p07_node_permutation(
        (spec, perm) in any_family().prop_flat_map(|s| {
            let ids: Vec<u32> = (0..s.n).collect();
            (Just(s), Just(ids).prop_shuffle())
        })
    ) {
        let t = RepeatedDijkstra.compute(&spec.build());
        let moved = GraphSpec {
            n: spec.n,
            edges: spec.edges.iter().map(|&(u, v, w)| (perm[u as usize], perm[v as usize], w)).collect(),
        };
        let t2 = RepeatedDijkstra.compute(&moved.build());
        let no_ties = has_no_ties(&spec, &t);
        for u in 0..spec.n {
            for to in 0..spec.n {
                let (pu, pt) = (perm[u as usize], perm[to as usize]);
                prop_assert_eq!(dist(&t2, pu, pt), dist(&t, u, to));
                if no_ties {
                    let hop = t.next_hop(NodeId(u), NodeId(to)).map(|v| NodeId(perm[v.0 as usize]));
                    prop_assert_eq!(t2.next_hop(NodeId(pu), NodeId(pt)), hop);
                }
            }
        }
    }

    #[test]
    fn p08_edge_order_does_not_matter(
        (spec, shuffled) in any_family().prop_flat_map(|s| {
            let edges = s.edges.clone();
            (Just(s), Just(edges).prop_shuffle())
        })
    ) {
        let other = GraphSpec { n: spec.n, edges: shuffled };
        prop_assert_eq!(
            RepeatedDijkstra.compute(&spec.build()),
            RepeatedDijkstra.compute(&other.build())
        );
    }

    #[test]
    fn p09_costlier_parallel_edge_changes_nothing(
        spec in any_family(),
        pick in any::<Index>(),
        extra in 0u64..=3,
    ) {
        if spec.edges.is_empty() {
            return Ok(());
        }
        let (u, v, w) = spec.edges[pick.index(spec.edges.len())];
        let mut more = spec.clone();
        more.edges.push((u, v, (w + extra).min(MAX_EDGE_COST)));
        prop_assert_eq!(
            RepeatedDijkstra.compute(&spec.build()),
            RepeatedDijkstra.compute(&more.build())
        );
    }

    #[test]
    fn p10_cheaper_edge_never_increases_distances(
        spec in any_family(),
        pick in any::<Index>(),
        cut in any::<Index>(),
    ) {
        if spec.edges.is_empty() {
            return Ok(());
        }
        let i = pick.index(spec.edges.len());
        let mut cheaper = spec.clone();
        let w = cheaper.edges[i].2;
        cheaper.edges[i].2 = w - cut.index(w as usize + 1) as u64;
        let before = RepeatedDijkstra.compute(&spec.build());
        let after = RepeatedDijkstra.compute(&cheaper.build());
        for u in 0..spec.n {
            for to in 0..spec.n {
                if let Some(b) = dist(&before, u, to) {
                    prop_assert!(dist(&after, u, to).is_some_and(|a| a <= b));
                }
            }
        }
    }

    #[test]
    fn p11_symmetric_graphs_have_symmetric_distances(spec in family_c()) {
        let t = RepeatedDijkstra.compute(&spec.build());
        for u in 0..spec.n {
            for v in 0..spec.n {
                prop_assert_eq!(dist(&t, u, v), dist(&t, v, u));
            }
        }
    }
}

proptest! {
    #![proptest_config(config(Some(8)))]

    #[test]
    fn d01_large_graphs(spec in family_f()) {
        check_fw_eq_dijkstra(&spec)?;
    }

    #[test]
    fn d04_large_graphs((spec, targets) in with_targets(family_f())) {
        check_target_columns(&spec, &targets)?;
    }

    #[test]
    fn over_192_nodes_d01_d04_p01_to_p06((spec, targets) in with_targets(family_f_upper())) {
        check_fw_eq_dijkstra(&spec)?;
        check_target_columns(&spec, &targets)?;
        check_basic_laws(&spec)?;
        check_paths(&spec)?;
    }
}
