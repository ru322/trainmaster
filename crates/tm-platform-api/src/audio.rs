//! 音の命令と音のデータ [10 §8.2]。

use core::fmt;

/// 音の番号。`tm-audiogen` が名前の辞書順に振る [02 D33]。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SoundId(pub u16);

/// 音の種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundKind {
    /// 効果音。
    Se,
    /// BGM。
    Bgm,
    /// ジングル。
    Jingle,
}

/// 音量を分けるバス。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioBus {
    /// BGM とジングル。
    Bgm,
    /// 効果音。
    Se,
}

/// ミキサーへの命令。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCmd {
    /// 効果音を鳴らす。
    PlaySe(SoundId),
    /// その ID のうちいちばん古い 1 つを止める。
    StopSe(SoundId),
    /// BGM を鳴らす。
    PlayBgm {
        /// BGM の ID。
        id: SoundId,
        /// 前の BGM とのクロスフェードの長さ。
        crossfade_ms: u16,
    },
    /// BGM を止める。
    StopBgm {
        /// フェードアウトの長さ。
        fade_ms: u16,
    },
    /// ジングルを鳴らす。
    PlayJingle(SoundId),
    /// BGM のダッキングの入り切り。
    SetBgmDuck(bool),
    /// バスの音量を決める。
    SetVolume {
        /// 対象のバス。
        bus: AudioBus,
        /// 音量。`0..=10`。
        level: u8,
    },
}

/// 埋め込んだ音 1 つ。サンプルは 16 ビット、モノラル、22050Hz。
#[derive(Clone, Copy)]
pub struct SoundData {
    /// 名前(`se_coin` など)。
    pub name: &'static str,
    /// 種類。
    pub kind: SoundKind,
    /// サンプル列。
    pub samples: &'static [i16],
    /// ループの戻り先。`Bgm` だけが持てる。
    pub loop_start: Option<u32>,
}

impl fmt::Debug for SoundData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SoundData")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("samples", &self.samples.len())
            .field("loop_start", &self.loop_start)
            .finish()
    }
}
