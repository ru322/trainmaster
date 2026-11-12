//! 埋め込みアセットへの参照と形の検査 [10 §8.2、15 §3.7]。

use core::fmt;
use core::hint::black_box;

use crate::audio::{SoundData, SoundId, SoundKind};
use crate::draw::SpriteId;

const MAX_ATLASES: usize = 2;
const MAX_ATLAS_SIZE: u32 = 2048;
const PREFAULT_STRIDE_SAMPLES: usize = 4096 / size_of::<i16>();

/// アトラスのページ。RGBA はストレートアルファで、バイトの並びは R, G, B, A。
#[derive(Clone, Copy)]
pub struct AtlasImage {
    /// 幅。
    pub width: u32,
    /// 高さ。
    pub height: u32,
    /// 画素。長さは `width × height × 4`。
    pub rgba: &'static [u8],
}

impl fmt::Debug for AtlasImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AtlasImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rgba", &self.rgba.len())
            .finish()
    }
}

/// スプライトのアトラス上の位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteRect {
    /// アトラスの番号。
    pub atlas: u8,
    /// 左端。
    pub x: u16,
    /// 上端。
    pub y: u16,
    /// 幅。
    pub w: u16,
    /// 高さ。
    pub h: u16,
}

impl SpriteRect {
    /// 描き先の大きさ `(w, h)`(描画の画素)が、縦横ともこの矩形の 1.5 倍以上か。大きい版(`SpriteAlt`)を使う目安。
    pub fn is_magnified_by(&self, w: u64, h: u64) -> bool {
        2 * w >= 3 * u64::from(self.w) && 2 * h >= 3 * u64::from(self.h)
    }

    /// 表の未使用の枠に置くゼロ値。`validate` は空の矩形として拒否する。
    pub const ZERO: SpriteRect = SpriteRect {
        atlas: 0,
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };
}

/// スプライトの大きい版(2 倍に描くとき用の、縦横 2 倍の画素の絵)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpriteAlt {
    /// 元のスプライト。
    pub id: SpriteId,
    /// 大きい版のアトラス上の位置。
    pub rect: SpriteRect,
}

/// データの誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// アトラスが 0 枚、または 2 枚を超える。
    AtlasCount {
        /// 枚数。
        count: usize,
    },
    /// アトラスの幅か高さが 0、または 2048 を超える。
    AtlasSize {
        /// アトラスの番号。
        atlas: u8,
        /// 幅。
        width: u32,
        /// 高さ。
        height: u32,
    },
    /// 画素の長さが `width × height × 4` と違う。
    AtlasBytes {
        /// アトラスの番号。
        atlas: u8,
        /// 期待する長さ。
        expected: usize,
        /// 実際の長さ。
        got: usize,
    },
    /// アトラスの番号が範囲外、または矩形がページからはみ出す。
    SpriteOutOfAtlas {
        /// 該当のスプライト。
        sprite: SpriteId,
    },
    /// 幅か高さが 0 のスプライト。
    SpriteEmpty {
        /// 該当のスプライト。
        sprite: SpriteId,
    },
    /// 大きい版の元の番号が範囲外、昇順でない、または矩形が元より大きくない。
    SpriteAlt {
        /// 該当のスプライト。
        sprite: SpriteId,
    },
    /// スプライトの数が `u16` で数えられる範囲を超える。
    TooManySprites {
        /// 数。
        count: usize,
    },
    /// サンプルが空の音。
    SoundEmpty {
        /// 該当の音。
        sound: SoundId,
    },
    /// `Bgm` 以外にループがある、または `loop_start` がサンプル数以上。
    SoundLoop {
        /// 該当の音。
        sound: SoundId,
    },
    /// 名前が `^(se|bgm|jingle)_[a-z0-9_]+$` に合わない、または種類と接頭辞が合わない。
    SoundName {
        /// 該当の音。
        sound: SoundId,
    },
    /// 名前が辞書順に並んでいない [02 D33]。
    SoundOrder {
        /// 該当の音。
        sound: SoundId,
    },
    /// `tm-assets` が行う WAV の検査の失敗。
    Wav {
        /// 音の名前。
        name: &'static str,
        /// 理由。
        reason: &'static str,
    },
}

/// 実行ファイルに埋め込んだ画像と音のデータへの参照の束。
#[derive(Clone, Copy)]
pub struct AssetBundle {
    /// アトラスのページ。2048 × 2048 以下で 2 枚まで。
    pub atlases: &'static [AtlasImage],
    /// `SpriteId` の番号順のスプライトの位置。画素は描画の解像度(論理 1px = `RENDER_SCALE` 画素)の絵。
    pub sprites: &'static [SpriteRect],
    /// 大きい版。`id` の昇順で、1 つの `id` に 1 つまで。
    pub alt: &'static [SpriteAlt],
    /// `SoundId` の番号順の音。
    pub sounds: &'static [SoundData],
    /// 白・不透明の画素のスプライト。矩形と線の描画に使う。
    pub white: SpriteId,
}

impl fmt::Debug for AssetBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssetBundle")
            .field("atlases", &self.atlases)
            .field("sprites", &self.sprites.len())
            .field("alt", &self.alt.len())
            .field("sounds", &self.sounds.len())
            .field("white", &self.white)
            .finish()
    }
}

