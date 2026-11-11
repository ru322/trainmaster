use super::*;

/// 16 §2.7 の固定シード。
const SEED_A: u64 = 1;
const SEED_B: u64 = 42;
const SEED_C: u64 = 0xDEAD_BEEF_0123_4567;

fn first_words(seed: u64, stream: u64, n: usize) -> Vec<u32> {
    let mut rng = StreamRng::new(seed, stream);
    (0..n).map(|_| rng.next_u32()).collect()
}

fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

/// 仕様どおりの ChaCha8(djb 版: 64 ビットのブロックカウンタ = 語 12〜13、ストリーム = 語 14〜15)の 1 ブロック。
fn reference_block(seed: u64, stream: u64, counter: u64) -> [u32; 16] {
    let mut init = [0u32; 16];
    init[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]);
    init[4] = seed as u32;
    init[5] = (seed >> 32) as u32;
    init[12] = counter as u32;
    init[13] = (counter >> 32) as u32;
    init[14] = stream as u32;
    init[15] = (stream >> 32) as u32;
    let mut s = init;
    for _ in 0..4 {
        quarter_round(&mut s, 0, 4, 8, 12);
        quarter_round(&mut s, 1, 5, 9, 13);
        quarter_round(&mut s, 2, 6, 10, 14);
        quarter_round(&mut s, 3, 7, 11, 15);
        quarter_round(&mut s, 0, 5, 10, 15);
        quarter_round(&mut s, 1, 6, 11, 12);
        quarter_round(&mut s, 2, 7, 8, 13);
        quarter_round(&mut s, 3, 4, 9, 14);
    }
    for (o, i) in s.iter_mut().zip(init) {
        *o = o.wrapping_add(i);
    }
    s
}

/// 02 V11・11 VS-01: 語はキーストリームの順に 1 つずつ出る(バッファの境界をまたいでも)。
#[test]
fn matches_reference_chacha8_keystream() {
    for seed in [SEED_A, SEED_B, SEED_C] {
        for stream in [STREAM_STATION_SPAWN, STREAM_PASSENGER_DEST, u64::MAX] {
            let expected: Vec<u32> = (0..20)
                .flat_map(|b| reference_block(seed, stream, b))
                .collect();
            assert_eq!(
                first_words(seed, stream, expected.len()),
                expected,
                "seed {seed:#x} stream {stream}"
            );
        }
    }
}

#[test]
fn counts_words_and_keeps_stream() {
    let mut rng = StreamRng::new(SEED_A, STREAM_PASSENGER_DEST);
    assert_eq!((rng.stream(), rng.words()), (STREAM_PASSENGER_DEST, 0));
    for _ in 0..70 {
        rng.next_u32();
    }
    assert_eq!(rng.words(), 70);
    let copy = rng.clone();
    assert_eq!(copy, rng);
    assert_eq!(
        RngStreams::new(SEED_A).passenger_dest,
        StreamRng::new(SEED_A, STREAM_PASSENGER_DEST)
    );
}

/// 10 VT-5: n 語を取り出した後の `get_word_pos` が n で、`resume(n)` の続きの出力が一致する。
#[test]
fn resume_continues_after_n_words() {
    for n in [0u64, 1, 15, 16, 17, 63, 64, 65, 1_000_000] {
        let mut rng = StreamRng::new(SEED_C, STREAM_PASSENGER_DEST);
        for _ in 0..n {
            rng.next_u32();
        }
        assert_eq!(rng.inner.get_word_pos(), u128::from(n));
        let mut resumed = StreamRng::resume(SEED_C, STREAM_PASSENGER_DEST, n);
        assert_eq!(resumed, rng, "n = {n}");
        assert_eq!(
            (resumed.words(), resumed.stream()),
            (n, STREAM_PASSENGER_DEST)
        );
        for _ in 0..80 {
            assert_eq!(resumed.next_u32(), rng.next_u32(), "n = {n}");
        }
        assert_eq!(resumed.words(), n + 80);
    }
}

/// ゴールデン(11 §11.3): SEED_A の各ストリームの最初の 64 語。
#[test]
fn golden_first_64_words() {
    assert_eq!(
        first_words(SEED_A, STREAM_STATION_SPAWN, 64),
        GOLDEN_STATION_SPAWN
    );
    assert_eq!(
        first_words(SEED_A, STREAM_PASSENGER_DEST, 64),
        GOLDEN_PASSENGER_DEST
    );
}

/// ゴールデン(11 §11.3): SEED_A の駅の位置のストリームで `below(n)`、n = 1, 2, …, 64 を順に引いた値。
#[test]
fn golden_below() {
    let mut rng = StreamRng::new(SEED_A, STREAM_STATION_SPAWN);
    let got: Vec<u32> = (1..=64).map(|n| below(&mut rng, n)).collect();
    assert_eq!(got, GOLDEN_BELOW);
    assert_eq!(rng.words(), GOLDEN_BELOW_WORDS);
}

#[test]
fn below_one_is_always_zero() {
    let mut rng = StreamRng::new(SEED_B, STREAM_STATION_SPAWN);
    for i in 1..=100 {
        assert_eq!(below(&mut rng, 1), 0);
        assert_eq!(rng.words(), i);
    }
}

