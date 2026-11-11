//! 音のデータ。WAV を検査して `&[i16]` にする。生成物。生成元は `tm-audiogen`(`assets/audio`)。

use std::sync::OnceLock;

use tm_platform_api::{AssetError, SoundData, SoundId, SoundKind};

#[cfg(target_endian = "big")]
compile_error!(
    "WAV はリトルエンディアンのまま読むので、ビッグエンディアンのターゲットには対応しない"
);

const WAV_HEADER: usize = 44;
const SAMPLE_RATE: u32 = 22_050;

/// 2 バイト境界にそろえて埋め込むラッパー。`&[i16]` への変換に使う。
#[repr(C, align(2))]
pub struct AlignedWav<B: ?Sized>(pub B);

/// 埋め込んだ 1 つの音。`wav` は 44 バイトのヘッダを含むファイル全体。
pub struct SoundEntry {
    /// 名前(`se_coin` など)。
    pub name: &'static str,
    /// 種類。
    pub kind: SoundKind,
    /// ループの戻り先。`Bgm` だけ。
    pub loop_start: Option<u32>,
    /// WAV のバイト列。
    pub wav: &'static AlignedWav<[u8]>,
}

#[allow(missing_docs)]
mod generated {
    use super::{AlignedWav, SoundEntry};
    use tm_platform_api::{SoundId, SoundKind};

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/generated/audio/sounds.rs"
    ));
}

pub use generated::*;

fn wav_error(entry: &SoundEntry, reason: &'static str) -> AssetError {
    AssetError::Wav {
        name: entry.name,
        reason,
    }
}

fn le_u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// WAV のヘッダを検査し、後ろのサンプル列を `&[i16]` で返す。コピーしない。
pub fn pcm16(entry: &'static SoundEntry) -> Result<&'static [i16], AssetError> {
    let bytes: &'static [u8] = &entry.wav.0;
    let fail = |reason| Err(wav_error(entry, reason));
    if bytes.len() < WAV_HEADER {
        return fail("ヘッダより短い");
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return fail("RIFF/WAVE でない");
    }
    if &bytes[12..16] != b"fmt " || le_u32(bytes, 16) != 16 {
        return fail("fmt チャンクが 16 バイトでない");
    }
    if le_u16(bytes, 20) != 1 {
        return fail("PCM でない");
    }
    if le_u16(bytes, 22) != 1 {
        return fail("モノラルでない");
    }
    if le_u32(bytes, 24) != SAMPLE_RATE {
        return fail("22050Hz でない");
    }
    if le_u16(bytes, 34) != 16 {
        return fail("16 ビットでない");
    }
    if &bytes[36..40] != b"data" {
        return fail("data チャンクが 36 バイト目にない");
    }
    let data = &bytes[WAV_HEADER..];
    if le_u32(bytes, 40) as usize != data.len() || !data.len().is_multiple_of(2) {
        return fail("data のサイズが全体 - 44 の偶数と一致しない");
    }
    bytemuck::try_cast_slice::<u8, i16>(data).or_else(|_| fail("サンプル列が 2 バイト境界にない"))
}

static SOUNDS: OnceLock<Result<Box<[SoundData]>, AssetError>> = OnceLock::new();

/// 全音の `SoundData` の表。最初の呼び出しで 1 回だけ作る。
pub fn sound_data() -> Result<&'static [SoundData], AssetError> {
    SOUNDS
        .get_or_init(|| {
            SOUND_ENTRIES
                .iter()
                .map(|e| {
                    Ok(SoundData {
                        name: e.name,
                        kind: e.kind,
                        samples: pcm16(e)?,
                        loop_start: e.loop_start,
                    })
                })
                .collect()
        })
        .as_ref()
        .map(|table| &**table)
        .map_err(Clone::clone)
}

/// 番号から名前を引く。範囲外は `None`。
pub fn name(id: SoundId) -> Option<&'static str> {
    SOUND_ENTRIES.get(usize::from(id.0)).map(|e| e.name)
}