impl AssetBundle {
    /// データの形を検査し、最初に見つけた誤りを返す。
    pub fn validate(&self) -> Result<(), AssetError> {
        self.validate_atlases()?;
        self.validate_sprites()?;
        self.validate_sounds()
    }

    /// `id` を描き先の大きさ `(w, h)`(描画の画素)で描くときのアトラス上の位置。縦横とも元の 1.5 倍以上に描くときは大きい版。
    pub fn source_rect(&self, id: SpriteId, w: u64, h: u64) -> Option<SpriteRect> {
        let base = *self.sprites.get(usize::from(id.0))?;
        if !base.is_magnified_by(w, h) {
            return Some(base);
        }
        Some(
            self.alt
                .binary_search_by_key(&id, |a| a.id)
                .map_or(base, |i| self.alt[i].rect),
        )
    }

    /// 音のサンプル列を 4096 バイトごとに 1 バイト読み、ページインを済ませる。読んだ値の合計を返す。
    pub fn prefault(&self) -> u64 {
        let mut sum = 0u64;
        for sound in self.sounds {
            for sample in sound.samples.iter().step_by(PREFAULT_STRIDE_SAMPLES) {
                sum += u64::from(*sample as u8);
            }
        }
        black_box(sum)
    }

    fn validate_atlases(&self) -> Result<(), AssetError> {
        let count = self.atlases.len();
        if count == 0 || count > MAX_ATLASES {
            return Err(AssetError::AtlasCount { count });
        }
        for (i, atlas) in self.atlases.iter().enumerate() {
            let index = u8::try_from(i).expect("ページ数は 2 以下");
            let (w, h) = (atlas.width, atlas.height);
            if w == 0 || h == 0 || w > MAX_ATLAS_SIZE || h > MAX_ATLAS_SIZE {
                return Err(AssetError::AtlasSize {
                    atlas: index,
                    width: w,
                    height: h,
                });
            }
            let expected = w as usize * h as usize * 4;
            if atlas.rgba.len() != expected {
                return Err(AssetError::AtlasBytes {
                    atlas: index,
                    expected,
                    got: atlas.rgba.len(),
                });
            }
        }
        Ok(())
    }

    fn validate_sprites(&self) -> Result<(), AssetError> {
        let count = self.sprites.len();
        if count > usize::from(u16::MAX) {
            return Err(AssetError::TooManySprites { count });
        }
        for (i, rect) in self.sprites.iter().enumerate() {
            let sprite = SpriteId(u16::try_from(i).expect("上で数を検査済み"));
            let Some(atlas) = self.atlases.get(usize::from(rect.atlas)) else {
                return Err(AssetError::SpriteOutOfAtlas { sprite });
            };
            if rect.w == 0 || rect.h == 0 {
                return Err(AssetError::SpriteEmpty { sprite });
            }
            let right = u32::from(rect.x) + u32::from(rect.w);
            let bottom = u32::from(rect.y) + u32::from(rect.h);
            if right > atlas.width || bottom > atlas.height {
                return Err(AssetError::SpriteOutOfAtlas { sprite });
            }
        }
        if usize::from(self.white.0) >= count {
            return Err(AssetError::SpriteOutOfAtlas { sprite: self.white });
        }
        let mut previous: Option<SpriteId> = None;
        for alt in self.alt {
            let sprite = alt.id;
            let Some(base) = self.sprites.get(usize::from(sprite.0)) else {
                return Err(AssetError::SpriteAlt { sprite });
            };
            if previous.is_some_and(|p| p >= sprite) || alt.rect.w <= base.w || alt.rect.h <= base.h
            {
                return Err(AssetError::SpriteAlt { sprite });
            }
            previous = Some(sprite);
            let Some(atlas) = self.atlases.get(usize::from(alt.rect.atlas)) else {
                return Err(AssetError::SpriteOutOfAtlas { sprite });
            };
            let right = u32::from(alt.rect.x) + u32::from(alt.rect.w);
            let bottom = u32::from(alt.rect.y) + u32::from(alt.rect.h);
            if right > atlas.width || bottom > atlas.height {
                return Err(AssetError::SpriteOutOfAtlas { sprite });
            }
        }
        Ok(())
    }

    fn validate_sounds(&self) -> Result<(), AssetError> {
        let mut previous: Option<&str> = None;
        for (i, sound) in self.sounds.iter().enumerate() {
            let id = SoundId(u16::try_from(i).unwrap_or(u16::MAX));
            if sound.samples.is_empty() {
                return Err(AssetError::SoundEmpty { sound: id });
            }
            if let Some(loop_start) = sound.loop_start
                && (sound.kind != SoundKind::Bgm || loop_start as usize >= sound.samples.len())
            {
                return Err(AssetError::SoundLoop { sound: id });
            }
            if !name_matches(sound.name, sound.kind) {
                return Err(AssetError::SoundName { sound: id });
            }
            if previous.is_some_and(|p| p >= sound.name) {
                return Err(AssetError::SoundOrder { sound: id });
            }
            previous = Some(sound.name);
        }
        Ok(())
    }
}

fn name_matches(name: &str, kind: SoundKind) -> bool {
    let prefix = match kind {
        SoundKind::Se => "se_",
        SoundKind::Bgm => "bgm_",
        SoundKind::Jingle => "jingle_",
    };
    name.strip_prefix(prefix).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    })
}
