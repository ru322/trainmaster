//! 主な画面の `draw_text()` のスナップショット(13 §10.2)。

use super::{fill_at, harness_with_line, plan, sprite_at};
use tm_input::Action;

use crate::layout::{BAND_ROW2, BANNER, HUD, HUD_ROW2, INFO_ROW};
use crate::testing::UiHarness;

#[test]
fn title() {
    let mut h = UiHarness::new_title();
    h.frames(1);
    insta::assert_snapshot!("title", h.draw_text());
}

fn title_with_save() -> UiHarness {
    let mut h = UiHarness::new_title();
    h.set_app_status(crate::AppStatus {
        has_save: true,
        save_enabled: true,
        ..Default::default()
    });
    h.frames(1);
    h
}

#[test]
fn title_continue() {
    let h = title_with_save();
    insta::assert_snapshot!("title_continue", after_deco(&h.draw_text()));
}

#[test]
fn title_new_game_confirm() {
    let mut h = title_with_save();
    h.press(Action::MoveDown);
    h.frames(30);
    h.press(Action::Confirm);
    insta::assert_snapshot!("title_new_game_confirm", after_deco(&h.draw_text()));
}

/// タイトルのかざり(走る電車)より後(メニュー・確認・ヒント)。
fn after_deco(text: &str) -> String {
    let at = text.find("Sprite trn_e ").expect("かざりの電車");
    let rest = &text[at..];
    rest[rest.find('\n').unwrap() + 1..].to_owned()
}

fn quitting(status: crate::AppStatus) -> UiHarness {
    let mut h = harness_with_line();
    h.press(Action::Menu);
    for _ in 0..5 {
        h.press(Action::MoveDown);
    }
    h.press(Action::Confirm);
    for _ in 0..2 {
        h.frames(20);
        h.press(Action::MoveRight);
        h.press(Action::Confirm);
    }
    assert_eq!(h.requests, [crate::AppRequest::QuitGame]);
    h.set_app_status(status);
    h.frames(1);
    h
}

fn after_dim(text: &str) -> &str {
    &text[text.find("Fill 0,0 640x360 #00000080").expect("うすい黒")..]
}

#[test]
fn quitting_saving_box() {
    let h = quitting(crate::AppStatus {
        saving: true,
        save_enabled: true,
        ..Default::default()
    });
    insta::assert_snapshot!("quitting_saving", after_dim(&h.draw_text()));
}

#[test]
fn quitting_power_box() {
    let h = quitting(crate::AppStatus {
        power_off_on_quit: true,
        save_enabled: true,
        ..Default::default()
    });
    insta::assert_snapshot!("quitting_power_off", after_dim(&h.draw_text()));
}

#[test]
fn hud_with_save_mark() {
    let mut h = harness_with_line();
    h.set_app_status(crate::AppStatus {
        saving: true,
        save_enabled: true,
        ..Default::default()
    });
    h.frames(1);
    let text = h.draw_text();
    let in_row2 = |l: &&str| {
        l.split(' ')
            .nth(2)
            .and_then(|p| p.split_once(','))
            .and_then(|(_, y)| y.parse::<i32>().ok())
            .is_some_and(|y| (HUD_ROW2.y..HUD_ROW2.y + HUD_ROW2.h as i32).contains(&y))
    };
    let row2 = text
        .lines()
        .skip_while(|l| !l.starts_with(&fill_at(HUD)))
        .take_while(|l| !l.starts_with(&fill_at(INFO_ROW)))
        .filter(|l| l.starts_with("Sprite ") && in_row2(l))
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("hud_save_mark", row2);
}

#[test]
fn map_and_hud() {
    let mut h = harness_with_line();
    h.frames(240);
    h.move_cursor_to(plan()[0]);
    h.move_cursor_to(plan()[1]);
    h.frames(30);
    assert!(
        h.draw_text().contains("Sprite trn_") && h.draw_text().contains("BeginLayer 0 "),
        "路線の全体と列車が見える"
    );
    insta::assert_snapshot!("map_and_hud", h.draw_text());
}

#[test]
fn map_cursor_on_track_shows_the_line() {
    let mut h = harness_with_line();
    h.frames(30);
    h.move_cursor_to(tm_core::GridPos { x: 8, y: 4 });
    let text = h.draw_text();
    let info = text
        .lines()
        .skip_while(|l| !l.starts_with(&fill_at(INFO_ROW)))
        .take_while(|l| !l.starts_with(&fill_at(BAND_ROW2)))
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("info_line", info);
}

#[test]
fn hud_with_huge_numbers() {
    let mut h = super::budget::busiest();
    h.frames(1);
    let text = h.draw_text();
    let hud = text
        .lines()
        .skip_while(|l| !l.starts_with(&fill_at(HUD)))
        .take_while(|l| !l.starts_with(&fill_at(INFO_ROW)))
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("hud_huge_numbers", hud);
}

/// 会社レベルの帯(3 行)、トースト、ゴールの帯(13 §3.10 のワイヤーフレーム)。紙ふぶきは除く。
#[test]
fn banner_toast_and_goal() {
    let mut h = harness_with_line();
    let now = h.now;
    for ev in [
        tm_core::GameEvent::CompanyLevelUp {
            new_level: tm_core::CompanyLevel(3),
            all_features_unlocked: false,
        },
        tm_core::GameEvent::FeatureUnlocked {
            feature: tm_core::Feature::all_in_order().next().unwrap(),
            company_level: tm_core::CompanyLevel(3),
        },
        tm_core::GameEvent::MoneyUnitReached { unit: 3 },
        tm_core::GameEvent::GoalReached {
            total_earned: tm_numeric::BigNum::ONE,
        },
    ] {
        h.ui.observe(now, &ev, &mut Vec::new());
    }
    h.frames(18);
    let text = h.draw_text();
    let overlay = text
        .lines()
        .skip_while(|l| !l.starts_with(&sprite_at("ui_win_tl", BANNER.x, BANNER.y)))
        .filter(|l| !(l.starts_with("Fill ") && l.contains(" 4x4 ")))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!overlay.is_empty(), "帯が止まっている");
    insta::assert_snapshot!("banner_toast_goal", overlay);
}
