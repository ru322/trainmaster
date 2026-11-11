//! 新しいゲームのシード [02 D24、16 §1.11、APP-D-05]。

use log::warn;
use tm_platform_api::MonoTime;

/// シードの決め方。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedPolicy {
    /// OS の乱数に単調時計を混ぜる(既定)。
    Os,
    /// 開発用: そのまま使う(混ぜない)。
    Fixed(u64),
    /// テスト用: 順に使い、尽きたら先頭へ。
    Script(Vec<u64>),
}

/// 64 ビットの乱数源。失敗は理由の文。
pub type EntropyFn = fn() -> Result<u64, String>;

/// `StartNewGame` のたびにシードを 1 つ出す。
#[derive(Clone, Debug)]
pub struct SeedSource {
    policy: SeedPolicy,
    script_pos: usize,
    entropy: EntropyFn,
}

impl SeedSource {
    /// OS の乱数(`getrandom`)を使う。
    pub fn new(policy: SeedPolicy) -> Self {
        Self::with_entropy(policy, os_entropy)
    }

    /// 乱数源を差し替えて作る(テスト用)。
    pub fn with_entropy(policy: SeedPolicy, entropy: EntropyFn) -> Self {
        SeedSource {
            policy,
            script_pos: 0,
            entropy,
        }
    }

    /// 次のシード。`now` は決定を押したフレームの時刻。
    pub fn next_seed(&mut self, now: MonoTime) -> u64 {
        match &self.policy {
            SeedPolicy::Fixed(s) => *s,
            SeedPolicy::Script(v) => {
                let s = if v.is_empty() {
                    0
                } else {
                    v[self.script_pos % v.len()]
                };
                self.script_pos += 1;
                s
            }
            SeedPolicy::Os => match (self.entropy)() {
                Ok(r) => mix(r, now),
                Err(e) => {
                    warn!("OS の乱数が使えない: {e}。時計から作る");
                    let s = splitmix64(now.0 ^ splitmix64(self.script_pos as u64 + 1));
                    self.script_pos += 1;
                    s
                }
            },
        }
    }
}

fn os_entropy() -> Result<u64, String> {
    getrandom::u64().map_err(|e| e.to_string())
}

/// OS の乱数 `r` に、全ビットに広げた単調時計を混ぜる。
pub fn mix(r: u64, now: MonoTime) -> u64 {
    r ^ splitmix64(now.0)
}

/// SplitMix64 の 1 段(定数は標準の値)。
pub fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
