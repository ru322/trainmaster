use serde::{Deserialize, Serialize};

use crate::ids::StationId;
use crate::units::Tiles;

/// マスの座標。x は右、y は下、原点は左上のマス(10 TD-25)。`Ord` は (x, y) の辞書順。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GridPos {
    /// 列(0 が左端)。
    pub x: i32,
    /// 行(0 が上端)。
    pub y: i32,
}

impl GridPos {
    /// チェビシェフ距離(斜めも 1)。
    pub fn chebyshev(self, other: GridPos) -> u32 {
        self.x.abs_diff(other.x).max(self.y.abs_diff(other.y))
    }
}

/// 8 方向。値はビット番号。y は下向きなので `N` は y − 1。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Dir8 {
    /// 右。
    E = 0,
    /// 右上。
    NE,
    /// 上。
    N,
    /// 左上。
    NW,
    /// 左。
    W,
    /// 左下。
    SW,
    /// 下。
    S,
    /// 右下。
    SE,
}

impl Dir8 {
    /// ビット番号の順のすべての方向。
    pub const ALL: [Dir8; 8] = [
        Dir8::E,
        Dir8::NE,
        Dir8::N,
        Dir8::NW,
        Dir8::W,
        Dir8::SW,
        Dir8::S,
        Dir8::SE,
    ];

    /// 1 歩の (dx, dy)。
    pub const fn delta(self) -> (i32, i32) {
        match self {
            Dir8::E => (1, 0),
            Dir8::NE => (1, -1),
            Dir8::N => (0, -1),
            Dir8::NW => (-1, -1),
            Dir8::W => (-1, 0),
            Dir8::SW => (-1, 1),
            Dir8::S => (0, 1),
            Dir8::SE => (1, 1),
        }
    }

    /// 逆向き。
    pub const fn opposite(self) -> Dir8 {
        Dir8::ALL[(self as usize + 4) % 8]
    }

    /// (dx, dy) の方向。どちらも −1..=1 で、(0, 0) でないこと。そうでなければ panic する。
    pub fn from_delta(dx: i32, dy: i32) -> Dir8 {
        Dir8::ALL
            .into_iter()
            .find(|d| d.delta() == (dx, dy))
            .expect("Dir8::from_delta: 隣のマスへの差ではない")
    }
}

/// 1 マスの線路のつながり。ビット i が立っていれば `Dir8` の値 i の向きへ線路が出ている。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackMask(pub u8);

impl TrackMask {
    /// その向きの線路があるか。
    pub fn contains(self, d: Dir8) -> bool {
        self.0 & (1 << d as u8) != 0
    }

    /// その向きの線路を足す。
    pub fn insert(&mut self, d: Dir8) {
        self.0 |= 1 << d as u8;
    }

    /// 線路がないか。
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// 隣り合う 2 駅の間の線路の曲がり方 [01 §6.3 D-14]。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bend {
    /// まっすぐが先(同点のときの既定)。
    StraightFirst,
    /// 斜めが先。
    DiagonalFirst,
}

impl Bend {
    /// 逆向きにたどったときの同じ線路の曲がり方(10 TD-23)。
    pub const fn reversed(self) -> Bend {
        match self {
            Bend::StraightFirst => Bend::DiagonalFirst,
            Bend::DiagonalFirst => Bend::StraightFirst,
        }
    }
}

/// 2 駅の間の線路。`tiles` は `from` の駅のマスから `to` の駅のマスまで(両端を含む)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackSegment {
    /// 始点の駅。
    pub from: StationId,
    /// 終点の駅。
    pub to: StationId,
    /// `from` → `to` の向きの曲がり方。
    pub bend: Bend,
    /// 長さ `max(|dx|, |dy|)`。
    pub len: Tiles,
    /// 通るマス(`len + 1` 個)。
    pub tiles: Vec<GridPos>,
}

/// マップ上の点(`PosUnits`)。マス (x, y) の中心が (x·U, y·U)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapPoint {
    /// 横。
    pub x: i64,
    /// 縦。
    pub y: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir8_delta_round_trip() {
        for d in Dir8::ALL {
            let (dx, dy) = d.delta();
            assert_eq!(Dir8::from_delta(dx, dy), d);
            let (ox, oy) = d.opposite().delta();
            assert_eq!((ox, oy), (-dx, -dy));
            assert_eq!(d.opposite().opposite(), d);
        }
        assert_eq!(Dir8::N.delta(), (0, -1));
    }

    #[test]
    fn track_mask_bits() {
        let mut m = TrackMask::default();
        assert!(m.is_empty());
        m.insert(Dir8::NW);
        assert_eq!(m, TrackMask(1 << 3));
        assert!(m.contains(Dir8::NW));
        assert!(!m.contains(Dir8::SE));
    }

    #[test]
    fn bend_and_distance() {
        assert_eq!(Bend::StraightFirst.reversed(), Bend::DiagonalFirst);
        assert_eq!(
            Bend::DiagonalFirst.reversed().reversed(),
            Bend::DiagonalFirst
        );
        assert_eq!(GridPos { x: 0, y: 0 }.chebyshev(GridPos { x: 3, y: -5 }), 5);
    }
}
