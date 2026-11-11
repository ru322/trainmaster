/// SplitMix64。固定シードのグラフを作るためだけに使う。
pub(crate) struct SplitMix64(pub(crate) u64);

impl SplitMix64 {
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

const TRANSFER_COST: u64 = 1000;

/// 12 §3.6 と同じ層の形(入口・出口・乗車ノード)のグラフを `(ノード数, 辺の列)` で返す。
/// 各ルートは重複のない `stops` 個のハブに止まり、走行コストは 100〜5000 の一様乱数。
pub(crate) fn transit_like(
    hubs: u32,
    routes: u32,
    stops: u32,
    seed: u64,
) -> (u32, Vec<(u32, u32, u64)>) {
    assert!(stops <= hubs);
    let mut rng = SplitMix64(seed);
    let mut next = 2 * hubs;
    let mut at_hub: Vec<Vec<(u32, u32)>> = vec![Vec::new(); hubs as usize];
    let mut route_base = Vec::with_capacity(routes as usize);
    for r in 0..routes {
        let mut order: Vec<u32> = (0..hubs).collect();
        for k in 0..stops as usize {
            let j = k + rng.below((order.len() - k) as u64) as usize;
            order.swap(k, j);
        }
        route_base.push(next);
        for (k, &h) in order[..stops as usize].iter().enumerate() {
            at_hub[h as usize].push((r, next + k as u32));
        }
        next += stops;
    }
    let mut edges = Vec::new();
    for (h, list) in at_hub.iter().enumerate() {
        let h = h as u32;
        for &(_, b) in list {
            edges.push((2 * h, b, 0));
            edges.push((b, 2 * h + 1, 0));
        }
    }
    for list in &at_hub {
        for &(r1, b1) in list {
            for &(r2, b2) in list {
                if r1 != r2 {
                    edges.push((b1, b2, TRANSFER_COST));
                }
            }
        }
    }
    for &base in &route_base {
        for k in 0..stops.saturating_sub(1) {
            let w = 100 + rng.below(4901);
            edges.push((base + k, base + k + 1, w));
            edges.push((base + k + 1, base + k, w));
        }
    }
    (next, edges)
}
