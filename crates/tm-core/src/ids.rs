use core::fmt;

use serde::{Deserialize, Serialize};

macro_rules! entity_id {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl $name {
            /// `Vec` の添字。
            pub fn index(self) -> usize {
                self.0 as usize
            }

            /// 添字から作る。`u32` に収まらなければ panic する(上限は `BalanceTable` の検証で保証)。
            pub fn from_index(i: usize) -> Self {
                Self(u32::try_from(i).expect(concat!(stringify!($name), " の添字が u32 を超えた")))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

entity_id!(
    /// 駅。出現順に 0 から連番で、`Vec<Station>` の添字と一致する [02 §4.2]。
    StationId,
    "S"
);
entity_id!(
    /// 路線。作成順に 0 から連番で、`Vec<Line>` の添字。
    LineId,
    "L"
);
entity_id!(
    /// 列車。作成順に 0 から連番(全路線で通し番号)で、`Vec<Train>` の添字 [02 D9]。
    TrainId,
    "T"
);
entity_id!(
    /// 転生の進化。設定の `nodes` の並びの添字。
    EvolutionId,
    "E"
);

/// パラメータのレベル。0 から上がるだけ。最大値は `BalanceTable` の検証で 255 未満。
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Level(pub u8);

impl Level {
    /// レベル 0。
    pub const ZERO: Level = Level(0);

    /// 1 つ上のレベル。255 で panic する(検証済みの表では到達しない)。
    pub fn next(self) -> Level {
        Level(self.0.checked_add(1).expect("Level が 255 を超えた"))
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lv{}", self.0)
    }
}

/// 会社レベル。`0..=max_level`(01 では 24)。
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct CompanyLevel(pub u8);

impl CompanyLevel {
    /// 会社レベル 0。
    pub const ZERO: CompanyLevel = CompanyLevel(0);

    /// 1 つ上の会社レベル。255 で panic する(検証済みの表では到達しない)。
    pub fn next(self) -> CompanyLevel {
        CompanyLevel(self.0.checked_add(1).expect("CompanyLevel が 255 を超えた"))
    }
}

impl fmt::Display for CompanyLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_short() {
        assert_eq!(StationId(3).to_string(), "S3");
        assert_eq!(LineId(1).to_string(), "L1");
        assert_eq!(TrainId(5).to_string(), "T5");
        assert_eq!(Level(7).to_string(), "Lv7");
        assert_eq!(CompanyLevel(2).to_string(), "C2");
    }

    #[test]
    fn index_round_trip() {
        assert_eq!(StationId::from_index(18).index(), 18);
        assert_eq!(Level::ZERO.next(), Level(1));
        assert_eq!(CompanyLevel::ZERO.next(), CompanyLevel(1));
    }

    #[test]
    #[should_panic(expected = "Level が 255 を超えた")]
    fn level_next_panics_at_255() {
        let _ = Level(255).next();
    }
}
