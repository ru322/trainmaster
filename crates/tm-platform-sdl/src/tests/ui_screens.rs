//! UI-V-08 の CI の分: `tm-ui` の `UiHarness` の画面を 4 通りの方式で描き、`SoftRaster` と比べる。
//! `tm-platform-sdl` は `tm-ui` に依存しない(02 §2.3)ので、`tm-ui` の insta のスナップショット(`draw_text()`)を読んで `DrawList` に戻す。

use std::collections::BTreeMap;
use std::path::PathBuf;

use tm_assets::font::{self, FontId};
use tm_assets::sprite;
use tm_platform_api::{Color, DrawCmd, DrawList, LayerId, Rect, SpriteId};

use super::UI_SCREEN;
use super::render::check_all_modes;

fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tm-ui/src/tests/snapshots")
}

fn sprite_names() -> BTreeMap<&'static str, SpriteId> {
    (0..sprite::SPRITE_COUNT as u16)
        .map(|i| (sprite::name(SpriteId(i)), SpriteId(i)))
        .collect()
}

fn color(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    let v = u32::from_str_radix(h, 16).ok()?;
    (h.len() == 8).then(|| {
        let [r, g, b, a] = v.to_be_bytes();
        Color { r, g, b, a }
    })
}

fn pair(s: &str, sep: char) -> Option<(i32, i32)> {
    let (a, b) = s.split_once(sep)?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn rect(pos: &str, size: &str) -> Option<Rect> {
    let (x, y) = pair(pos, ',')?;
    let (w, h) = size.split_once('x')?;
    Some(Rect {
        x,
        y,
        w: w.parse().ok()?,
        h: h.parse().ok()?,
    })
}

fn sprite_id(name: &str, names: &BTreeMap<&'static str, SpriteId>) -> Option<SpriteId> {
    if let Some(n) = name.strip_prefix("glyph#") {
        return n.parse().ok().map(SpriteId);
    }
    let mut chars = name.chars();
    if chars.next() == Some('\'') {
        let ch = chars.next()?;
        let size = chars.as_str().strip_prefix('\'')?;
        let face = match size {
            "12" => FontId::Body12,
            "16" => FontId::Head16,
            _ => return None,
        };
        return font::glyph(face, ch)?.bitmap.map(|b| b.sprite);
    }
    names.get(name).copied()
}

/// `tm_ui::testing::draw_text` の 1 行を `DrawCmd` に戻す。
fn parse_line(line: &str, names: &BTreeMap<&'static str, SpriteId>) -> Option<DrawCmd> {
    let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
    let f: Vec<&str> = rest.split(' ').collect();
    Some(match (kind, f.as_slice()) {
        ("Clear", [c]) => DrawCmd::Clear(color(c)?),
        ("Fill", [p, s, c]) => DrawCmd::FillRect {
            rect: rect(p, s)?,
            color: color(c)?,
        },
        ("Line", [ends, c]) => {
            let from = ends.find(',')? + 2;
            let sep = from + ends.get(from..)?.find('-')?;
            let ((x0, y0), (x1, y1)) = (pair(&ends[..sep], ',')?, pair(&ends[sep + 1..], ',')?);
            DrawCmd::Line {
                x0,
                y0,
                x1,
                y1,
                color: color(c)?,
            }
        }
        ("Sprite", [n, p, s, c]) => DrawCmd::Sprite {
            id: sprite_id(n, names)?,
            dst: rect(p, s)?,
            tint: color(c)?,
        },
        ("BeginLayer", [l, rev, p, s]) => {
            let r = rect(p, s)?;
            DrawCmd::BeginCachedLayer {
                layer: LayerId(l.parse().ok()?),
                revision: rev.strip_prefix("rev=")?.parse().ok()?,
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
            }
        }
        ("Quad", [a, b, c, d, col]) => DrawCmd::Quad {
            pts: [pair(a, ',')?, pair(b, ',')?, pair(c, ',')?, pair(d, ',')?],
            color: color(col)?,
        },
        ("EndLayer", [""]) => DrawCmd::EndCachedLayer,
        ("DrawLayer", [l]) => DrawCmd::DrawCachedLayer {
            layer: LayerId(l.parse().ok()?),
        },
        _ => return None,
    })
}

/// スナップショットのうち画面全体の `h.draw_text()` を写したもの(名前、`DrawList`)。行を戻せなければ失敗にする。
fn ui_lists() -> Vec<(String, DrawList)> {
    let names = sprite_names();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(snapshot_dir())
        .expect("tm-ui のスナップショットのディレクトリ")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "snap"))
        .collect();
    paths.sort();
    let mut lists = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut parts = text.splitn(3, "---\n");
        let (Some(""), Some(header), Some(body)) = (parts.next(), parts.next(), parts.next())
        else {
            panic!("{path:?}: insta の形式でない");
        };
        if !header.lines().any(|l| l == "expression: h.draw_text()") {
            continue;
        }
        let cmds = body
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| parse_line(l, &names).unwrap_or_else(|| panic!("{path:?}: {l}")))
            .collect();
        let stem = path.file_stem().unwrap().to_string_lossy();
        let name = stem.rsplit("__").next().unwrap().to_owned();
        lists.push((format!("ui-{name}"), DrawList { cmds }));
    }
    lists
}