/// 棄却がよく起きる n で、捨てた語の数が仕様(`x < 2^32 mod n` を捨てる)どおり。
#[test]
fn below_rejects_per_spec() {
    let n = 0x8000_0001_u32;
    let threshold = n.wrapping_neg() % n;
    assert_eq!(threshold, 0x7fff_ffff);
    let mut rng = StreamRng::new(SEED_C, STREAM_PASSENGER_DEST);
    let mut raw = StreamRng::new(SEED_C, STREAM_PASSENGER_DEST);
    let mut rejected = 0;
    for _ in 0..200 {
        let got = below(&mut rng, n);
        let expected = loop {
            let x = raw.next_u32();
            if x >= threshold {
                break x % n;
            }
            rejected += 1;
        };
        assert_eq!(got, expected);
        assert!(got < n);
        assert_eq!(rng.words(), raw.words());
    }
    assert!(rejected > 0);
    assert_eq!(rng.words(), 200 + rejected);
}

#[test]
#[should_panic(expected = "below: n = 0")]
fn below_zero_panics() {
    below(&mut StreamRng::new(SEED_A, STREAM_STATION_SPAWN), 0);
}

const GOLDEN_STATION_SPAWN: [u32; 64] = [
    0x8c6fea14, 0x1a8125d4, 0xac499192, 0xd167e651, 0xa09a4bb5, 0xa35d1d16, 0xa9d282dd, 0x53d7c3fc,
    0x03f19033, 0x5bdfcff7, 0x687c686f, 0xf8a497b9, 0xfa54a870, 0x898608c0, 0x6489e828, 0x2c2ddfdd,
    0x6c38c98a, 0xdf48c9ef, 0xade7f339, 0xfd75e0ee, 0x8b0566e9, 0x27f296d4, 0x9b1dd270, 0x460caa14,
    0xc75091c8, 0x2333d084, 0xa23f889c, 0x06b7307d, 0xd8bed875, 0x516c9c8d, 0x59a3d43d, 0x71aa0184,
    0x4d62a901, 0x5dfc7bed, 0x513a6abb, 0x597f151f, 0x2082b931, 0x34c0cf6c, 0xa48fd607, 0xeb72ec4b,
    0x3790854d, 0xdee18fb2, 0xcb1d7834, 0x1092baff, 0xcac400d5, 0x14b2e7a0, 0xbdc8b5d6, 0x38b6320f,
    0x682aab26, 0x37a8fc34, 0x15476df2, 0xe1810232, 0x4ee319b7, 0x1cf061be, 0x1291f9da, 0x957930cb,
    0xc9a5411d, 0xf73a52d5, 0x2b5e3342, 0x58d8c39c, 0xbc3ffcdb, 0x197b29f4, 0x4ca13241, 0x5cc69388,
];
const GOLDEN_PASSENGER_DEST: [u32; 64] = [
    0xd3f162fa, 0x604be002, 0xd11a0190, 0x1c1ed398, 0x75ec03b9, 0x680d4ac0, 0xd9a5b65a, 0x43085d08,
    0xa2162569, 0xfe6f2eb0, 0x8fee60c4, 0xe6b3f14a, 0xaca589df, 0x5b252baa, 0x5f6dba0b, 0xf1057c5c,
    0xace69ae9, 0x88f85b9b, 0xa2e6e277, 0xb6ed317c, 0x38215b4c, 0xb8b17c99, 0xe41f64e7, 0x971cc68e,
    0xfa12291c, 0xc59fe1ff, 0x006623ee, 0x029374c3, 0xe813407e, 0xff37f493, 0x65f91084, 0xe5552a9d,
    0xe0078094, 0x4b9a6e01, 0xc1bbafae, 0xb07d1758, 0xe5bdde5d, 0x7375d82e, 0x1c8bec11, 0x23104f7d,
    0x6df25eb6, 0xb66af40d, 0x7d1ae24d, 0x802908cc, 0x9eea41da, 0xde76a2e7, 0xcdac6673, 0x5423b333,
    0xf265b6c1, 0x1f9067c6, 0x21bf3215, 0xd46778f1, 0x3626535d, 0x0e8d68bb, 0x4704d9e4, 0x78a119ce,
    0xe66c2c39, 0x733acfce, 0x2048d87c, 0x4e9cd4be, 0x3f9d60e5, 0xe0cdde57, 0xee9bac94, 0x7b9fbdee,
];
const GOLDEN_BELOW: [u32; 64] = [
    0, 0, 2, 1, 0, 4, 0, 4, 4, 3, 2, 1, 7, 6, 14, 13, 10, 15, 12, 2, 20, 10, 13, 20, 9, 8, 16, 17,
    16, 21, 4, 4, 15, 25, 17, 31, 22, 8, 30, 35, 0, 36, 24, 7, 41, 28, 3, 15, 42, 42, 35, 30, 14,
    36, 50, 51, 34, 45, 12, 40, 32, 40, 19, 8,
];
const GOLDEN_BELOW_WORDS: u64 = 64;
