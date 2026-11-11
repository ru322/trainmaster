use crate::graph::{Cost, Graph, NodeId};

pub(crate) const INF: u64 = u64::MAX;
pub(crate) const NO_HOP: u32 = u32::MAX;
const NO_COL: u32 = u32::MAX;

/// 距離表と次ホップ表(行優先 n×k。k は持っている列 = 目的地の数。12 §2.4、RT-D-17)。
/// 全点対の表は k = n、[`crate::ReverseDijkstra`] の表は目的地の列だけを持つ。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteTable {
    n: u32,
    k: u32,
    col_of: Vec<u32>,
    dist: Vec<u64>,
    next: Vec<u32>,
}

impl RouteTable {
    /// ノード数 0 の表。
    pub fn empty() -> Self {
        RouteTable {
            n: 0,
            k: 0,
            col_of: Vec::new(),
            dist: Vec::new(),
            next: Vec::new(),
        }
    }

    /// ノード数。
    pub fn node_count(&self) -> u32 {
        self.n
    }

    /// `to` の列を持っているか。範囲外は `false`。
    pub fn has_target(&self, to: NodeId) -> bool {
        to.0 < self.n && self.col_of[to.0 as usize] != NO_COL
    }

    /// `from` から `to` への最短コスト。到達できなければ `None`。範囲外・持たない列は panic。
    pub fn cost(&self, from: NodeId, to: NodeId) -> Option<Cost> {
        let d = self.dist[self.index(from, to)];
        (d != INF).then_some(Cost(d))
    }

    /// `from` から `to` への最短経路の次のノード。`from == to` か到達できなければ `None`。
    /// 範囲外・持たない列は panic。
    pub fn next_hop(&self, from: NodeId, to: NodeId) -> Option<NodeId> {
        let h = self.next[self.index(from, to)];
        (h != NO_HOP).then_some(NodeId(h))
    }

    /// `from` と `to` を両端に含む経路。`from == to` なら `[from]`、到達できなければ空(12 RT-D-06)。
    /// 乗客の行動には使わない。テスト・デバッグ・表示用。
    pub fn path(&self, from: NodeId, to: NodeId) -> Path<'_> {
        let reachable = self.dist[self.index(from, to)] != INF;
        Path {
            table: self,
            cur: if reachable { from.0 } else { NO_HOP },
            to,
            steps_left: self.n,
        }
    }

    pub(crate) fn from_distances(graph: &Graph, dist: Vec<u64>) -> RouteTable {
        let targets: Vec<u32> = (0..graph.node_count()).collect();
        RouteTable::with_columns(graph, targets, dist)
    }

    /// `dist` は行優先 n×k で、列は `targets` の順(昇順・重複なし)。
    pub(crate) fn from_target_distances(
        graph: &Graph,
        targets: &[NodeId],
        dist: Vec<u64>,
    ) -> RouteTable {
        let targets: Vec<u32> = targets.iter().map(|t| t.0).collect();
        RouteTable::with_columns(graph, targets, dist)
    }

    fn with_columns(graph: &Graph, targets: Vec<u32>, dist: Vec<u64>) -> RouteTable {
        let n = graph.node_count();
        let k = targets.len() as u32;
        assert_eq!(dist.len(), (n as usize) * (k as usize));
        let mut col_of = vec![NO_COL; n as usize];
        for (c, &t) in targets.iter().enumerate() {
            assert!(
                t < n && col_of[t as usize] == NO_COL,
                "tm-pathfind: 目的地 {t} が範囲外か重複"
            );
            col_of[t as usize] = c as u32;
        }
        let mut next = vec![NO_HOP; dist.len()];
        next_hops_for(graph, &targets, &dist, &mut next);
        RouteTable {
            n,
            k,
            col_of,
            dist,
            next,
        }
    }

    fn index(&self, from: NodeId, to: NodeId) -> usize {
        assert!(
            from.0 < self.n && to.0 < self.n,
            "tm-pathfind: {from:?} -> {to:?} がノード数 {} の範囲外",
            self.n
        );
        let c = self.col_of[to.0 as usize];
        assert!(c != NO_COL, "tm-pathfind: {to:?} の列を持っていない");
        from.0 as usize * self.k as usize + c as usize
    }
}

/// [`RouteTable::path`] の反復子。`n` 歩で打ち切る。
#[derive(Clone, Debug)]
pub struct Path<'a> {
    table: &'a RouteTable,
    cur: u32,
    to: NodeId,
    steps_left: u32,
}

impl Iterator for Path<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        if self.cur == NO_HOP {
            return None;
        }
        let out = NodeId(self.cur);
        if out == self.to {
            self.cur = NO_HOP;
        } else if self.steps_left == 0 {
            debug_assert!(
                false,
                "tm-pathfind: path() が n 歩で終わらない(コスト 0 の閉路)"
            );
            self.cur = NO_HOP;
        } else {
            self.steps_left -= 1;
            self.cur = self.table.next_hop(out, self.to).map_or(NO_HOP, |h| h.0);
        }
        Some(out)
    }
}

/// 距離表(行優先 n×n、`None` = 到達できない)から次ホップ表を作る共通の後処理(12 §4.4)。
/// 長さが n×n でなければ panic。
/// `dist` は `graph` の最短距離表であること(整合しない表はデバッグで止まり、リリースでは `None`)。
pub fn build_next_hops(graph: &Graph, dist: &[Option<Cost>]) -> Vec<Option<NodeId>> {
    let raw: Vec<u64> = dist.iter().map(|d| d.map_or(INF, |c| c.0)).collect();
    let mut next = vec![NO_HOP; raw.len()];
    build_next_hops_raw(graph, &raw, &mut next);
    next.into_iter()
        .map(|h| (h != NO_HOP).then_some(NodeId(h)))
        .collect()
}

/// 全点対(n×n)の次ホップ表。
pub(crate) fn build_next_hops_raw(graph: &Graph, dist: &[u64], next: &mut [u32]) {
    let targets: Vec<u32> = (0..graph.node_count()).collect();
    next_hops_for(graph, &targets, dist, next);
}

/// 次ホップは、最短経路の条件を満たす辺の行き先のうち最小の `NodeId`(02 §4.3)。
/// 表は行優先 n×k で、列 c の目的地は `targets[c]`。
fn next_hops_for(graph: &Graph, targets: &[u32], dist: &[u64], next: &mut [u32]) {
    let n = graph.node_count() as usize;
    let k = targets.len();
    assert!(dist.len() == n * k && next.len() == n * k);
    next.fill(NO_HOP);
    for u in 0..n {
        let edges = graph.edges_from(NodeId(u as u32));
        for (c, &t) in targets.iter().enumerate() {
            let dut = dist[u * k + c];
            if u == t as usize || dut == INF {
                continue;
            }
            let mut best = NO_HOP;
            for &(v, w) in edges {
                if v.0 < best && w.0.checked_add(dist[v.0 as usize * k + c]) == Some(dut) {
                    best = v.0;
                }
            }
            debug_assert!(
                best != NO_HOP,
                "tm-pathfind: 距離表が最短経路の条件を満たしていない"
            );
            next[u * k + c] = best;
        }
    }
}
