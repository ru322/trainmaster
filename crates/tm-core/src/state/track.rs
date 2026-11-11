use crate::geom::{Bend, Dir8, GridPos, TrackMask};

/// 全路線の線路のマス(導出。描画と既定の曲がり方だけが使う)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrackGrid {
    pub width: u8,
    pub height: u8,
    /// 添字 = y × width + x。
    pub masks: Vec<TrackMask>,
}

impl TrackGrid {
    pub fn new(width: u8, height: u8) -> Self {
        TrackGrid {
            width,
            height,
            masks: vec![TrackMask::default(); usize::from(width) * usize::from(height)],
        }
    }

    /// マップの外なら panic する。
    pub fn index(&self, p: GridPos) -> usize {
        assert!(
            (0..i32::from(self.width)).contains(&p.x) && (0..i32::from(self.height)).contains(&p.y),
            "マップの外のマス {p:?}"
        );
        p.y as usize * usize::from(self.width) + p.x as usize
    }

    pub fn get(&self, p: GridPos) -> TrackMask {
        self.masks[self.index(p)]
    }

    /// 空でないマスの数(INV-20)。
    pub fn occupied(&self) -> u32 {
        let n = self.masks.iter().filter(|m| !m.is_empty()).count();
        u32::try_from(n).expect("マスの数は u32")
    }

    /// その向きのビットを OR で立てる。
    pub fn or(&mut self, p: GridPos, d: Dir8) {
        let i = self.index(p);
        self.masks[i].insert(d);
    }
}

/// 線路の自動敷設 [01 §6.3]。`from` から `to` へ `bend` の順に歩いたマス(両端を含む)を `out` に書く。
pub(crate) fn lay_track(from: GridPos, to: GridPos, bend: Bend, out: &mut Vec<GridPos>) {
    out.clear();
    out.extend(walk_track(from, to, bend));
    debug_assert_eq!(out.last(), Some(&to));
}

/// `lay_track` と同じ順のマスを、確保せずに返す(11 §4.8)。
pub(crate) fn walk_track(from: GridPos, to: GridPos, bend: Bend) -> TrackWalk {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let (sx, sy) = (dx.signum(), dy.signum());
    let (ax, ay) = (dx.abs(), dy.abs());
    let diag = (ax.min(ay), (sx, sy));
    let straight = ((ax - ay).abs(), if ax > ay { (sx, 0) } else { (0, sy) });
    let legs = match bend {
        Bend::StraightFirst => [straight, diag],
        Bend::DiagonalFirst => [diag, straight],
    };
    TrackWalk {
        p: from,
        started: false,
        legs,
    }
}

/// `walk_track` の反復子。
#[derive(Clone, Debug)]
pub(crate) struct TrackWalk {
    p: GridPos,
    started: bool,
    /// (残りの歩数, 1 歩の向き) を歩く順に 2 つ。
    legs: [(i32, (i32, i32)); 2],
}

impl Iterator for TrackWalk {
    type Item = GridPos;

    fn next(&mut self) -> Option<GridPos> {
        if !self.started {
            self.started = true;
            return Some(self.p);
        }
        let (n, (ex, ey)) = self.legs.iter_mut().find(|(n, _)| *n > 0)?;
        *n -= 1;
        self.p.x += *ex;
        self.p.y += *ey;
        Some(self.p)
    }
}

/// マスの添字の固定長の集合(1024 マス。10 TD-42)。`validate` で確保せずに重複を除く。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TileBits([u64; 16]);

impl TileBits {
    pub const EMPTY: TileBits = TileBits([0; 16]);