#[test]
fn draw_text_lines_round_trip() {
    let names = sprite_names();
    let id = |n| sprite_id(n, &names).unwrap();
    assert_eq!(id("glyph#4000"), SpriteId(4000));
    assert_eq!(
        Some(id("'電'12")),
        font::glyph(FontId::Body12, '電').and_then(|g| g.bitmap.map(|b| b.sprite))
    );
    assert_eq!(
        Some(id("'電'16")),
        font::glyph(FontId::Head16, '電').and_then(|g| g.bitmap.map(|b| b.sprite))
    );
    assert_eq!(id(sprite::name(SpriteId(3))), SpriteId(3));
    assert_eq!(
        parse_line("Line -5,3--7,-9 #01020304", &names),
        Some(DrawCmd::Line {
            x0: -5,
            y0: 3,
            x1: -7,
            y1: -9,
            color: Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            }
        })
    );
    assert_eq!(
        parse_line("BeginLayer 0 rev=2 -37,32 640x288", &names),
        Some(DrawCmd::BeginCachedLayer {
            layer: LayerId(0),
            revision: 2,
            x: -37,
            y: 32,
            w: 640,
            h: 288
        })
    );
    assert_eq!(
        parse_line("EndLayer", &names),
        Some(DrawCmd::EndCachedLayer)
    );
    assert_eq!(parse_line("Fill 1,2 3x4", &names), None);
}

#[test]
fn ui_v08_ui_screens_match_soft_raster() {
    let lists = ui_lists();
    let names: Vec<&str> = lists.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["ui-map_and_hud", "ui-title"]);
    let mut all = lists.clone();
    let (_, map) = lists.iter().find(|(n, _)| n == "ui-map_and_hud").unwrap();
    let layers = map
        .cmds
        .iter()
        .filter(|c| matches!(c, DrawCmd::BeginCachedLayer { .. }))
        .count();
    assert_eq!(layers, 2, "縦に切った 2 枚の静的な層");
    // カメラを縦横に動かしたときと同じに、層の置き場所だけをずらす
    for (dx, dy) in [(37, 21), (224, 96)] {
        let mut shifted = map.clone();
        for cmd in &mut shifted.cmds {
            if let DrawCmd::BeginCachedLayer { x, y, .. } = cmd {
                *x -= dx;
                *y -= dy;
            }
        }
        all.push((format!("ui-map-cam{dx}-{dy}"), shifted));
    }
    let all_names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        all_names,
        [
            "ui-map_and_hud",
            "ui-title",
            "ui-map-cam37-21",
            "ui-map-cam224-96"
        ]
    );
    let bundle = tm_assets::bundle().expect("tm_assets::bundle");
    check_all_modes(bundle, &all, UI_SCREEN);
}
