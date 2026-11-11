//! 小さいグラフ用の全探索オラクル(12 §8.2 D-02)。

use crate::testgen::GraphSpec;

/// 単純経路をすべて列挙して求めた距離表(行優先 n×n、`None` = 到達できない)。
pub(crate) fn shortest(spec: &GraphSpec) -> Vec<Option<u64>> {
    let n = spec.n as usize;
    let mut best = vec![None; n * n];
    let mut on_path = vec![false; n];
    for s in 0..n {
        walk(spec, s, 0, &mut on_path, &mut best[s * n..(s + 1) * n]);
    }
    best
}

fn walk(spec: &GraphSpec, u: usize, cost: u64, on_path: &mut [bool], row: &mut [Option<u64>]) {
    if row[u].is_none_or(|b| cost < b) {
        row[u] = Some(cost);
    }
    on_path[u] = true;
    for &(a, b, w) in &spec.edges {
        if a as usize == u && !on_path[b as usize] {
            walk(spec, b as usize, cost + w, on_path, row);
        }
    }
    on_path[u] = false;
}
