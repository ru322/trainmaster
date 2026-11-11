use serde::{Deserialize, Serialize};

/// 1 秒あたりの tick 数 [02 D6、10 TD-02]。
pub const TICKS_PER_SEC: u32 = 60;

/// 列車の位置の単位: 1 マス = 1000 × `TICKS_PER_SEC` 単位 [02 D25]。
pub const POS_UNITS_PER_TILE: u64 = 1000 * TICKS_PER_SEC as u64;

/// 絶対時刻(tick 番号)。最初に処理する tick が `Tick(1)`(10 TD-01)。
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Tick(pub u64);

impl Tick {
    /// 作った直後の時刻。
    pub const ZERO: Tick = Tick(0);

    /// 次の tick。
    pub fn next(self) -> Tick {
        Tick(self.0 + 1)
    }

    /// 毎秒の処理をする tick か(`Tick(60)`、`Tick(120)`、…)。
    pub fn is_second_boundary(self) -> bool {
        self.0 > 0 && self.0.is_multiple_of(u64::from(TICKS_PER_SEC))
    }
}

/// tick の長さ(停車時間の残りなど)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ticks(pub u32);

impl Ticks {
    /// 長さ 0。
    pub const ZERO: Ticks = Ticks(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_boundary() {
        assert!(!Tick::ZERO.is_second_boundary());
        assert!(!Tick(59).is_second_boundary());
        assert!(Tick(60).is_second_boundary());
        assert!(!Tick(61).is_second_boundary());
        assert!(Tick(120).is_second_boundary());
        assert_eq!(Tick::ZERO.next(), Tick(1));
        assert_eq!(POS_UNITS_PER_TILE, 60_000);
    }
}
