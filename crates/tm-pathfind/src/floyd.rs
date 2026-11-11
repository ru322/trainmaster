use crate::apsp::AllPairsShortestPath;
use crate::graph::{Graph, NodeId};
use crate::table::{INF, RouteTable};

/// Floyd–Warshall。テストの正解(オラクル)として使う(02 D7)。
#[derive(Clone, Copy, Debug, Default)]
pub struct FloydWarshall;

impl AllPairsShortestPath for FloydWarshall {
    fn compute(&self, graph: &Graph) -> RouteTable {
        let n = graph.node_count() as usize;
        let mut d = vec![INF; n * n];
        for i in 0..n {
            d[i * n + i] = 0;
            for &(v, w) in graph.edges_from(NodeId(i as u32)) {
                let c = &mut d[i * n + v.0 as usize];
                *c = (*c).min(w.0);
            }
        }
        for k in 0..n {
            for i in 0..n {
                let dik = d[i * n + k];
                if dik == INF {
                    continue;
                }
                for j in 0..n {
                    let dkj = d[k * n + j];
                    if dkj == INF {
                        continue;
                    }
                    let c = dik
                        .checked_add(dkj)
                        .expect("tm-pathfind: 上限の前提が破れている");
                    if c < d[i * n + j] {
                        d[i * n + j] = c;
                    }
                }
            }
        }
        RouteTable::from_distances(graph, d)
    }
}
