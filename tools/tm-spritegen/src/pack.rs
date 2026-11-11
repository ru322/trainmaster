//! 棚詰めのパッキング(15 §4.5 と同じ規則)。

use crate::atlas::Strip;
use crate::error::SpriteError;

/// 矩形の間の透明な余白(px)。
pub const PADDING: u32 = 1;

/// `items`(幅, 高さ, 名前)を、高さの降順 → 名前の昇順で棚詰めし、入力と同じ順の左上座標(ページの絶対座標)と使った行数を返す。
pub fn pack(
    items: &[(u32, u32, &str)],
    strip: &Strip,
) -> Result<(Vec<(u32, u32)>, u32), SpriteError> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        items[b]
            .1
            .cmp(&items[a].1)
            .then_with(|| items[a].2.cmp(items[b].2))
    });
    let mut places = vec![(0, 0); items.len()];
    let (mut x, mut y, mut shelf_h) = (0, strip.y, 0);
    let mut used = 0;
    for i in order {
        let (w, h, name) = items[i];
        if w + PADDING > strip.width {
            return Err(SpriteError::Shape {
                sprite: name.into(),
                reason: format!("幅 {w} が帯の幅 {} に入らない", strip.width),
            });
        }
        if x + w + PADDING > strip.width {
            y += shelf_h + PADDING;
            x = 0;
            shelf_h = 0;
        }
        if shelf_h == 0 {
            shelf_h = h;
        }
        if y + h > strip.y + strip.height {
            return Err(SpriteError::Overflow {
                needed: y + h - strip.y,
                available: strip.height,
            });
        }
        places[i] = (x, y);
        used = used.max(y + h - strip.y);
        x += w + PADDING;
    }
    Ok((places, used))
}
