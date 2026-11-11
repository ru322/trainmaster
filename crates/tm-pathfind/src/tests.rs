use crate::dijkstra::{Probe, distances};
use crate::reverse::{reversed, target_distances};
use crate::table::{INF, NO_HOP, build_next_hops_raw};
use crate::testgen::transit_graph;
use crate::{
    AllPairsShortestPath, Cost, FloydWarshall, Graph, MAX_EDGE_COST, MAX_NODES, NodeId,
    RepeatedDijkstra, ReverseDijkstra, RouteTable, build_next_hops,
};

fn graph(n: u32, edges: &[(u32, u32, u64)]) -> Graph {
    let mut g = Graph::new(n);
    for &(u, v, w) in edges {
        g.add_edge(NodeId(u), NodeId(v), Cost(w));
    }
    g
}

fn both(g: &Graph) -> RouteTable {
    let d = RepeatedDijkstra.compute(g);
    assert_eq!(FloydWarshall.compute(g), d);
    d
}

fn path(t: &RouteTable, from: u32, to: u32) -> Vec<u32> {
    t.path(NodeId(from), NodeId(to)).map(|v| v.0).collect()
}

fn cost(t: &RouteTable, from: u32, to: u32) -> Option<u64> {
    t.cost(NodeId(from), NodeId(to)).map(|c| c.0)
}

fn hop(t: &RouteTable, from: u32, to: u32) -> Option<u32> {
    t.next_hop(NodeId(from), NodeId(to)).map(|v| v.0)
}

#[test]
fn cost_and_graph_basics() {
    assert_eq!(Cost::ZERO, Cost(0));
    assert_eq!(Cost(3).saturating_add(Cost(4)), Cost(7));
    assert_eq!(Cost(u64::MAX - 1).saturating_add(Cost(5)), Cost(u64::MAX));
    let g = graph(3, &[(0, 2, 5), (0, 1, 1), (0, 2, 3), (1, 1, 0)]);
    assert_eq!(g.node_count(), 3);
    assert_eq!(g.edge_count(), 4);
    assert_eq!(
        g.edges_from(NodeId(0)),
        &[
            (NodeId(2), Cost(5)),
            (NodeId(1), Cost(1)),
            (NodeId(2), Cost(3))
        ]
    );
    assert!(g.edges_from(NodeId(2)).is_empty());
}

#[test]
fn u01_tiny_graphs() {
    let t0 = both(&Graph::new(0));
    assert_eq!(t0, RouteTable::empty());
    assert_eq!(t0.node_count(), 0);

    for t in [
        both(&graph(1, &[])),
        both(&graph(1, &[(0, 0, 5)])),
        both(&graph(1, &[(0, 0, 0)])),
    ] {
        assert_eq!(t.node_count(), 1);
        assert_eq!(cost(&t, 0, 0), Some(0));
        assert_eq!(hop(&t, 0, 0), None);
        assert_eq!(path(&t, 0, 0), [0]);
    }

    let t = both(&graph(2, &[]));
    for u in 0..2 {
        assert_eq!(cost(&t, u, u), Some(0));
        assert_eq!(path(&t, u, u), [u]);
        assert_eq!(cost(&t, u, 1 - u), None);
        assert_eq!(hop(&t, u, 1 - u), None);
        assert!(path(&t, u, 1 - u).is_empty());
    }
}

#[test]
#[should_panic]
fn u01_out_of_range_query_panics() {
    let _ = both(&graph(2, &[])).cost(NodeId(0), NodeId(2));
}

#[test]
fn u02_multi_edges_take_the_minimum() {
    let t = both(&graph(2, &[(0, 1, 5), (0, 1, 3)]));
    assert_eq!(cost(&t, 0, 1), Some(3));
    assert_eq!(hop(&t, 0, 1), Some(1));
    assert_eq!(path(&t, 0, 1), [0, 1]);
}

