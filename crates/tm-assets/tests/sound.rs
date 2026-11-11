use tm_assets::SoundId;
use tm_assets::sound::{AlignedWav, SOUND_COUNT, SOUND_ENTRIES, SoundEntry, pcm16};
use tm_platform_api::{AssetError, SoundKind};

#[test]
fn every_embedded_wav_decodes_to_samples() {
    assert_eq!(SOUND_ENTRIES.len(), SOUND_COUNT);
    for e in &SOUND_ENTRIES {
        let samples = pcm16(e).unwrap_or_else(|err| panic!("{}: {err:?}", e.name));
        assert_eq!(samples.len(), (e.wav.0.len() - 44) / 2, "{}", e.name);
        assert!(!samples.is_empty());
        match (e.kind, e.loop_start) {
            (SoundKind::Bgm, Some(start)) => assert!((start as usize) < samples.len()),
            (SoundKind::Bgm, None) => panic!("{}: Bgm にループがない", e.name),
            (_, loop_start) => assert!(loop_start.is_none(), "{}", e.name),
        }
    }
}

#[test]
fn samples_are_borrowed_from_the_embedded_bytes() {
    let e = &SOUND_ENTRIES[usize::from(tm_assets::sound::SE_COIN.0)];
    let samples = pcm16(e).unwrap();
    assert_eq!(samples.as_ptr() as usize, e.wav.0[44..].as_ptr() as usize);
    let first = i16::from_le_bytes([e.wav.0[44], e.wav.0[45]]);
    assert_eq!(samples[0], first);
}

#[test]
fn ids_match_names_and_kinds_follow_prefixes() {
    assert_eq!(tm_assets::sound::SE_COIN, SoundId(11));
    assert_eq!(
        tm_assets::sound::name(tm_assets::sound::SE_COIN),
        Some("se_coin")
    );
    assert_eq!(tm_assets::sound::name(SoundId(u16::MAX)), None);
    for e in &SOUND_ENTRIES {
        let kind = match e.name.split('_').next() {
            Some("se") => SoundKind::Se,
            Some("bgm") => SoundKind::Bgm,
            _ => SoundKind::Jingle,
        };
        assert_eq!(e.kind, kind, "{}", e.name);
    }
}

macro_rules! bad_entry {
    ($name:ident, $bytes:expr) => {
        static $name: SoundEntry = SoundEntry {
            name: "se_bad",
            kind: SoundKind::Se,
            loop_start: None,
            wav: &AlignedWav(*$bytes),
        };
    };
}

bad_entry!(SHORT, b"RIFF");
bad_entry!(
    NOT_PCM,
    b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x03\0\x01\0\x22\x56\0\0\x44\xac\0\0\x02\0\x10\0data\0\0\0\0"
);
bad_entry!(
    BAD_RATE,
    b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x44\xac\0\0\x44\xac\0\0\x02\0\x10\0data\0\0\0\0"
);
bad_entry!(
    BAD_DATA_SIZE,
    b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x22\x56\0\0\x44\xac\0\0\x02\0\x10\0data\x04\0\0\0\0\0"
);
bad_entry!(
    GOOD_EMPTY,
    b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x22\x56\0\0\x44\xac\0\0\x02\0\x10\0data\0\0\0\0"
);

bad_entry!(
    STEREO,
    b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x02\0\x22\x56\0\0\x44\xac\0\0\x02\0\x10\0data\0\0\0\0"
);
bad_entry!(
    EIGHT_BIT,
    b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x22\x56\0\0\x44\xac\0\0\x02\0\x08\0data\0\0\0\0"
);
bad_entry!(
    NOT_RIFF,
    b"RIFX\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x22\x56\0\0\x44\xac\0\0\x02\0\x10\0data\0\0\0\0"
);
bad_entry!(
    NO_DATA_CHUNK,
    b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x22\x56\0\0\x44\xac\0\0\x02\0\x10\0LIST\0\0\0\0"
);

fn reason(entry: &'static SoundEntry) -> &'static str {
    match pcm16(entry) {
        Err(AssetError::Wav { reason, .. }) => reason,
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_header_check_rejects_its_own_defect() {
    assert!(reason(&STEREO).contains("モノラル"));
    assert!(reason(&EIGHT_BIT).contains("16 ビット"));
    assert!(reason(&NOT_RIFF).contains("RIFF"));
    assert!(reason(&NO_DATA_CHUNK).contains("data"));
    assert!(reason(&NOT_PCM).contains("PCM"));
    assert!(reason(&BAD_RATE).contains("22050"));
    assert!(reason(&BAD_DATA_SIZE).contains("サイズ"));
}

#[test]
fn malformed_headers_are_rejected_with_the_entry_name() {
    for entry in [&SHORT, &NOT_PCM, &BAD_RATE, &BAD_DATA_SIZE] {
        match pcm16(entry) {
            Err(AssetError::Wav { name, .. }) => assert_eq!(name, "se_bad"),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(pcm16(&GOOD_EMPTY).unwrap().len(), 0);
}
