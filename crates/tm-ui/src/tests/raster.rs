//! 目で確かめるための仮の描画。`TM_UI_DUMP_DIR` に PPM を書く(`cargo test -- --ignored dump_screens`)。

use tm_input::Action;
use tm_platform_api::{AssetBundle, Color, DrawCmd, DrawList, Rect};

use super::budget::busiest;
use super::map::shared_lines;
use super::{harness_with_line, plan};
use crate::testing::UiHarness;

const W: i32 = 640;
const H: i32 = 360;

struct Canvas {
    px: Vec<[u8; 3]>,
    clip: Rect,
    dx: i32,
    dy: i32,
}

fn blend(dst: &mut [u8; 3], src: [u8; 4]) {
    let a = u32::from(src[3]);
    for i in 0..3 {
        dst[i] = ((u32::from(src[i]) * a + u32::from(dst[i]) * (255 - a)) / 255) as u8;
    }
}

impl Canvas {
    fn put(&mut self, x: i32, y: i32, c: [u8; 4]) {
        let (x, y) = (x + self.dx, y + self.dy);
        let r = self.clip;
        if x < r.x.max(0)
            || y < r.y.max(0)
            || x >= (r.x + r.w as i32).min(W)
            || y >= (r.y + r.h as i32).min(H)
        {
            return;
        }
        blend(&mut self.px[(y * W + x) as usize], c);
    }

    fn fill(&mut self, r: Rect, c: Color) {
        for y in r.y..r.y + r.h as i32 {
            for x in r.x..r.x + r.w as i32 {
                self.put(x, y, [c.r, c.g, c.b, c.a]);
            }
        }
    }

