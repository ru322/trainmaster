use tm_numeric::BigNum;
use tm_pathfind::Cost;

use crate::time::{POS_UNITS_PER_TILE, TICKS_PER_SEC, Ticks};

/// お金(円)。
pub type Money = BigNum;

/// 経路コスト(秒 × 100)。
pub type RouteCost = Cost;

macro_rules! unit_type {
    ($(#[$meta:meta])* $name:ident, $inner:ty) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(pub $inner);

        impl $name {
            /// 0。
            pub const ZERO: $name = $name(0);

            /// 和。あふれたら飽和し、デバッグビルドでは止める(10 TD-21)。
            pub fn add_sat(self, rhs: $name) -> $name {
                let sum = self.0.checked_add(rhs.0);
                debug_assert!(sum.is_some(), concat!(stringify!($name), " の和があふれた"));
                $name(sum.unwrap_or(<$inner>::MAX))
            }

            /// 負にならない差。`rhs > self` ならデバッグビルドで止め、リリースでは 0。
            pub fn sub_floor(self, rhs: $name) -> $name {
                let diff = self.0.checked_sub(rhs.0);
                debug_assert!(diff.is_some(), concat!(stringify!($name), " の差が負になった"));
                $name(diff.unwrap_or(0))
            }

            /// 小さい方。
            pub fn min(self, rhs: $name) -> $name {
                $name(self.0.min(rhs.0))
            }
        }
    };
}

unit_type!(
    /// 人数(人)。
    People,
    u64
);
unit_type!(
    /// 1/1000 人(発生の累積器、発生頻度)。
    MilliPeople,
    u64
);
unit_type!(
    /// 速度(1/1000 マス/秒、`v‰`)。1 tick の移動量そのもの。
    SpeedMilli,
    u32
);
unit_type!(
    /// 千分率(魅力度 `A‰`)。
    Permille,
    u32
);
unit_type!(
    /// 1/4 倍(混雑率)。
    Quarter,
    u32
);
unit_type!(
    /// 両(両数、ホーム長)。
    Cars,
    u32
);
unit_type!(
    /// マス(チェビシェフ距離。斜めも 1)。
    Tiles,
    u32
);
unit_type!(
    /// 列車の位置と区間長(1/(1000 × `TICKS_PER_SEC`) マス)。
    PosUnits,
    u64
);
unit_type!(
    /// ミリ秒。
    Millis,
    u32
);

impl People {
    /// お金の計算に使う値(`BigNum::from_u64`)。
    pub fn to_money(self) -> Money {
        BigNum::from_u64(self.0)
    }
}

fn saturate_u64(v: u128) -> u64 {
    u64::try_from(v).unwrap_or(u64::MAX)
}

/// 四捨五入の割り算 `⌊(2n + d) / (2d)⌋`。
pub fn round_div(n: u128, d: u128) -> u128 {
    (2 * n + d) / (2 * d)
}

/// 切り上げの割り算 `(n + d − 1) / d`。
pub fn ceil_div(n: u128, d: u128) -> u128 {
    n.div_ceil(d)
}

/// 1 秒あたりの発生量 `⌊rate × a / 1000⌋`(10 §2.5)。
pub fn spawn_per_sec(rate: MilliPeople, a: Permille) -> MilliPeople {
    let v = u128::from(rate.0) * u128::from(a.0) / 1000;
    debug_assert!(v <= u128::from(u64::MAX), "spawn_per_sec があふれた");
    MilliPeople(saturate_u64(v))
}

/// 列車の定員 `⌊per_car × cars × crowd / 4⌋`。`u64` に飽和する。
pub fn train_capacity(per_car: People, cars: Cars, crowd: Quarter) -> People {
    People(saturate_u64(
        u128::from(per_car.0) * u128::from(cars.0) * u128::from(crowd.0) / 4,
    ))
}

/// 停車時間 `min(max, base + ⌈1000 × moved / rate⌉)`。`rate` は 1 以上。
pub fn dwell_ms(moved: People, rate: People, base: Millis, max: Millis) -> Millis {
    let v = u128::from(base.0) + ceil_div(1000 * u128::from(moved.0), u128::from(rate.0));
    Millis(u32::try_from(v.min(u128::from(max.0))).unwrap_or(max.0))
}

/// ミリ秒を tick に切り上げる `⌈ms × TICKS_PER_SEC / 1000⌉`。
pub fn ms_to_ticks(ms: Millis) -> Ticks {
    let v = (u64::from(ms.0) * u64::from(TICKS_PER_SEC)).div_ceil(1000);
    Ticks(u32::try_from(v).unwrap_or(u32::MAX))
}

/// 区間長 `d × POS_UNITS_PER_TILE`。
pub fn section_length(d: Tiles) -> PosUnits {
    PosUnits(u64::from(d.0) * POS_UNITS_PER_TILE)
}

/// 走行辺のコスト `max(1, round(N / D))`。`N = 10^5 × (1000·d + stop_s·v)`、`D = v × a` [02 §3.7]。
pub fn ride_cost(d: Tiles, v: SpeedMilli, a: Permille, stop_s: u32) -> RouteCost {
    let n = 100_000 * (1000 * u128::from(d.0) + u128::from(stop_s) * u128::from(v.0));
    let den = u128::from(v.0) * u128::from(a.0);
    Cost(saturate_u64(round_div(n, den).max(1)))
}

/// 運賃収入 `fare × riders`。
pub fn fare_income(riders: People, fare: Money) -> Money {
    fare.mul_small(riders.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ride_cost_matches_01_examples() {
        assert_eq!(
            ride_cost(Tiles(6), SpeedMilli(1000), Permille(1000), 2),
            Cost(800)
        );
        assert_eq!(
            ride_cost(Tiles(6), SpeedMilli(1000), Permille(3000), 2),
            Cost(267)
        );
        assert_eq!(
            ride_cost(Tiles(6), SpeedMilli(6000), Permille(1000), 2),
            Cost(300)
        );
        assert_eq!(
            ride_cost(Tiles(0), SpeedMilli(6000), Permille(3000), 0),
            Cost(1)
        );
    }

    #[test]
    fn rounding_helpers() {
        assert_eq!(round_div(5, 2), 3);
        assert_eq!(round_div(4, 3), 1);
        assert_eq!(ceil_div(7, 7), 1);
        assert_eq!(ceil_div(8, 7), 2);
    }

    #[test]
    fn spawn_and_capacity() {
        assert_eq!(
            spawn_per_sec(MilliPeople(500), Permille(1000)),
            MilliPeople(500)
        );
        assert_eq!(
            spawn_per_sec(MilliPeople(500), Permille(1200)),
            MilliPeople(600)
        );
        assert_eq!(train_capacity(People(10), Cars(1), Quarter(4)), People(10));
        assert_eq!(train_capacity(People(10), Cars(3), Quarter(5)), People(37));
        assert_eq!(
            train_capacity(People(u64::MAX), Cars(10), Quarter(10)),
            People(u64::MAX)
        );
    }

    #[test]
    fn dwell_and_ticks() {
        let (base, max) = (Millis(1000), Millis(10_000));
        assert_eq!(dwell_ms(People(0), People(5), base, max), Millis(1000));
        assert_eq!(dwell_ms(People(1), People(5), base, max), Millis(1200));
        assert_eq!(dwell_ms(People(1), People(3), base, max), Millis(1334));
        assert_eq!(dwell_ms(People(1_000), People(5), base, max), max);
        assert_eq!(ms_to_ticks(Millis(1000)), Ticks(60));
        assert_eq!(ms_to_ticks(Millis(1334)), Ticks(81));
        assert_eq!(ms_to_ticks(Millis(10_000)), Ticks(600));
        assert_eq!(section_length(Tiles(6)), PosUnits(360_000));
    }

    #[test]
    fn saturating_unit_ops() {
        assert_eq!(People(3).add_sat(People(4)), People(7));
        assert_eq!(People(5).sub_floor(People(2)), People(3));
        assert_eq!(People(5).min(People(2)), People(2));
        assert_eq!(
            fare_income(People(3), BigNum::from_u64(10)),
            BigNum::from_u64(30)
        );
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn release_saturates() {
        assert_eq!(People(u64::MAX).add_sat(People(1)), People(u64::MAX));
        assert_eq!(People(1).sub_floor(People(2)), People::ZERO);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "People の和があふれた")]
    fn debug_stops_on_overflow() {
        let _ = People(u64::MAX).add_sat(People(1));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Cars の差が負になった")]
    fn debug_stops_on_underflow() {
        let _ = Cars(1).sub_floor(Cars(2));
    }
}