#[test]
fn u03_ties_pick_the_smallest_node() {
    let t = both(&graph(4, &[(0, 2, 1), (2, 3, 1), (0, 1, 1), (1, 3, 1)]));
    assert_eq!(cost(&t, 0, 3), Some(2));
    assert_eq!(hop(&t, 0, 3), Some(1));
    assert_eq!(path(&t, 0, 3), [0, 1, 3]);

    let t = both(&graph(4, &[(0, 1, 2), (1, 3, 1), (0, 2, 1), (2, 3, 2)]));
    assert_eq!(hop(&t, 0, 3), Some(1));

    let t = both(&graph(4, &[(0, 2, 2), (2, 3, 1), (0, 1, 1), (1, 3, 2)]));
    assert_eq!(hop(&t, 0, 3), Some(1));
    assert_eq!(path(&t, 0, 3), [0, 1, 3]);
}

#[test]
#[should_panic(expected = "MAX_NODES")]
fn u04_too_many_nodes_panics() {
    let _ = Graph::new(MAX_NODES + 1);
}

#[test]
#[should_panic(expected = "範囲外")]
fn u04_edge_from_out_of_range_panics() {
    graph(2, &[]).add_edge(NodeId(2), NodeId(0), Cost(1));
}

#[test]
#[should_panic(expected = "範囲外")]
fn u04_edge_to_out_of_range_panics() {
    graph(2, &[]).add_edge(NodeId(0), NodeId(2), Cost(1));
}

#[test]
#[should_panic(expected = "MAX_EDGE_COST")]
fn u04_too_costly_edge_panics() {
    graph(2, &[]).add_edge(NodeId(0), NodeId(1), Cost(MAX_EDGE_COST + 1));
}

#[test]
fn u04_limits_are_accepted() {
    let mut g = Graph::new(MAX_NODES);
    g.add_edge(NodeId(MAX_NODES - 1), NodeId(0), Cost(MAX_EDGE_COST));
    assert_eq!(g.edge_count(), 1);
}

fn chain(n: u32) -> Graph {
    let edges: Vec<_> = (0..n - 1).map(|i| (i, i + 1, MAX_EDGE_COST)).collect();
    graph(n, &edges)
}

fn check_chain(t: &RouteTable, n: u32) {
    let last = n - 1;
    assert_eq!(cost(t, 0, last), Some(u64::from(last) * MAX_EDGE_COST));
    assert_eq!(hop(t, 0, last), Some(1));
    assert_eq!(cost(t, last, 0), None);
    assert_eq!(path(t, 0, last).len(), n as usize);
}

#[test]
fn u05_max_edge_cost_chain_does_not_overflow() {
    check_chain(&both(&chain(128)), 128);
    check_chain(&RepeatedDijkstra.compute(&chain(1024)), 1024);
}

#[test]
#[ignore = "MAX_NODES の鎖。表が約 200MB になる(test:long で回す)"]
fn u05_max_nodes_chain_does_not_overflow() {
    check_chain(&RepeatedDijkstra.compute(&chain(MAX_NODES)), MAX_NODES);
}

fn check_public_next_hops(g: &Graph) {
    let n = g.node_count();
    let t = RepeatedDijkstra.compute(g);
    let dist: Vec<Option<Cost>> = (0..n)
        .flat_map(|u| (0..n).map(move |v| (u, v)))
        .map(|(u, v)| t.cost(NodeId(u), NodeId(v)))
        .collect();
    let public = build_next_hops(g, &dist);

    let raw_dist: Vec<u64> = dist.iter().map(|d| d.map_or(INF, |c| c.0)).collect();
    let mut raw = vec![0; raw_dist.len()];
    build_next_hops_raw(g, &raw_dist, &mut raw);

    for u in 0..n {
        for v in 0..n {
            let i = (u * n + v) as usize;
            let expect = t.next_hop(NodeId(u), NodeId(v));
            assert_eq!(public[i], expect);
            assert_eq!((raw[i] != NO_HOP).then_some(NodeId(raw[i])), expect);
        }
    }
}