    fn cmd(&mut self, b: &AssetBundle, cmd: &DrawCmd, layer_size: (u32, u32)) {
        match *cmd {
            DrawCmd::Clear(c) => self.fill(
                Rect {
                    x: 0,
                    y: 0,
                    w: layer_size.0,
                    h: layer_size.1,
                },
                c,
            ),
            DrawCmd::FillRect { rect, color } => self.fill(rect, color),
            DrawCmd::Line {
                x0,
                y0,
                x1,
                y1,
                color,
            } => {
                let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
                for i in 0..=n {
                    self.put(
                        x0 + (x1 - x0) * i / n,
                        y0 + (y1 - y0) * i / n,
                        [color.r, color.g, color.b, color.a],
                    );
                }
            }
            DrawCmd::Sprite { id, dst, tint } => {
                let s = b.sprites[usize::from(id.0)];
                let atlas = &b.atlases[usize::from(s.atlas)];
                for y in 0..dst.h as i32 {
                    for x in 0..dst.w as i32 {
                        let sx = i32::from(s.x) + x * i32::from(s.w) / dst.w as i32;
                        let sy = i32::from(s.y) + y * i32::from(s.h) / dst.h as i32;
                        let o = ((sy as u32 * atlas.width + sx as u32) * 4) as usize;
                        let p = &atlas.rgba[o..o + 4];
                        let m = |a: u8, t: u8| (u32::from(a) * u32::from(t) / 255) as u8;
                        self.put(
                            dst.x + x,
                            dst.y + y,
                            [
                                m(p[0], tint.r),
                                m(p[1], tint.g),
                                m(p[2], tint.b),
                                m(p[3], tint.a),
                            ],
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// 直接描画モードと同じ規則で描く(層は `DrawCachedLayer` の位置で平行移動して切り取る。14 §3.5.3)。
pub(super) fn rasterize(list: &DrawList) -> Vec<[u8; 3]> {
    let b = tm_assets::bundle().expect("アセット");
    let full = Rect {
        x: 0,
        y: 0,
        w: W as u32,
        h: H as u32,
    };
    let mut c = Canvas {
        px: vec![[0; 3]; (W * H) as usize],
        clip: full,
        dx: 0,
        dy: 0,
    };
    let mut layer: Option<(Rect, Vec<DrawCmd>)> = None;
    let mut recording = false;
    for cmd in &list.cmds {
        match *cmd {
            DrawCmd::BeginCachedLayer { x, y, w, h, .. } => {
                layer = Some((Rect { x, y, w, h }, Vec::new()));
                recording = true;
            }
            DrawCmd::EndCachedLayer => recording = false,
            DrawCmd::DrawCachedLayer { .. } => {
                if let Some((r, cmds)) = &layer {
                    c.clip = *r;
                    c.dx = r.x;
                    c.dy = r.y;
                    for cmd in cmds {
                        c.cmd(b, cmd, (r.w, r.h));
                    }
                    c.clip = full;
                    c.dx = 0;
                    c.dy = 0;
                }
            }
            _ if recording => layer.as_mut().expect("層").1.push(*cmd),
            _ => c.cmd(b, cmd, (W as u32, H as u32)),
        }
    }
    c.px
}

pub(super) fn write_ppm(path: &std::path::Path, px: &[[u8; 3]]) {
    let mut data = format!("P6\n{W} {H}\n255\n").into_bytes();
    for p in px {
        data.extend_from_slice(p);
    }
    std::fs::write(path, data).expect("書き込み");
}

#[test]
#[ignore = "目で確かめるための画像を書く。TM_UI_DUMP_DIR がなければ何もしない"]
fn dump_screens() {
    let Ok(dir) = std::env::var("TM_UI_DUMP_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let mut title = UiHarness::new_title();
    title.frames(1);
    let mut map = harness_with_line();
    map.frames(240);
    map.move_cursor_to(plan()[1]);
    let mut shared = shared_lines();
    shared.move_cursor_to(plan()[0]);
    shared.frames(300);
    let mut busy = busiest();
    busy.frames(1);
    let save = crate::AppStatus {
        save_enabled: true,
        has_save: true,
        ..Default::default()
    };
    let mut title_save = UiHarness::new_title();
    title_save.set_app_status(save);
    title_save.frames(1);
    let mut title_confirm1 = UiHarness::new_title();
    title_confirm1.set_app_status(save);
    title_confirm1.press(Action::MoveDown);
    title_confirm1.frames(30);
    title_confirm1.press(Action::Confirm);
    title_confirm1.frames(20);
    let mut title_confirm2 = UiHarness::new_title();
    title_confirm2.set_app_status(save);
    title_confirm2.press(Action::MoveDown);
    title_confirm2.frames(30);
    title_confirm2.press(Action::Confirm);
    title_confirm2.frames(20);
    title_confirm2.press(Action::MoveRight);
    title_confirm2.press(Action::Confirm);
    title_confirm2.frames(20);
    let settings = |rows: usize, yes: usize| {
        let mut h = harness_with_line();
        h.set_app_status(save);
        h.frames(30);
        h.press(Action::Menu);
        for _ in 0..rows {
            h.press(Action::MoveDown);
        }
        for _ in 0..yes {
            h.press(Action::Confirm);
            h.frames(20);
            h.press(Action::MoveRight);
        }
        h.frames(2);
        h
    };
    let menu = settings(0, 0);
    let quit1 = settings(5, 1);
    let quit2 = settings(5, 2);
    let quitting = |status: crate::AppStatus| {
        let mut h = settings(5, 3);
        h.press(Action::Confirm);
        assert_eq!(h.requests, [crate::AppRequest::QuitGame]);
        h.set_app_status(status);
        h.frames(2);
        h
    };
    let saving = quitting(crate::AppStatus {
        saving: true,
        ..save
    });
    let power = quitting(crate::AppStatus {
        power_off_on_quit: true,
        ..save
    });
    let mut mark = harness_with_line();
    mark.frames(240);
    mark.set_app_status(crate::AppStatus {
        saving: true,
        ..save
    });
    mark.frames(1);
    for (name, h) in [
        ("title_save", &title_save),
        ("title_confirm1", &title_confirm1),
        ("title_confirm2", &title_confirm2),
        ("settings_top", &menu),
        ("quit_confirm1", &quit1),
        ("quit_confirm2", &quit2),
        ("quitting_saving", &saving),
        ("quitting_power", &power),
        ("save_mark", &mark),
        ("title", &title),
        ("map", &map),
        ("shared", &shared),
        ("busy", &busy),
    ] {
        write_ppm(&dir.join(format!("{name}.ppm")), &rasterize(h.draw_list()));
    }
}
