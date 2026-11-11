//! 44 バイトのヘッダの WAV(16 ビット、モノラル、22050Hz)と FNV-1a 64。

use crate::tables::SAMPLE_RATE;

/// WAV のヘッダの大きさ。
pub const WAV_HEADER_BYTES: usize = 44;

/// サンプル列を WAV のファイル全体のバイト列にする。
pub fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(WAV_HEADER_BYTES + samples.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVE");
    b.extend_from_slice(b"fmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    b.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

/// FNV-1a 64(10 §9.2 と同じ関数)。
pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], i: usize) -> u16 {
        u16::from_le_bytes([b[i], b[i + 1]])
    }

    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }

    #[test]
    fn a10_header() {
        let b = wav_bytes(&[1, -2, i16::MAX]);
        assert_eq!(b.len(), 44 + 6);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32_at(&b, 4), 36 + 6);
        assert_eq!(&b[8..12], b"WAVE");
        assert_eq!(&b[12..16], b"fmt ");
        assert_eq!(u32_at(&b, 16), 16);
        assert_eq!(u16_at(&b, 20), 1);
        assert_eq!(u16_at(&b, 22), 1);
        assert_eq!(u32_at(&b, 24), 22050);
        assert_eq!(u32_at(&b, 28), 44100);
        assert_eq!(u16_at(&b, 32), 2);
        assert_eq!(u16_at(&b, 34), 16);
        assert_eq!(&b[36..40], b"data");
        assert_eq!(u32_at(&b, 40), 6);
        assert_eq!(&b[44..], &[1, 0, 0xFE, 0xFF, 0xFF, 0x7F]);
        assert_eq!(wav_bytes(&[]).len(), WAV_HEADER_BYTES);
    }

    #[test]
    fn fnv_known_values() {
        assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv64(b"foobar"), 0x8594_4171_f739_67e8);
    }
}