#[test]
fn u06_public_and_raw_next_hops_agree() {
    check_public_next_hops(&Graph::new(0));
    check_public_next_hops(&g02());
    check_public_next_hops(&g03());
    check_public_next_hops(&graph(
        4,
        &[(0, 2, 1), (2, 3, 1), (0, 1, 1), (1, 3, 1), (3, 3, 0)],
    ));
    check_public_next_hops(&transit_graph(19, 6, 5, 1));
}

#[test]
fn directed_distances_can_be_asymmetric() {
    let t = both(&graph(2, &[(0, 1, 4)]));
    assert_eq!(cost(&t, 0, 1), Some(4));
    assert_eq!(cost(&t, 1, 0), None);
}

/// 12 §8.5 G-02 の 4 駅の例(§3.6 の追加順)。
fn g02() -> Graph {
    graph(
        13,
        &[
            (0, 8, 0),
            (8, 1, 0),
            (2, 9, 0),
            (9, 3, 0),
            (2, 11, 0),
            (11, 3, 0),
            (4, 10, 0),
            (10, 5, 0),
            (6, 12, 0),
            (12, 7, 0),
            (9, 11, 1000),
            (11, 9, 1000),
            (8, 9, 800),
            (9, 8, 800),
            (9, 10, 600),
            (10, 9, 600),
            (11, 12, 700),
            (12, 11, 700),
        ],
    )
}

/// 12 §8.5 G-03 の同点の例。
fn g03() -> Graph {
    graph(
        8,
        &[
            (0, 4, 0),
            (4, 1, 0),
            (0, 6, 0),
            (6, 1, 0),
            (2, 5, 0),
            (5, 3, 0),
            (2, 7, 0),
            (7, 3, 0),
            (4, 6, 1000),
            (6, 4, 1000),
            (5, 7, 1000),
            (7, 5, 1000),
            (4, 5, 800),
            (5, 4, 800),
            (6, 7, 800),
            (7, 6, 800),
        ],
    )
}

#[test]
fn golden_g02() {
    let g = g02();
    assert_eq!((g.node_count(), g.edge_count()), (13, 18));
    let t = both(&g);
    for (from, to, d, h) in [
        (0, 7, 2500, 8),
        (9, 7, 1700, 11),
        (2, 7, 700, 11),
        (4, 1, 1400, 10),
        (6, 1, 2500, 12),
        (2, 5, 600, 9),
        (6, 5, 2300, 12),
    ] {
        assert_eq!(
            (cost(&t, from, to), hop(&t, from, to)),
            (Some(d), Some(h)),
            "{from} -> {to}"
        );
    }
    assert_eq!(path(&t, 0, 7), [0, 8, 9, 11, 12, 7]);
    assert_eq!(cost(&t, 0, 3), Some(800));
    assert_eq!(cost(&t, 2, 7), Some(700));
}

#[test]
fn golden_g03() {
    let t = both(&g03());
    assert_eq!((cost(&t, 0, 3), hop(&t, 0, 3)), (Some(800), Some(4)));
    assert_eq!((cost(&t, 2, 1), hop(&t, 2, 1)), (Some(800), Some(5)));
}

#[derive(Default)]
struct Counts {
    pushes: u64,
    relaxations: u64,
}

impl Probe for Counts {
    fn push(&mut self) {
        self.pushes += 1;
    }
    fn relax(&mut self) {
        self.relaxations += 1;
    }
}

#[test]
fn transit_like_sizes_match_12_section_5_4() {
    for (hubs, stops, n, e) in [
        (19, 19, 152, 1014),
        (24, 24, 192, 1284),
        (27, 27, 216, 1446),
        (32, 32, 256, 1716),
    ] {
        let g = transit_graph(hubs, 6, stops, 1);
        assert_eq!((g.node_count(), g.edge_count()), (n, e));
    }
    assert_eq!(transit_graph(19, 6, 5, 1).node_count(), 68);
}

