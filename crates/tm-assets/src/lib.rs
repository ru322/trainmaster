//! テキスト表、アトラスのメタデータ、音のデータ。生成アセットを埋め込んで `AssetBundle` として渡す。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod atlas;
pub mod font;
pub mod palette;
pub mod sound;
pub mod sprite;
pub mod text;

use std::sync::OnceLock;

pub use text::txt;
pub use tm_platform_api::{AssetBundle, AssetError, Color, SoundId, SpriteId};

static BUNDLE: OnceLock<Result<AssetBundle, AssetError>> = OnceLock::new();

/// 埋め込みアセットの束。最初の呼び出しで組み立てて `validate` を通し、以後は同じ参照を返す。
pub fn bundle() -> Result<&'static AssetBundle, AssetError> {
    BUNDLE.get_or_init(build).as_ref().map_err(Clone::clone)
}

fn build() -> Result<AssetBundle, AssetError> {
    let bundle = AssetBundle {
        atlases: &atlas::ATLASES,
        sprites: &atlas::ALL_RECTS,
        sounds: sound::sound_data()?,
        white: sprite::PX_WHITE,
    };
    bundle.validate()?;
    Ok(bundle)
}
