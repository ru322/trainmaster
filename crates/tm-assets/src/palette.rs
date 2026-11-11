//! ドット絵のパレットの名前付き色。生成物。生成元は `tm-spritegen`(`assets/sprites/palette.ron`)。

#[allow(missing_docs)]
mod generated {
    use tm_platform_api::Color;

    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/palette.rs"));
}

pub use generated::*;