/// 12 §8.7: n = 152 の固定グラフでの操作回数。計算量を悪くする変更を検出する。
#[test]
fn golden_dijkstra_operation_counts() {
    let g = transit_graph(19, 6, 19, 1);
    let n = u64::from(g.node_count());
    let e = u64::from(g.edge_count());
    let mut c = Counts::default();
    let dist = distances(&g, &mut c);
    assert_eq!(
        RouteTable::from_distances(&g, dist),
        FloydWarshall.compute(&g)
    );
    assert!(c.pushes <= n * (e + 1));
    assert_eq!((c.pushes, c.relaxations), (22_002, 119_814));
}

fn outs(hubs: u32) -> Vec<NodeId> {
    (0..hubs).map(|h| NodeId(2 * h + 1)).collect()
}

fn assert_columns_match(full: &RouteTable, part: &RouteTable, targets: &[NodeId]) {
    let n = full.node_count();
    assert_eq!(part.node_count(), n);
    for t in 0..n + 2 {
        assert_eq!(
            part.has_target(NodeId(t)),
            targets.contains(&NodeId(t)),
            "has_target({t})"
        );
    }
    for &t in targets {
        for u in (0..n).map(NodeId) {
            assert_eq!(part.cost(u, t), full.cost(u, t), "{u:?} -> {t:?}");
            assert_eq!(part.next_hop(u, t), full.next_hop(u, t), "{u:?} -> {t:?}");
            assert!(part.path(u, t).eq(full.path(u, t)), "{u:?} -> {t:?}");
        }
    }
}

#[test]
fn u07_target_columns_match_full_table() {
    for g in [g02(), g03(), transit_graph(19, 6, 5, 1), chain(64)] {
        let full = both(&g);
        let n = g.node_count();
        for targets in [
            (0..n)
                .filter(|t| t % 3 == 1)
                .map(NodeId)
                .collect::<Vec<_>>(),
            vec![NodeId(n - 1)],
            vec![NodeId(0)],
        ] {
            let part = ReverseDijkstra.compute_to(&g, &targets);
            assert_columns_match(&full, &part, &targets);
        }
    }
}

#[test]
fn u07_all_or_no_targets() {
    for g in [Graph::new(0), graph(1, &[(0, 0, 0)]), g02(), g03()] {
        let n = g.node_count();
        let all: Vec<NodeId> = (0..n).map(NodeId).collect();
        assert_eq!(
            ReverseDijkstra.compute_to(&g, &all),
            RepeatedDijkstra.compute(&g)
        );
        let none = ReverseDijkstra.compute_to(&g, &[]);
        assert_eq!(none.node_count(), n);
        assert!((0..n).all(|t| !none.has_target(NodeId(t))));
    }
    assert_eq!(
        ReverseDijkstra.compute_to(&Graph::new(0), &[]),
        RouteTable::empty()
    );
}

#[test]
fn u07_golden_g02_through_reverse() {
    let t = ReverseDijkstra.compute_to(&g02(), &outs(4));
    assert_eq!((cost(&t, 0, 7), hop(&t, 0, 7)), (Some(2500), Some(8)));
    assert_eq!((cost(&t, 9, 7), hop(&t, 9, 7)), (Some(1700), Some(11)));
    assert_eq!((cost(&t, 4, 1), hop(&t, 4, 1)), (Some(1400), Some(10)));
    assert_eq!((cost(&t, 6, 5), hop(&t, 6, 5)), (Some(2300), Some(12)));
    assert_eq!(path(&t, 0, 7), [0, 8, 9, 11, 12, 7]);
    assert_eq!(cost(&t, 7, 7), Some(0));
    assert_eq!(hop(&t, 7, 7), None);
    assert_eq!(cost(&t, 7, 1), None);
    assert!(path(&t, 7, 1).is_empty());
}

#[test]
#[should_panic(expected = "範囲外")]
fn u07_target_out_of_range_panics() {
    let _ = ReverseDijkstra.compute_to(&g03(), &[NodeId(1), NodeId(8)]);
}