    /// 新しく入れたら true。`i ≥ 1024` なら panic する(BV-20 で起きない)。
    pub fn insert(&mut self, i: usize) -> bool {
        let (w, bit) = (i / 64, 1u64 << (i % 64));
        let fresh = self.0[w] & bit == 0;
        self.0[w] |= bit;
        fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn lay(from: GridPos, to: GridPos, bend: Bend) -> Vec<GridPos> {
        let mut out = vec![GridPos { x: -99, y: -99 }];
        lay_track(from, to, bend, &mut out);
        out
    }

    fn gp(x: i32, y: i32) -> GridPos {
        GridPos { x, y }
    }

    fn check_walk(from: GridPos, to: GridPos, bend: Bend) {
        let tiles = lay(from, to, bend);
        assert_eq!(
            tiles.len() as u32,
            from.chebyshev(to) + 1,
            "{from:?}→{to:?}"
        );
        assert_eq!(tiles[0], from);
        assert_eq!(*tiles.last().unwrap(), to);
        let deltas: Vec<(i32, i32)> = tiles
            .windows(2)
            .map(|w| (w[1].x - w[0].x, w[1].y - w[0].y))
            .collect();
        for &(dx, dy) in &deltas {
            Dir8::from_delta(dx, dy);
        }
        let turns = deltas.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(turns <= 1, "曲がりは 1 回だけ: {from:?}→{to:?} {bend:?}");
        let mut rev = lay(to, from, bend.reversed());
        rev.reverse();
        assert_eq!(rev, tiles, "10 TD-23 の反転: {from:?}→{to:?} {bend:?}");
    }

    #[test]
    fn all_directions_and_shapes() {
        let o = gp(10, 8);
        for (dx, dy) in [
            (1, 0),
            (1, -1),
            (0, -1),
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            for (a, b) in [(3, 3), (5, 0), (5, 2), (2, 5)] {
                let to = gp(o.x + dx * a, o.y + dy * b);
                if to == o {
                    continue;
                }
                for bend in [Bend::StraightFirst, Bend::DiagonalFirst] {
                    check_walk(o, to, bend);
                }
            }
        }
    }

    #[test]
    fn straight_first_and_diagonal_first_examples() {
        let (a, b) = (gp(0, 0), gp(3, 1));
        assert_eq!(
            lay(a, b, Bend::StraightFirst),
            vec![gp(0, 0), gp(1, 0), gp(2, 0), gp(3, 1)]
        );
        assert_eq!(
            lay(a, b, Bend::DiagonalFirst),
            vec![gp(0, 0), gp(1, 1), gp(2, 1), gp(3, 1)]
        );
        assert_eq!(
            lay(b, a, Bend::DiagonalFirst),
            vec![gp(3, 1), gp(2, 0), gp(1, 0), gp(0, 0)]
        );
        assert_eq!(
            lay(gp(5, 5), gp(5, 1), Bend::DiagonalFirst),
            lay(gp(5, 5), gp(5, 1), Bend::StraightFirst)
        );
        assert_eq!(
            lay(gp(1, 1), gp(4, 4), Bend::DiagonalFirst),
            vec![gp(1, 1), gp(2, 2), gp(3, 3), gp(4, 4)]
        );
    }

    #[test]
    fn tile_bits_insert_once() {
        let mut b = TileBits::EMPTY;
        for i in 0..1024 {
            assert!(b.insert(i), "{i}");
            assert!(!b.insert(i), "{i}");
        }
        assert!((0..1024).all(|i| !b.insert(i)));
    }

    #[test]
    #[should_panic]
    fn tile_bits_reject_index_1024() {
        let mut b = TileBits::EMPTY;
        b.insert(1024);
    }

    #[test]
    fn bends_differ_when_both_parts_exist() {
        let (a, b) = (gp(4, 4), gp(1, 9));
        assert_ne!(
            lay(a, b, Bend::StraightFirst),
            lay(a, b, Bend::DiagonalFirst)
        );
        let mut rev = lay(b, a, Bend::StraightFirst);
        rev.reverse();
        assert_ne!(rev, lay(a, b, Bend::StraightFirst));
    }

    fn config() -> ProptestConfig {
        let mut c = ProptestConfig {
            failure_persistence: None,
            ..ProptestConfig::default()
        };
        if matches!(c.rng_seed, proptest::test_runner::RngSeed::Random) {
            c.rng_seed = proptest::test_runner::RngSeed::Fixed(1);
        }
        c
    }

    proptest! {
        #![proptest_config(config())]
        #[test]
        fn walk_properties(ax in 0..map_w(), ay in 0..map_h(), bx in 0..map_w(), by in 0..map_h(), diag in any::<bool>()) {
            let (a, b) = (gp(ax, ay), gp(bx, by));
            prop_assume!(a != b);
            check_walk(a, b, if diag { Bend::DiagonalFirst } else { Bend::StraightFirst });
        }
    }

    fn builtin_map() -> (u8, u8) {
        let b = crate::balance::BalanceTable::builtin();
        (b.spec().map.width, b.spec().map.height)
    }

    fn map_w() -> i32 {
        i32::from(builtin_map().0)
    }

    fn map_h() -> i32 {
        i32::from(builtin_map().1)
    }

    #[test]
    fn or_and_get() {
        let (w, h) = builtin_map();
        let mut g = TrackGrid::new(w, h);
        let cells = usize::from(w) * usize::from(h);
        assert_eq!(g.masks.len(), cells);
        let p = GridPos {
            x: map_w() - 1,
            y: map_h() - 1,
        };
        g.or(p, Dir8::W);
        g.or(p, Dir8::N);
        assert!(g.get(p).contains(Dir8::W) && g.get(p).contains(Dir8::N));
        assert_eq!(g.index(p), cells - 1);
        assert!(g.get(GridPos { x: 0, y: 0 }).is_empty());
    }

    #[test]
    fn index_panics_only_outside_the_map() {
        let (w, h) = builtin_map();
        let g = TrackGrid::new(w, h);
        let panics = |x, y| std::panic::catch_unwind(|| g.index(gp(x, y))).is_err();
        let (w, h) = (map_w(), map_h());
        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            assert!(!panics(x, y), "({x}, {y}) はマップの中");
        }
        for (x, y) in [(w, 0), (0, h), (-1, 0), (0, -1), (w, h - 1)] {
            assert!(panics(x, y), "({x}, {y}) はマップの外");
        }
    }
}
