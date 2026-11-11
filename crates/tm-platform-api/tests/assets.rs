mod common;

use common::{atlas, bundle, leak, rect, sound, valid_bundle};
use tm_platform_api::{AssetBundle, AssetError, SoundData, SoundId, SoundKind, SpriteId};

#[test]
fn valid_bundle_passes() {
    assert_eq!(valid_bundle().validate(), Ok(()));
}

#[test]
fn atlas_count_must_be_one_or_two() {
    let none = AssetBundle {
        atlases: &[],
        ..valid_bundle()
    };
    assert_eq!(none.validate(), Err(AssetError::AtlasCount { count: 0 }));
    let two = AssetBundle {
        atlases: leak(vec![atlas(16, 16), atlas(4, 4)]),
        ..valid_bundle()
    };
    assert_eq!(two.validate(), Ok(()));
    let three = AssetBundle {
        atlases: leak(vec![atlas(16, 16), atlas(4, 4), atlas(4, 4)]),
        ..valid_bundle()
    };
    assert_eq!(three.validate(), Err(AssetError::AtlasCount { count: 3 }));
}

#[test]
fn atlas_size_is_limited() {
    for (w, h) in [(0, 4), (4, 0), (1025, 4), (4, 1025)] {
        let bad = AssetBundle {
            atlases: leak(vec![atlas(4, 4), atlas(w, h)]),
            ..valid_bundle()
        };
        assert_eq!(
            bad.validate(),
            Err(AssetError::AtlasSize {
                atlas: 1,
                width: w,
                height: h
            })
        );
    }
    let max = AssetBundle {
        atlases: leak(vec![atlas(1024, 1024)]),
        ..valid_bundle()
    };
    assert_eq!(max.validate(), Ok(()));
}

#[test]
fn atlas_bytes_must_match_size() {
    for got in [63, 65] {
        let mut page = atlas(4, 4);
        page.rgba = leak(vec![0; got]);
        let bad = AssetBundle {
            atlases: leak(vec![page]),
            ..valid_bundle()
        };
        assert_eq!(
            bad.validate(),
            Err(AssetError::AtlasBytes {
                atlas: 0,
                expected: 64,
                got
            })
        );
    }
}

#[test]
fn sprite_must_stay_in_its_atlas() {
    for bad_rect in [
        rect(1, 0, 0, 2, 2),
        rect(0, 15, 0, 2, 2),
        rect(0, 0, 9, 2, 8),
    ] {
        let bad = AssetBundle {
            sprites: leak(vec![rect(0, 0, 0, 2, 2), bad_rect]),
            ..valid_bundle()
        };
        assert_eq!(
            bad.validate(),
            Err(AssetError::SpriteOutOfAtlas {
                sprite: SpriteId(1)
            })
        );
    }
    let edge = AssetBundle {
        sprites: leak(vec![rect(0, 14, 14, 2, 2)]),
        ..valid_bundle()
    };
    assert_eq!(edge.validate(), Ok(()));
}

#[test]
fn sprite_must_not_be_empty() {
    for empty in [
        rect(0, 0, 0, 0, 2),
        rect(0, 0, 0, 2, 0),
        tm_platform_api::SpriteRect::ZERO,
    ] {
        let bad = AssetBundle {
            sprites: leak(vec![rect(0, 0, 0, 2, 2), empty]),
            ..valid_bundle()
        };
        assert_eq!(
            bad.validate(),
            Err(AssetError::SpriteEmpty {
                sprite: SpriteId(1)
            })
        );
    }
}

#[test]
fn too_many_sprites() {
    let count = usize::from(u16::MAX) + 1;
    let bad = AssetBundle {
        sprites: leak(vec![rect(0, 0, 0, 1, 1); count]),
        ..valid_bundle()
    };
    assert_eq!(bad.validate(), Err(AssetError::TooManySprites { count }));
}

#[test]
fn white_must_be_a_sprite() {
    let bad = AssetBundle {
        white: SpriteId(2),
        ..valid_bundle()
    };
    assert_eq!(
        bad.validate(),
        Err(AssetError::SpriteOutOfAtlas {
            sprite: SpriteId(2)
        })
    );
}

fn with_sounds(sounds: Vec<SoundData>) -> AssetBundle {
    AssetBundle {
        sounds: leak(sounds),
        ..valid_bundle()
    }
}

