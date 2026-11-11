//! キーボードだけ・ゲームパッドだけで全部の機能に届くこと、操作の画面のスナップショットと画像。

use std::collections::BTreeSet;

use tm_core::{Command, GridPos};
use tm_input::Action;

use super::screens::{play, wait};
use super::{cheap_game, create_line, fill_at, plan, sprite_at};
use crate::AppRequest;
use crate::layout::{BAND_ROW2, HUD, PANEL};
use crate::scene::PlayMode;
use crate::testing::UiHarness;

struct Script {
    h: UiHarness,
    used: BTreeSet<u8>,
}

impl Script {
    fn press(&mut self, a: Action) {
        self.used.insert(a as u8);
        self.h.press(a);
    }

    fn to(&mut self, pos: GridPos) {
        for a in [
            Action::MoveUp,
            Action::MoveDown,
            Action::MoveLeft,
            Action::MoveRight,
        ] {
            self.used.insert(a as u8);
        }
        self.h.move_cursor_to(pos);
    }
}

/// すべての機能を順に使う: 路線を作る・のばす、強化、おすすめ、せってい、リセット。
fn session(gamepad: bool) -> (Vec<Command>, Vec<AppRequest>, BTreeSet<u8>, String) {
    let mut h = UiHarness::from_game(cheap_game(), false);
    if gamepad {
        h.use_gamepad();
    }
    h.frames(2);
    let mut s = Script {
        h,
        used: BTreeSet::new(),
    };
    s.press(Action::BuildMode);
    s.press(Action::Confirm);
    s.to(plan()[0]);
    s.press(Action::Confirm);
    s.to(plan()[2]);
    s.press(Action::ToggleBend);
    s.press(Action::Confirm);
    s.press(Action::Confirm);
    wait(&mut s.h, 300);
    s.press(Action::Confirm);
    assert!(matches!(play(&s.h).mode, PlayMode::Map));
    s.press(Action::TabNext);
    s.press(Action::TabPrev);
    s.press(Action::TabNext);
    s.press(Action::MoveRight);
    s.press(Action::MoveDown);
    s.press(Action::Confirm);
    s.press(Action::BuyMax);
    s.press(Action::JumpRecommend);
    s.press(Action::Confirm);
    let draw = s.h.draw_text();
    s.press(Action::Cancel);
    s.to(plan()[1]);
    s.press(Action::Confirm);
    s.press(Action::Cancel);
    s.press(Action::BuildMode);
    s.press(Action::MoveDown);
    s.press(Action::Confirm);
    s.to(plan()[2]);
    s.press(Action::Confirm);
    s.to(plan()[1]);
    s.press(Action::Confirm);
    wait(&mut s.h, 300);
    s.press(Action::Confirm);
    s.press(Action::Menu);
    s.press(Action::MoveDown);
    s.press(Action::MoveRight);
    for _ in 0..5 {
        s.press(Action::MoveDown);
    }
    s.press(Action::Confirm);
    wait(&mut s.h, 300);
    s.press(Action::MoveRight);
    s.press(Action::Confirm);
    wait(&mut s.h, 300);
    s.press(Action::MoveRight);
    s.press(Action::Confirm);
    (s.h.commands, s.h.requests, s.used, draw)
}

#[test]
fn keyboard_only_and_gamepad_only_reach_everything() {
    let (kc, kr, used, kdraw) = session(false);
    let (pc, pr, _, pdraw) = session(true);
    assert_eq!(used.len(), 13, "13 の Action をすべて使う");
    assert_eq!(kc, pc, "どちらの機器でも同じコマンド");
    assert_eq!(kr, pr);
    assert_eq!(kr, [AppRequest::ResetToTitle]);
    let kinds = |c: &[Command]| -> (usize, usize, usize) {
        (
            c.iter()
                .filter(|c| matches!(c, Command::CreateLine { .. }))
                .count(),
            c.iter()
                .filter(|c| matches!(c, Command::ExtendLine { .. }))
                .count(),
            c.iter()
                .filter(|c| matches!(c, Command::LevelUp { .. }))
                .count(),
        )
    };
    let (create, extend, levelup) = kinds(&kc);
    assert_eq!((create, extend), (1, 1));
    assert!(levelup >= 2, "{levelup}");
    assert!(kdraw.contains("btn_key_z") && !kdraw.contains("btn_pad_"));
    assert!(pdraw.contains("btn_pad_a") && !pdraw.contains("btn_key_"));
}

fn after(text: &str, marker: &str) -> String {
    let i = text
        .find(marker)
        .unwrap_or_else(|| panic!("{marker} がない"));
    text[i..].to_string()
}

/// 最後の静的な層(`DrawLayer`)の行から後(層の中身を除く)。
fn after_layers(text: &str) -> String {
    let i = text.rfind("DrawLayer ").expect("層");
    text[i..].to_string()
}

fn cheap_with_line() -> UiHarness {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    h
}

