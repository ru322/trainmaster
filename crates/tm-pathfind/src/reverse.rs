use crate::dijkstra::{Heap, Probe, single_source};
use crate::graph::{Cost, Graph, NodeId};
use crate::table::{INF, RouteTable};

/// 目的地の列だけを計算する逆向きの Dijkstra(12 §5.5 の予備案 1、RT-D-17)。
/// 各列の値は全点対の表の同じ列と一致する。
#[derive(Clone, Copy, Debug, Default)]
pub struct ReverseDijkstra;

impl ReverseDijkstra {
    /// `targets`(昇順・重複なし)への距離と次ホップの列だけを持つ表を作る。範囲外・未整列は panic。
    pub fn compute_to(&self, graph: &Graph, targets: &[NodeId]) -> RouteTable {
        let dist = target_distances(graph, targets, &mut ());
        RouteTable::from_target_distances(graph, targets, dist)
    }
}

/// 行優先 n×k(列は `targets` の順)の距離表。
pub(crate) fn target_distances(
    graph: &Graph,
    targets: &[NodeId],
    probe: &mut impl Probe,
) -> Vec<u64> {
    let n = graph.node_count() as usize;
    assert!(
        targets.windows(2).all(|w| w[0] < w[1]),
        "tm-pathfind: 目的地が昇順でないか重複している"
    );
    assert!(
        targets.last().is_none_or(|t| (t.0 as usize) < n),
        "tm-pathfind: 目的地がノード数 {n} の範囲外"
    );
    let (start, rev) = reversed(graph);
    let k = targets.len();
    let mut dist = vec![INF; n * k];
    let mut col = vec![INF; n];
    let mut heap = Heap::with_capacity(graph.edge_count() as usize + 1);
    for (c, t) in targets.iter().enumerate() {
        col.fill(INF);
        single_source(
            t.0,
            &mut col,
            &mut heap,
            |v| &rev[start[v.0 as usize] as usize..start[v.0 as usize + 1] as usize],
            probe,
        );
        for (v, &d) in col.iter().enumerate() {
            dist[v * k + c] = d;
        }
    }
    dist
}

/// 辺を逆向きにした隣接リスト(CSR)。各ノードの辺は元の (from, 追加順) の順。
pub(crate) fn reversed(graph: &Graph) -> (Vec<u32>, Vec<(NodeId, Cost)>) {
    let n = graph.node_count();
    let mut start = vec![0u32; n as usize + 1];
    for u in 0..n {
        for &(v, _) in graph.edges_from(NodeId(u)) {
            start[v.0 as usize + 1] += 1;
        }
    }
    for i in 0..n as usize {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut rev = vec![(NodeId(0), Cost::ZERO); graph.edge_count() as usize];
    for u in 0..n {
        for &(v, w) in graph.edges_from(NodeId(u)) {
            let slot = &mut fill[v.0 as usize];
            rev[*slot as usize] = (NodeId(u), w);
            *slot += 1;
        }
    }
    (start, rev)
}