#[test]
fn empty_sound_is_rejected() {
    let bad = with_sounds(vec![sound("se_a", SoundKind::Se, 0, None)]);
    assert_eq!(
        bad.validate(),
        Err(AssetError::SoundEmpty { sound: SoundId(0) })
    );
}

#[test]
fn only_bgm_may_loop_and_inside_the_samples() {
    let cases = [
        sound("se_a", SoundKind::Se, 10, Some(0)),
        sound("jingle_a", SoundKind::Jingle, 10, Some(0)),
        sound("bgm_a", SoundKind::Bgm, 10, Some(10)),
    ];
    for bad_sound in cases {
        let bad = with_sounds(vec![bad_sound]);
        assert_eq!(
            bad.validate(),
            Err(AssetError::SoundLoop { sound: SoundId(0) })
        );
    }
    let ok = with_sounds(vec![
        sound("bgm_a", SoundKind::Bgm, 10, Some(9)),
        sound("bgm_b", SoundKind::Bgm, 10, None),
    ]);
    assert_eq!(ok.validate(), Ok(()));
}

#[test]
fn sound_name_must_match_kind_and_pattern() {
    let cases = [
        sound("coin", SoundKind::Se, 10, None),
        sound("se_", SoundKind::Se, 10, None),
        sound("bgm_a", SoundKind::Se, 10, None),
        sound("se_Coin", SoundKind::Se, 10, None),
        sound("se_co-in", SoundKind::Se, 10, None),
        sound("jingle_a", SoundKind::Bgm, 10, None),
    ];
    for bad_sound in cases {
        let bad = with_sounds(vec![bad_sound]);
        assert_eq!(
            bad.validate(),
            Err(AssetError::SoundName { sound: SoundId(0) }),
            "{bad_sound:?}"
        );
    }
    let ok = with_sounds(vec![sound("se_coin_2", SoundKind::Se, 10, None)]);
    assert_eq!(ok.validate(), Ok(()));
}

#[test]
fn sound_names_must_be_in_strict_lexical_order() {
    let unsorted = with_sounds(vec![
        sound("se_b", SoundKind::Se, 10, None),
        sound("se_a", SoundKind::Se, 10, None),
    ]);
    assert_eq!(
        unsorted.validate(),
        Err(AssetError::SoundOrder { sound: SoundId(1) })
    );
    let duplicate = with_sounds(vec![
        sound("se_a", SoundKind::Se, 10, None),
        sound("se_a", SoundKind::Se, 10, None),
    ]);
    assert_eq!(
        duplicate.validate(),
        Err(AssetError::SoundOrder { sound: SoundId(1) })
    );
}

#[test]
fn prefault_reads_every_4096_bytes_of_every_sound() {
    let mut long = vec![1i16; 5000];
    for (block, start) in [0, 2048, 4096].into_iter().enumerate() {
        long[start] = 10 * (block as i16 + 1);
    }
    let mut short = vec![1i16; 100];
    short[0] = 7;
    let sounds = vec![
        SoundData {
            name: "se_long",
            kind: SoundKind::Se,
            samples: leak(long),
            loop_start: None,
        },
        SoundData {
            name: "se_short",
            kind: SoundKind::Se,
            samples: leak(short),
            loop_start: None,
        },
    ];
    assert_eq!(with_sounds(sounds).prefault(), 10 + 20 + 30 + 7);
    assert_eq!(with_sounds(Vec::new()).prefault(), 0);
}

#[test]
fn bundle_debug_is_a_summary() {
    let text = format!(
        "{:?}",
        bundle(
            vec![atlas(1024, 1024)],
            vec![rect(0, 0, 0, 1, 1)],
            Vec::new()
        )
    );
    assert!(text.len() < 200, "{text}");
}

#[cfg(feature = "headless")]
#[test]
fn test_minimal_is_valid() {
    let minimal = AssetBundle::test_minimal();
    assert_eq!(minimal.validate(), Ok(()));
    assert_eq!(minimal.atlases.len(), 1);
    assert_eq!(minimal.sprites.len(), 2);
    assert_eq!(minimal.sounds[0].kind, SoundKind::Bgm);
    assert_eq!(minimal.sounds[1].kind, SoundKind::Se);
    assert_eq!(minimal.prefault(), 0);
}
