/// ノードの番号(`0..node_count`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

/// 辺と経路のコスト(非負の整数)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cost(pub u64);

impl Cost {
    /// コスト 0。
    pub const ZERO: Cost = Cost(0);

    /// 飽和加算。このクレートの内部の計算では使わない(12 §2.2)。
    #[must_use]
    pub fn saturating_add(self, rhs: Cost) -> Cost {
        Cost(self.0.saturating_add(rhs.0))
    }
}

/// ノード数の上限(汎用クレートとしての上限。12 RT-D-03)。ゲームは BV-09 で 256 以下。
pub const MAX_NODES: u32 = 4096;

/// 1 本の辺のコストの上限(2^40)。最短距離が 2^52 未満に収まり、計算の途中であふれない。
pub const MAX_EDGE_COST: u64 = 1 << 40;

/// 重み付き有向グラフ(隣接リスト。辺は追加順に持つ)。
#[derive(Clone, Debug)]
pub struct Graph {
    node_count: u32,
    edge_count: u32,
    adj: Vec<Vec<(NodeId, Cost)>>,
}

impl Graph {
    /// 辺のないグラフを作る。`node_count > MAX_NODES` なら panic。
    pub fn new(node_count: u32) -> Self {
        assert!(
            node_count <= MAX_NODES,
            "tm-pathfind: ノード数 {node_count} が MAX_NODES を超えている"
        );
        Graph {
            node_count,
            edge_count: 0,
            adj: vec![Vec::new(); node_count as usize],
        }
    }

    /// ノード数。
    pub fn node_count(&self) -> u32 {
        self.node_count
    }

    /// 辺の総数(多重辺と自己ループを含む)。
    pub fn edge_count(&self) -> u32 {
        self.edge_count
    }

    /// 辺を足す。端点が範囲外か `cost > MAX_EDGE_COST` なら panic。多重辺と自己ループは許す。
    pub fn add_edge(&mut self, from: NodeId, to: NodeId, cost: Cost) {
        assert!(
            from.0 < self.node_count && to.0 < self.node_count,
            "tm-pathfind: 辺 {from:?} -> {to:?} がノード数 {} の範囲外",
            self.node_count
        );
        assert!(
            cost.0 <= MAX_EDGE_COST,
            "tm-pathfind: 辺のコスト {} が MAX_EDGE_COST を超えている",
            cost.0
        );
        self.adj[from.0 as usize].push((to, cost));
        self.edge_count = self
            .edge_count
            .checked_add(1)
            .expect("tm-pathfind: 辺の数が u32 を超えた");
    }

    /// `from` から出る辺(追加順)。`from` が範囲外なら panic。
    pub fn edges_from(&self, from: NodeId) -> &[(NodeId, Cost)] {
        &self.adj[from.0 as usize]
    }
}
