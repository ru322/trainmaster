use crate::graph::Graph;
use crate::table::RouteTable;

/// 全点対最短経路のアルゴリズム(差し替え可能)。
pub trait AllPairsShortestPath {
    /// 距離表と次ホップ表を計算する。コスト 0 の閉路があっても表は正しいが、
    /// [`RouteTable::path`] の停止はコスト 0 の閉路がないときだけ保証する(12 §2.6)。
    fn compute(&self, graph: &Graph) -> RouteTable;
}
