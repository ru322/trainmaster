//! 目で確かめるための描画。`SoftRaster` で描画の解像度(1920×1080)に描き、`TM_UI_DUMP_DIR` に PPM を書く(`cargo test -- --ignored dump_screens`)。

use tm_input::Action;
use tm_platform_api::{DrawList, RENDER_HEIGHT, RENDER_SCALE, RENDER_WIDTH, SoftRaster};

use super::budget::busiest;
use super::map::shared_lines;
use super::{harness_with_line, plan};
use crate::testing::UiHarness;

const W: usize = RENDER_WIDTH as usize;
const H: usize = RENDER_HEIGHT as usize;

/// 描画の解像度の RGB(1920×1080、行優先)。層は `DrawCachedLayer` の位置でその場描きする(14 §3.5.3)。
pub(super) fn rasterize(list: &DrawList) -> Vec<[u8; 3]> {
    let b = tm_assets::bundle().expect("アセット");
    let mut s = SoftRaster::new();
    s.draw(b, list);
    s.pixels().chunks(4).map(|p| [p[0], p[1], p[2]]).collect()
}

/// 論理の画素(640×360)ごとに、その画素の中心にあたる描画の画素を取り出す。
pub(super) fn logical(native: &[[u8; 3]]) -> Vec<[u8; 3]> {
    let k = RENDER_SCALE as usize;
    (0..H / k)
        .flat_map(|y| (0..W / k).map(move |x| native[(y * k + k / 2) * W + x * k + k / 2]))
        .collect()
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
    let mut goal = harness_with_line();
    let now = goal.now;
    goal.ui.observe(
        now,
        &tm_core::GameEvent::GoalReached {
            total_earned: tm_numeric::BigNum::ONE,
        },
        &mut Vec::new(),
    );
    goal.frames(18);
    for (name, h) in [
        ("goal", &goal),
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
