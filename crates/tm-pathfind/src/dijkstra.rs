use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::apsp::AllPairsShortestPath;
use crate::graph::{Cost, Graph, NodeId};
use crate::table::{INF, RouteTable};

/// 各始点からの Dijkstra(二分ヒープ)。既定のアルゴリズム(02 D7)。
#[derive(Clone, Copy, Debug, Default)]
pub struct RepeatedDijkstra;

impl AllPairsShortestPath for RepeatedDijkstra {
    fn compute(&self, graph: &Graph) -> RouteTable {
        RouteTable::from_distances(graph, distances(graph, &mut ()))
    }
}

/// 操作回数の計測点。テストだけが数える(12 §8.7)。
pub(crate) trait Probe {
    fn push(&mut self) {}
    fn relax(&mut self) {}
}

impl Probe for () {}

/// キーを (距離, NodeId) の全順序にして取り出し順まで入力だけで決める(12 RT-D-04)。
pub(crate) type Heap = BinaryHeap<Reverse<(u64, u32)>>;

pub(crate) fn distances(graph: &Graph, probe: &mut impl Probe) -> Vec<u64> {
    let n = graph.node_count() as usize;
    let mut dist = vec![INF; n * n];
    let mut heap = Heap::with_capacity(graph.edge_count() as usize + 1);
    for (s, row) in dist.chunks_exact_mut(n.max(1)).enumerate() {
        single_source(s as u32, row, &mut heap, |u| graph.edges_from(u), probe);
    }
    dist
}

/// `row`(長さ n、すべて `INF`)に `source` からの最短距離を書く。
pub(crate) fn single_source<'g>(
    source: u32,
    row: &mut [u64],
    heap: &mut Heap,
    edges_from: impl Fn(NodeId) -> &'g [(NodeId, Cost)],
    probe: &mut impl Probe,
) {
    row[source as usize] = 0;
    heap.clear();
    heap.push(Reverse((0, source)));
    probe.push();
    while let Some(Reverse((d, u))) = heap.pop() {
        if d > row[u as usize] {
            continue;
        }
        for &(v, w) in edges_from(NodeId(u)) {
            probe.relax();
            let nd = d
                .checked_add(w.0)
                .expect("tm-pathfind: 上限の前提が破れている");
            if nd < row[v.0 as usize] {
                row[v.0 as usize] = nd;
                heap.push(Reverse((nd, v.0)));
                probe.push();
            }
        }
    }
}
