//! 用途別の乱数ストリーム(02 §3.3)。ChaCha8 は `rand_chacha` の生の出力だけを使う(02 V11)。

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};

/// 駅の位置(`Game::new` の中で使い切る)。
pub(crate) const STREAM_STATION_SPAWN: u64 = 1;
/// 乗客の目的駅。
pub(crate) const STREAM_PASSENGER_DEST: u64 = 2;
/// 転生の次のシード。
pub(crate) const STREAM_REBIRTH: u64 = 3;

/// 1 つの用途のストリーム。`words` は取り出した 32 ビット語の数(state_hash に入る)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StreamRng {
    inner: ChaCha8Rng,
    stream: u64,
    words: u64,
}

impl StreamRng {
    /// 鍵 = シードの LE 8 バイト + 0 × 24。`from_seed` → `set_stream` → 取り出しの順に固定 [02 §3.3]。
    pub fn new(master_seed: u64, stream: u64) -> Self {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&master_seed.to_le_bytes());
        let mut inner = ChaCha8Rng::from_seed(key);
        inner.set_stream(stream);
        StreamRng {
            inner,
            stream,
            words: 0,
        }
    }

    /// `new` の後に語の位置を `words` に合わせる(10 TD-45)。
    pub fn resume(master_seed: u64, stream: u64, words: u64) -> Self {
        let mut rng = StreamRng::new(master_seed, stream);
        rng.inner.set_word_pos(u128::from(words));
        rng.words = words;
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        self.words += 1;
        self.inner.next_u32()
    }

    pub fn stream(&self) -> u64 {
        self.stream
    }

    pub fn words(&self) -> u64 {
        self.words
    }
}

/// `0..n` の一様な整数(11 SIM-D-10)。`2^32 mod n` より小さい語を捨てる棄却サンプリング。`n ≥ 1`。
pub(crate) fn below(rng: &mut StreamRng, n: u32) -> u32 {
    assert!(n >= 1, "below: n = 0");
    let threshold = n.wrapping_neg() % n;
    loop {
        let x = rng.next_u32();
        if x >= threshold {
            return x % n;
        }
    }
}

/// 状態に持つストリーム。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RngStreams {
    pub passenger_dest: StreamRng,
}

impl RngStreams {
    pub fn new(master_seed: u64) -> Self {
        RngStreams {
            passenger_dest: StreamRng::new(master_seed, STREAM_PASSENGER_DEST),
        }
    }
}

#[cfg(test)]
mod tests;
