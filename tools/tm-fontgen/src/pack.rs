//! グリフの棚詰め(15 §4.5)。

use crate::config::Strip;
use crate::error::FontgenError;

/// 詰める矩形 1 つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    /// 幅。
    pub w: u32,
    /// 高さ。
    pub h: u32,
    /// フォントの大きさ(px)。
    pub size: u32,
    /// 字。
    pub ch: char,
    /// フォントの番号(大きさが同じフォントがあるときの順序の決め手)。
    pub face: usize,
}

/// 高さの降順 → 大きさの降順 → 文字コードの昇順で棚詰めし、入力と同じ順の左上(ページの絶対座標)と使った行数を返す。
pub fn pack(
    items: &[Item],
    strip: &Strip,
    padding: u32,
) -> Result<(Vec<(u32, u32)>, u32), FontgenError> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (&items[a], &items[b]);
        b.h.cmp(&a.h)
            .then(b.size.cmp(&a.size))
            .then(a.ch.cmp(&b.ch))
            .then(a.face.cmp(&b.face))
    });
    let mut places = vec![(0, 0); items.len()];
    let (mut x, mut y, mut shelf_h, mut used) = (0, strip.y, 0, 0);
    for i in order {
        let it = items[i];
        if it.w + padding > strip.width {
            return Err(FontgenError::Overflow {
                needed: it.w + padding,
                available: strip.width,
            });
        }
        if x + it.w + padding > strip.width {
            y += shelf_h + padding;
            x = 0;
            shelf_h = 0;
        }
        if shelf_h == 0 {
            shelf_h = it.h;
        }
        if y + it.h > strip.y + strip.height {
            return Err(FontgenError::Overflow {
                needed: y + it.h - strip.y,
                available: strip.height,
            });
        }
        places[i] = (x, y);
        used = used.max(y + it.h - strip.y);
        x += it.w + padding;
    }
    Ok((places, used))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(width: u32, y: u32, height: u32) -> Strip {
        Strip {
            width,
            page: 0,
            index: 0,
            y,
            height,
            render_scale: 1,
        }
    }

    fn item(w: u32, h: u32, size: u32, ch: char) -> Item {
        Item {
            w,
            h,
            size,
            ch,
            face: 0,
        }
    }

    #[test]
    fn golden_positions() {
        let items = [
            item(4, 5, 16, 'b'),
            item(6, 8, 16, 'a'),
            item(3, 8, 24, 'z'),
            item(5, 5, 16, 'a'),
            item(7, 2, 24, 'c'),
        ];
        let (pos, used) = pack(&items, &strip(16, 100, 20), 1).unwrap();
        assert_eq!(pos, vec![(6, 109), (4, 100), (0, 100), (0, 109), (0, 115)]);
        assert_eq!(used, 17);
        assert_eq!(pack(&items, &strip(16, 100, 20), 1).unwrap().0, pos);
    }

    #[test]
    fn overflow_is_e11() {
        let items = [item(10, 10, 16, 'a'), item(10, 10, 16, 'b')];
        let e = pack(&items, &strip(16, 0, 15), 1).unwrap_err();
        assert_eq!(
            e,
            FontgenError::Overflow {
                needed: 21,
                available: 15
            }
        );
        assert_eq!(
            pack(&[item(16, 1, 16, 'a')], &strip(16, 0, 15), 1)
                .unwrap_err()
                .code(),
            "FG-E11"
        );
    }
}