#[test]
#[should_panic(expected = "昇順")]
fn u07_unsorted_targets_panic() {
    let _ = ReverseDijkstra.compute_to(&g03(), &[NodeId(3), NodeId(1)]);
}

#[test]
#[should_panic(expected = "昇順")]
fn u07_duplicate_targets_panic() {
    let _ = ReverseDijkstra.compute_to(&g03(), &[NodeId(1), NodeId(1)]);
}

#[test]
#[should_panic(expected = "列を持っていない")]
fn u07_missing_column_cost_panics() {
    let _ = ReverseDijkstra
        .compute_to(&g03(), &[NodeId(1)])
        .cost(NodeId(0), NodeId(3));
}

#[test]
#[should_panic(expected = "列を持っていない")]
fn u07_missing_column_next_hop_panics() {
    let _ = ReverseDijkstra
        .compute_to(&g03(), &[NodeId(1)])
        .next_hop(NodeId(0), NodeId(3));
}

#[test]
#[should_panic(expected = "列を持っていない")]
fn u07_missing_column_path_panics() {
    let _ = ReverseDijkstra
        .compute_to(&g03(), &[NodeId(1)])
        .path(NodeId(0), NodeId(3));
}

#[test]
#[should_panic(expected = "範囲外")]
fn u07_out_of_range_query_panics() {
    let _ = ReverseDijkstra
        .compute_to(&g03(), &[NodeId(1)])
        .cost(NodeId(8), NodeId(1));
}

#[test]
fn u05_max_edge_cost_chain_reverse() {
    let g = chain(1024);
    let t = ReverseDijkstra.compute_to(&g, &[NodeId(0), NodeId(1023)]);
    assert_eq!(cost(&t, 0, 1023), Some(1023 * MAX_EDGE_COST));
    assert_eq!(hop(&t, 0, 1023), Some(1));
    assert_eq!(cost(&t, 1023, 0), None);
    assert_eq!(path(&t, 0, 1023).len(), 1024);
}

/// 12 §8.7: n = 216 の固定グラフで目的地 = 全ハブの出口のときの操作回数。
#[test]
fn golden_reverse_operation_counts() {
    let g = transit_graph(27, 6, 27, 1);
    let targets = outs(27);
    let k = targets.len() as u64;
    let e = u64::from(g.edge_count());
    let mut c = Counts::default();
    let dist = target_distances(&g, &targets, &mut c);
    let part = RouteTable::from_target_distances(&g, &targets, dist);
    assert_columns_match(&FloydWarshall.compute(&g), &part, &targets);
    assert!(c.pushes <= k * (e + 1));
    assert_eq!((c.pushes, c.relaxations), (6_408, 34_830));
}

/// 12 §2.5: 逆向きの辺は元の (from, 追加順) の順。
#[test]
fn reverse_adjacency_keeps_from_then_insertion_order() {
    let g = graph(
        4,
        &[
            (2, 0, 7),
            (1, 0, 5),
            (1, 0, 3),
            (3, 1, 1),
            (0, 0, 2),
            (2, 0, 4),
        ],
    );
    let (start, rev) = reversed(&g);
    assert_eq!(start, [0, 5, 6, 6, 6]);
    let rev: Vec<(u32, u64)> = rev.iter().map(|&(u, w)| (u.0, w.0)).collect();
    assert_eq!(rev, [(0, 2), (1, 5), (1, 3), (2, 7), (2, 4), (3, 1)]);
}

/// 重複辺は追加順に緩和する(重い辺が先なら 2 回 push する)。
#[test]
fn golden_reverse_counts_with_parallel_edges() {
    for (edges, counts) in [
        ([(0, 1, 5), (0, 1, 3)], (3, 2)),
        ([(0, 1, 3), (0, 1, 5)], (2, 2)),
    ] {
        let g = graph(2, &edges);
        let mut c = Counts::default();
        let dist = target_distances(&g, &[NodeId(1)], &mut c);
        assert_eq!(dist, [3, 0]);
        assert_eq!((c.pushes, c.relaxations), counts, "{edges:?}");
    }
}
