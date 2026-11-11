//! デバッグ表示(devtools の `TM_DEBUG_OVERLAY`)[16 §1.14、APP-D-12]。

use std::fmt::Write as _;

use tm_assets::font::{self, FontId};
use tm_platform_api::{Color, DrawCmd, DrawList, LOGICAL_WIDTH, Rect};

use crate::logging;
use crate::profiler::{FrameProfiler, SECTIONS};

/// デバッグ表示で足す描画要素の上限。
pub const MAX_ELEMENTS: usize = 400;
/// 表示する直近のログの行数。
pub const LOG_LINES: usize = 6;
const BAR_H: u32 = 4;
const BAR_PX_PER_FRAME: u64 = 160;
const FRAME_US: u64 = 16_667;
const BAND: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 160,
};
const WHITE: Color = Color {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
const BAR_COLORS: [Color; SECTIONS] = [
    Color {
        r: 120,
        g: 120,
        b: 255,
        a: 255,
    },
    Color {
        r: 80,
        g: 220,
        b: 80,
        a: 255,
    },
    Color {
        r: 255,
        g: 200,
        b: 40,
        a: 255,
    },
    Color {
        r: 255,
        g: 90,
        b: 90,
        a: 255,
    },
    Color {
        r: 200,
        g: 200,
        b: 200,
        a: 255,
    },
];

struct Out<'a> {
    draw: &'a mut DrawList,
    left: usize,
}

impl Out<'_> {
    fn push(&mut self, cmd: DrawCmd) -> bool {
        if self.left == 0 {
            return false;
        }
        self.left -= 1;
        self.draw.cmds.push(cmd);
        true
    }

    fn text(&mut self, x: i32, top: i32, s: &str) {
        let face = font::face(FontId::Body12);
        let base = top + i32::from(face.ascent);
        let mut pen = x;
        for ch in s.chars() {
            let g = font::glyph(FontId::Body12, ch)
                .unwrap_or_else(|| font::fallback_glyph(FontId::Body12));
            if pen + i32::from(g.advance) > LOGICAL_WIDTH as i32 {
                return;
            }
            if let Some(b) = g.bitmap {
                let dst = Rect {
                    x: pen + i32::from(b.x_off),
                    y: base + i32::from(b.y_off),
                    w: u32::from(b.w),
                    h: u32::from(b.h),
                };
                if !self.push(DrawCmd::Sprite {
                    id: b.sprite,
                    dst,
                    tint: WHITE,
                }) {
                    return;
                }
            }
            pen += i32::from(g.advance);
        }
    }
}

/// `draw` の末尾にデバッグ表示を足す(`MAX_ELEMENTS` 個まで)。
pub fn append(profiler: &FrameProfiler, draw: &mut DrawList) {
    append_with(profiler, &logging::recent(LOG_LINES), draw);
}

fn append_with(profiler: &FrameProfiler, logs: &[String], draw: &mut DrawList) {
    let line_h = i32::from(font::face(FontId::Body12).line_height);
    let rows = 1 + logs.len() as i32;
    let mut out = Out {
        draw,
        left: MAX_ELEMENTS,
    };
    out.push(DrawCmd::FillRect {
        rect: Rect {
            x: 0,
            y: 0,
            w: LOGICAL_WIDTH,
            h: (BAR_H as i32 + 2 + rows * line_h) as u32,
        },
        color: BAND,
    });
    let sec = profiler.last_sections();
    let mut x = 0i32;
    for (us, color) in sec.iter().zip(BAR_COLORS) {
        let w = (us * BAR_PX_PER_FRAME / FRAME_US).min(u64::from(LOGICAL_WIDTH)) as u32;
        out.push(DrawCmd::FillRect {
            rect: Rect {
                x,
                y: 0,
                w,
                h: BAR_H,
            },
            color,
        });
        x += w as i32;
    }
    let p = profiler.last_platform();
    let interval = profiler.last_interval_us().max(1);
    let mut line = String::new();
    let _ = write!(
        line,
        "fps {}.{} in {} up {} sim {} draw {} out {} us quads {}",
        1_000_000 / interval,
        (10_000_000 / interval) % 10,
        sec[0],
        sec[1],
        sec[2],
        sec[3],
        sec[4],
        p.draw_quads
    );
    let mut y = BAR_H as i32 + 2;
    out.text(2, y, &line);
    for l in logs {
        y += line_h;
        out.text(2, y, l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_platform_api::MonoTime;

    use crate::profiler::FrameTimes;

    #[test]
    fn stays_within_the_budget() {
        let mut prof = FrameProfiler::new(false);
        let t = |us| MonoTime(us);
        prof.record(
            FrameTimes {
                input: t(0),
                update: t(100),
                sim: t(900),
                draw: t(2_000),
                out: t(3_000),
                end: t(16_000),
            },
            1,
            true,
            0,
            Default::default(),
        );
        let mut d = DrawList::default();
        append(&prof, &mut d);
        assert!(
            d.cmds.len() > 6 && d.cmds.len() <= MAX_ELEMENTS,
            "{}",
            d.cmds.len()
        );
    }

    #[test]
    fn body12_text_stays_inside_the_band() {
        let prof = FrameProfiler::new(false);
        let logs = vec!["Wあ".repeat(40); LOG_LINES];
        let mut d = DrawList::default();
        append_with(&prof, &logs, &mut d);
        let DrawCmd::FillRect { rect: band, .. } = d.cmds[0] else {
            panic!("先頭は帯");
        };
        assert_eq!(band.h as i32, 6 + 7 * 16, "Body12 の行の高さ 16");
        let sprites: Vec<Rect> = d
            .cmds
            .iter()
            .filter_map(|c| match c {
                DrawCmd::Sprite { dst, .. } => Some(*dst),
                _ => None,
            })
            .collect();
        assert!(sprites.len() > 100);
        let glyph = |face, ch| {
            font::glyph(face, ch)
                .and_then(|g| g.bitmap)
                .map(|b| b.sprite)
        };
        let used = |id| {
            d.cmds
                .iter()
                .any(|c| matches!(c, DrawCmd::Sprite { id: i, .. } if Some(*i) == id))
        };
        assert!(used(glyph(FontId::Body12, 'f')), "fps の行は Body12");
        assert!(!used(glyph(FontId::Head16, 'f')));
        let f = font::glyph(FontId::Body12, 'f')
            .and_then(|g| g.bitmap)
            .expect("f");
        let first = sprites
            .iter()
            .find(|r| r.w == u32::from(f.w) && r.x == 2 + i32::from(f.x_off));
        assert_eq!(
            first.map(|r| r.y),
            Some(6 + 11 + i32::from(f.y_off)),
            "基準線は上から 6 + ascent 11"
        );
        for r in sprites {
            assert!(r.x >= 0 && r.x + r.w as i32 <= 640, "{r:?}");
            assert!(r.y >= 0 && r.y + r.h as i32 <= band.h as i32, "{r:?}");
        }
    }

    #[test]
    fn long_logs_are_cut_at_the_budget() {
        let prof = FrameProfiler::new(false);
        let logs = vec!["W".repeat(200); LOG_LINES];
        let mut d = DrawList::default();
        d.cmds.push(DrawCmd::Clear(BAND));
        append_with(&prof, &logs, &mut d);
        assert_eq!(d.cmds.len(), 1 + MAX_ELEMENTS, "足すのは上限まで");
        assert_eq!(d.cmds[0], DrawCmd::Clear(BAND));
    }
}