fn panel_screen() -> UiHarness {
    let mut h = cheap_with_line();
    h.frames(30);
    h.move_cursor_to(plan()[1]);
    h.press(Action::Confirm);
    h.press(Action::MoveDown);
    h.press(Action::MoveRight);
    wait(&mut h, 400);
    h
}

fn choose_screen() -> UiHarness {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(2);
    h.press(Action::BuildMode);
    h
}

fn create_screen() -> UiHarness {
    let mut h = choose_screen();
    h.press(Action::Confirm);
    h.move_cursor_to(plan()[0]);
    h.press(Action::Confirm);
    h.move_cursor_to(plan()[1]);
    h.press(Action::Confirm);
    h.move_cursor_to(plan()[2]);
    h.frames(1);
    h
}

fn confirm_screen() -> UiHarness {
    let mut h = create_screen();
    h.press(Action::Confirm);
    h.press(Action::Confirm);
    h
}

fn settings_screen() -> UiHarness {
    let mut h = cheap_with_line();
    h.press(Action::Menu);
    h.press(Action::MoveDown);
    h.press(Action::MoveRight);
    h
}

fn settings_row_screen(downs: usize) -> UiHarness {
    let mut h = settings_screen();
    for _ in 0..downs {
        h.press(Action::MoveDown);
    }
    h.press(Action::Confirm);
    h
}

fn reset_screen() -> UiHarness {
    settings_row_screen(5)
}

fn quit_screen() -> UiHarness {
    settings_row_screen(4)
}

#[test]
fn snapshot_panel() {
    let h = panel_screen();
    let text = h.draw_text();
    let panel = text
        .lines()
        .skip_while(|l| !l.starts_with(&sprite_at("ui_win_tl", PANEL.x, PANEL.y)))
        .take_while(|l| !l.starts_with(&fill_at(HUD)))
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("panel", panel);
}

#[test]
fn snapshot_build_choose() {
    let h = choose_screen();
    insta::assert_snapshot!("build_choose", after(&h.draw_text(), &fill_at(BAND_ROW2)));
}

#[test]
fn snapshot_build_create() {
    let h = create_screen();
    insta::assert_snapshot!("build_create", after_layers(&h.draw_text()));
}

#[test]
fn snapshot_line_confirm() {
    let h = confirm_screen();
    insta::assert_snapshot!("line_confirm", after(&h.draw_text(), &fill_at(BAND_ROW2)));
}

#[test]
fn snapshot_settings() {
    let h = settings_screen();
    insta::assert_snapshot!("settings", after(&h.draw_text(), &fill_at(BAND_ROW2)));
}

#[test]
fn snapshot_reset_confirm() {
    let h = reset_screen();
    insta::assert_snapshot!("reset_confirm", after(&h.draw_text(), &fill_at(BAND_ROW2)));
}

#[test]
fn snapshot_quit_confirm() {
    let h = quit_screen();
    insta::assert_snapshot!("quit_confirm", after(&h.draw_text(), &fill_at(BAND_ROW2)));
}

#[test]
#[ignore = "目で確かめるための画像を書く。TM_UI_DUMP_DIR がなければ何もしない"]
fn dump_control_screens() {
    let Ok(dir) = std::env::var("TM_UI_DUMP_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let mut pad = panel_screen();
    pad.use_gamepad();
    pad.press(Action::MoveDown);
    let mut extend = cheap_with_line();
    extend.press(Action::BuildMode);
    extend.press(Action::MoveDown);
    extend.press(Action::Confirm);
    let mut replay = settings_screen();
    replay.press(Action::MoveDown);
    replay.press(Action::MoveDown);
    replay.press(Action::Confirm);
    let mut reset2 = reset_screen();
    wait(&mut reset2, 300);
    reset2.press(Action::MoveRight);
    reset2.press(Action::Confirm);
    let mut quit2 = quit_screen();
    wait(&mut quit2, 300);
    quit2.press(Action::MoveRight);
    quit2.press(Action::Confirm);
    let mut rec = cheap_with_line();
    rec.frames(30);
    rec.press(Action::JumpRecommend);
    rec.press(Action::MoveDown);
    wait(&mut rec, 300);
    let mut error = create_screen();
    error.move_cursor_to(GridPos { x: 20, y: 14 });
    error.press(Action::Confirm);
    for (name, h) in [
        ("panel", &panel_screen()),
        ("panel_pad", &pad),
        ("panel_rec", &rec),
        ("build_choose", &choose_screen()),
        ("build_create", &create_screen()),
        ("build_error", &error),
        ("line_confirm", &confirm_screen()),
        ("extend_end", &extend),
        ("settings", &settings_screen()),
        ("replay", &replay),
        ("reset1", &reset_screen()),
        ("reset2", &reset2),
        ("quit1", &quit_screen()),
        ("quit2", &quit2),
    ] {
        super::raster::write_ppm(
            &dir.join(format!("{name}.ppm")),
            &super::raster::rasterize(h.draw_list()),
        );
    }
}
