//! 単調時計 [10 §8.2]。

/// 起動からの経過時間(マイクロ秒)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonoTime(pub u64);

impl MonoTime {
    /// `earlier` からの経過マイクロ秒。`earlier` が後なら 0。
    pub fn saturating_since(self, earlier: MonoTime) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}
