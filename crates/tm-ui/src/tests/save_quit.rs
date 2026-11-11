//! タイトルのメニュー、「ゲームを終わる」、終了中、保存中の印(13 §10.2、IMP-807)。

use tm_assets::sound::{BGM_GAME_1, SE_CANCEL, SE_CONFIRM, SE_CURSOR, SE_MENU_CLOSE};
use tm_assets::txt;
use tm_input::Action;
use tm_platform_api::{AudioBus, AudioCmd, Rect, SoundId};
use tm_tutorial::{MenuAnchor, SettingsItem, UiEvent};

use super::screens::{menu_page, open_settings, play, select_item, ses, wait};
use super::{cheap_with_company, fill_at, harness_with_line, sprite_at};
use crate::hud::BandChips;
use crate::layout::{
    BAND_RIGHT, HUD_ROW2, POWER_BOX, SAVE_MARK, SAVING_BOX, SUB_CONFIRM, TITLE_ROWS,
};
use crate::scene::settings_menu::Page;
use crate::scene::{QuitBox, TitleItem};
use crate::sound_map::{bgm_for_level, bgm_rotation, bgm_stage};
use crate::testing::{FRAME_US, UiHarness};
use crate::{AppRequest, AppStatus, Settings};

const SAVE: AppStatus = AppStatus {
    save_enabled: true,
    has_save: true,
    saving: false,
    power_off_on_quit: false,
};

fn bgms(h: &UiHarness, from: usize) -> Vec<SoundId> {
    h.audio[from..]
        .iter()
        .filter_map(|c| match c {
            AudioCmd::PlayBgm { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

fn title(status: AppStatus) -> UiHarness {
    let mut h = UiHarness::new_title();
    h.set_app_status(status);
    h.frames(1);
    h
}

fn title_scene(h: &UiHarness) -> &crate::scene::TitleScene {
    h.ui.title().expect("タイトル")
}

fn parse_rect(l: &str) -> Rect {
    let mut it = l.split(' ').skip(2);
    let (x, y) = it.next().unwrap().split_once(',').unwrap();
    let (w, h) = it.next().unwrap().split_once('x').unwrap();
    Rect {
        x: x.parse().unwrap(),
        y: y.parse().unwrap(),
        w: w.parse().unwrap(),
        h: h.parse().unwrap(),
    }
}

fn frame_at(r: Rect) -> String {
    sprite_at("ui_sel_tl", r.x, r.y)
}

fn dialog_at() -> String {
    sprite_at("ui_win_tl", SUB_CONFIRM.x, SUB_CONFIRM.y)
}

#[test]
fn title_without_save_shows_only_new_game() {
    let mut h = title(AppStatus::default());
    assert_eq!(title_scene(&h).items(), [TitleItem::NewGame]);
    assert_eq!(title_scene(&h).selected, TitleItem::NewGame);
    let text = h.draw_text();
    assert!(
        text.contains(&frame_at(TITLE_ROWS[0])),
        "1 行目に「初めから」"
    );
    assert!(!text.contains(&frame_at(TITLE_ROWS[1])));
    assert!(text.contains("Sprite btn_key_z"), "下の帯の右に (A)選ぶ");
    let a = h.audio.len();
    h.press(Action::MoveDown);
    h.press(Action::MoveUp);
    assert!(ses(&h, a).is_empty(), "1 行なので動かない");
    wait(&mut h, 500);
    let a = h.audio.len();
    h.key_down(Action::Confirm);
    h.frames(1);
    assert_eq!(h.requests, [AppRequest::StartNewGame], "確認なしで始める");
    assert_eq!(ses(&h, a), [SE_CONFIRM]);
    assert_eq!(bgms(&h, a), [BGM_GAME_1]);
}

#[test]
fn title_with_save_starts_on_continue_and_moves_between_two_rows() {
    let mut h = title(SAVE);
    let t = title_scene(&h);
    assert_eq!(t.items(), [TitleItem::Continue, TitleItem::NewGame]);
    assert_eq!(t.selected, TitleItem::Continue, "初期の選択は「続きから」");
    let text = h.draw_text();
    assert!(text.contains(&frame_at(TITLE_ROWS[0])));
    assert!(!text.contains(&frame_at(TITLE_ROWS[1])));
    let a = h.audio.len();
    h.press(Action::MoveUp);
    assert!(ses(&h, a).is_empty(), "端で止まる(音なし)");
    h.press(Action::MoveDown);
    assert_eq!(title_scene(&h).selected, TitleItem::NewGame);
    assert_eq!(ses(&h, a), [SE_CURSOR]);
    assert!(h.draw_text().contains(&frame_at(TITLE_ROWS[1])));
    let a = h.audio.len();
    h.press(Action::MoveDown);
    assert!(ses(&h, a).is_empty(), "端で止まる(音なし)");
    h.press(Action::MoveUp);
    assert_eq!(title_scene(&h).selected, TitleItem::Continue);
}

#[test]
fn title_ignores_confirm_for_500ms_with_a_save() {
    let mut h = title(SAVE);
    h.press(Action::Confirm);
    assert!(h.requests.is_empty());
    wait(&mut h, 500);
    h.press(Action::Confirm);
    assert_eq!(h.requests, [AppRequest::ContinueGame]);
}

#[test]
fn continue_plays_the_company_level_song_on_the_first_visible_update() {
    let mut saved = cheap_with_company(5);
    let game = saved.game.take().expect("ゲーム");
    let level = game.view().company().level.0;
    assert_ne!(bgm_for_level(level), BGM_GAME_1, "段階 2 以上の曲");
    let mut h = title(SAVE);
    h.saved_game = Some(game);
    wait(&mut h, 500);
    let a = h.audio.len();
    h.key_down(Action::Confirm);
    h.frames(1);
    assert_eq!(h.requests, [AppRequest::ContinueGame]);
    assert_eq!(ses(&h, a), [SE_CONFIRM]);
    assert!(
        bgms(&h, a).is_empty(),
        "会社レベルが分からないので、まだ流さない"
    );
    let a = h.audio.len();
    h.key_up(Action::Confirm);
    h.frames(1);
    assert_eq!(bgms(&h, a), [bgm_for_level(level)], "続きの最初のフレーム");
    let a = h.audio.len();
    h.frames(5);
    assert!(bgms(&h, a).is_empty(), "1 回だけ");
    assert!(h.ui.map_cursor().is_some());
}

#[test]
fn go_to_title_forgets_a_pending_continue_song() {
    let mut h = title(SAVE);
    wait(&mut h, 500);
    h.key_down(Action::Confirm);
    h.frames(1);
    assert_eq!(h.requests, [AppRequest::ContinueGame]);
    h.reset_to_title();
    h.key_up(Action::Confirm);
    let a = h.audio.len();
    h.frames(3);
    assert!(bgms(&h, a).is_empty(), "タイトルの曲のまま");
    h.set_app_status(AppStatus::default());
    wait(&mut h, 500);
    let a = h.audio.len();
    h.press(Action::Confirm);
    h.frames(3);
    assert_eq!(h.requests.last(), Some(&AppRequest::StartNewGame));
    assert_eq!(bgms(&h, a), [BGM_GAME_1], "前の「続きから」の曲を出さない");
}

#[test]
fn new_game_with_a_save_needs_two_yes() {
    let mut h = title(SAVE);
    h.press(Action::MoveDown);
    wait(&mut h, 500);
    let a = h.audio.len();
    h.press(Action::Confirm);
    let t = title_scene(&h);
    let c = t.confirm.expect("確認 1 回目");
    assert_eq!((c.step, c.yes), (1, false), "初期位置は「いいえ」");
    assert_eq!(t.dialog_text(), Some(txt::TITLE_NEW_CONFIRM1));
    assert_eq!(ses(&h, a), [SE_CONFIRM]);
    let text = h.draw_text();
    assert!(text.contains(&dialog_at()), "リセット確認と同じ形の箱");
    assert!(
        !text.contains(&frame_at(TITLE_ROWS[0])) && !text.contains(&frame_at(TITLE_ROWS[1])),
        "確認の間はメニューを出さない"
    );
    h.press(Action::MoveRight);
    h.press(Action::Confirm);
    assert_eq!(
        title_scene(&h).confirm.map(|c| c.step),
        Some(1),
        "出てから 300ms は決定を受けない"
    );
    wait(&mut h, 300);
    let a = h.audio.len();
    h.press(Action::Confirm);
    let t = title_scene(&h);
    let c = t.confirm.expect("確認 2 回目");
    assert_eq!((c.step, c.yes), (2, false), "2 回目も「いいえ」から");
    assert_eq!(t.dialog_text(), Some(txt::RESET_CONFIRM2));
    assert_eq!(ses(&h, a), [SE_CONFIRM]);
    assert!(bgms(&h, a).is_empty());
    assert!(h.requests.is_empty(), "1 回目の「はい」では始めない");
    wait(&mut h, 300);
    h.press(Action::MoveRight);
    let a = h.audio.len();
    h.key_down(Action::Confirm);
    h.frames(1);
    assert_eq!(h.requests, [AppRequest::StartNewGame]);
    assert_eq!(ses(&h, a), [SE_CONFIRM]);
    assert_eq!(bgms(&h, a), [BGM_GAME_1]);
    assert!(h.game.is_some());
}

#[test]
fn every_way_out_of_the_title_confirm_returns_to_the_menu() {
    for step in [1u8, 2] {
        for exit in [Action::Cancel, Action::Confirm] {
            let mut h = title(SAVE);
            h.press(Action::MoveDown);
            wait(&mut h, 500);
            h.press(Action::Confirm);
            if step == 2 {
                wait(&mut h, 300);
                h.press(Action::MoveRight);
                h.press(Action::Confirm);
            }
            wait(&mut h, 300);
            let a = h.audio.len();
            h.press(exit);
            let t = title_scene(&h);
            assert_eq!(t.confirm, None, "段 {step} で {exit:?}");
            assert_eq!(t.selected, TitleItem::NewGame);
            let want = if exit == Action::Cancel {
                SE_CANCEL
            } else {
                SE_CONFIRM
            };
            assert_eq!(ses(&h, a), [want]);
            assert!(h.requests.is_empty());
            assert!(h.draw_text().contains(&frame_at(TITLE_ROWS[1])));
        }
    }
}

#[test]
fn title_hint_sits_in_the_band_and_changes_in_the_dialog() {
    let mut h = title(SAVE);
    let buttons = |h: &UiHarness, name: &str| -> Vec<Rect> {
        h.draw_text()
            .lines()
            .filter(|l| l.starts_with(&format!("Sprite {name} ")))
            .map(parse_rect)
            .collect()
    };
    let z = buttons(&h, "btn_key_z");
    assert_eq!(z.len(), 1, "(A)選ぶ");
    assert!(
        z[0].x >= BAND_RIGHT.x && z[0].y >= BAND_RIGHT.y,
        "{:?}",
        z[0]
    );
    assert!(buttons(&h, "btn_key_x").is_empty());
    h.press(Action::MoveDown);
    wait(&mut h, 500);
    h.press(Action::Confirm);
    let x = buttons(&h, "btn_key_x");
    assert_eq!(x.len(), 1, "確認の間は (B)戻る も出す");
    assert!(
        x[0].x >= BAND_RIGHT.x && x[0].y >= BAND_RIGHT.y,
        "{:?}",
        x[0]
    );
}

#[test]
fn losing_the_save_falls_back_to_new_game_only() {
    let mut h = title(SAVE);
    h.press(Action::MoveDown);
    wait(&mut h, 500);
    h.press(Action::Confirm);
    assert!(title_scene(&h).confirm.is_some());
    h.set_app_status(AppStatus::default());
    h.frames(1);
    let t = title_scene(&h);
    assert_eq!((t.selected, t.confirm), (TitleItem::NewGame, None));
    assert_eq!(t.items(), [TitleItem::NewGame]);
}

fn open_quit(h: &mut UiHarness) {
    open_settings(h);
    select_item(h, SettingsItem::Quit);
    h.press(Action::Confirm);
}

fn yes_twice(h: &mut UiHarness) {
    for _ in 0..2 {
        wait(h, 300);
        h.press(Action::MoveRight);
        h.press(Action::Confirm);
    }
}

#[test]
fn quit_needs_two_yes_and_then_ignores_everything() {
    let mut h = harness_with_line();
    let ev = h.ui_events.len();
    open_quit(&mut h);
    assert!(matches!(
        menu_page(&h),
        Some(Page::Quit {
            step: 1,
            yes: false,
            ..
        })
    ));
    assert!(
        h.ui_events[ev..].ends_with(&[UiEvent::MenuOpened(MenuAnchor::QuitConfirm { step: 1 })])
    );
    h.press(Action::MoveRight);
    h.press(Action::Confirm);
    assert!(
        matches!(menu_page(&h), Some(Page::Quit { step: 1, .. })),
        "出てから 300ms は決定を受けない"
    );
    wait(&mut h, 300);
    let a = h.audio.len();
    h.press(Action::Confirm);
    assert!(
        matches!(
            menu_page(&h),
            Some(Page::Quit {
                step: 2,
                yes: false,
                ..
            })
        ),
        "2 回目も「いいえ」から"
    );
    assert_eq!(ses(&h, a), [SE_CONFIRM]);
    assert!(h.ui_events.ends_with(&[
        UiEvent::MenuClosed(MenuAnchor::QuitConfirm { step: 1 }),
        UiEvent::MenuOpened(MenuAnchor::QuitConfirm { step: 2 }),
    ]));
    assert!(h.requests.is_empty());
    wait(&mut h, 300);
    h.press(Action::MoveRight);
    // 前の効果音が鳴り終わってから押す(同時 2 音の制限で音が落ちないように)
    wait(&mut h, 1500);
    let (a, ev) = (h.audio.len(), h.ui_events.len());
    h.press(Action::Confirm);
    assert_eq!(h.requests, [AppRequest::QuitGame]);
    assert_eq!(ses(&h, a), [SE_CONFIRM], "se_menu_close は鳴らさない");
    assert_eq!(h.ui_events.len(), ev, "UiEvent は出さない");
    assert!(play(&h).events.is_empty(), "MenuClosed も積まない");
    assert!(play(&h).quitting() && !play(&h).settings_open());
    assert!(h.ui.is_paused(h.tutorial.as_ref()));
    assert!(!h.ui.is_settings_open());

    let tick = h.game.as_ref().unwrap().view().tick();
    let (a, ev, cmds) = (h.audio.len(), h.ui_events.len(), h.commands.len());
    for act in [
        Action::Menu,
        Action::Confirm,
        Action::Cancel,
        Action::MoveLeft,
        Action::BuildMode,
        Action::TabNext,
        Action::JumpRecommend,
        Action::BuyMax,
    ] {
        h.press(act);
    }
    h.move_cursor_to(tm_core::GridPos { x: 0, y: 0 });
    assert!(ses(&h, a).is_empty(), "音も出さない");
    assert_eq!(h.ui_events.len(), ev);
    assert_eq!(h.commands.len(), cmds);
    assert_eq!(h.requests, [AppRequest::QuitGame], "要求は 1 回だけ");
    assert!(play(&h).quitting() && !play(&h).settings_open());
    assert_ne!(h.ui.map_cursor(), Some(tm_core::GridPos { x: 0, y: 0 }));
    assert_eq!(h.game.as_ref().unwrap().view().tick(), tick, "止まったまま");
    assert!(!h.audio[a..].contains(&AudioCmd::PlaySe(SE_MENU_CLOSE)));
    super::screens::play_mut(&mut h).cursor = super::plan()[0];
    h.frames(2);
    assert_eq!(h.ui_events.len(), ev, "駅の上に来ても CursorOn を出さない");
}

#[test]
fn quit_confirm_text_follows_save_enabled() {
    for (status, want) in [
        (SAVE, txt::QUIT_CONFIRM1),
        (AppStatus::default(), txt::QUIT_CONFIRM1_NOSAVE),
    ] {
        let mut h = harness_with_line();
        h.set_app_status(status);
        open_quit(&mut h);
        let menu = play(&h).settings_menu.as_ref().unwrap();
        assert_eq!(menu.confirm_text(), Some(want), "{status:?}");
        wait(&mut h, 300);
        h.press(Action::MoveRight);
        h.press(Action::Confirm);
        let menu = play(&h).settings_menu.as_ref().unwrap();
        assert_eq!(menu.confirm_text(), Some(txt::QUIT_CONFIRM2));
    }
}

#[test]
fn every_way_out_of_quit_confirm_returns_to_menu() {
    for step in [1u8, 2] {
        for exit in [Action::Cancel, Action::Menu, Action::Confirm] {
            let mut h = harness_with_line();
            open_quit(&mut h);
            if step == 2 {
                wait(&mut h, 300);
                h.press(Action::MoveRight);
                h.press(Action::Confirm);
            }
            wait(&mut h, 300);
            let a = h.audio.len();
            h.press(exit);
            assert_eq!(menu_page(&h), Some(Page::Top), "段 {step} で {exit:?}");
            assert!(play(&h).settings_open() && !play(&h).quitting());
            assert!(h.requests.is_empty());
            let want = if exit == Action::Confirm {
                SE_CONFIRM
            } else {
                SE_CANCEL
            };
            assert_eq!(ses(&h, a), [want]);
            assert!(
                h.ui_events
                    .ends_with(&[UiEvent::MenuClosed(MenuAnchor::QuitConfirm { step })])
            );
            assert_eq!(
                crate::scene::settings_menu::ITEMS[play(&h).settings_menu.as_ref().unwrap().sel],
                SettingsItem::Quit
            );
        }
    }
}

fn box_drawn(h: &UiHarness, r: Rect) -> bool {
    h.draw_text().contains(&sprite_at("ui_win_tl", r.x, r.y))
}

#[test]
fn quitting_shows_the_saving_box_then_the_power_box() {
    let mut h = harness_with_line();
    let mut status = AppStatus {
        power_off_on_quit: true,
        ..SAVE
    };
    h.set_app_status(status);
    open_quit(&mut h);
    wait(&mut h, 300);
    h.press(Action::MoveRight);
    h.press(Action::Confirm);
    wait(&mut h, 300);
    h.press(Action::MoveRight);
    h.key_down(Action::Confirm);
    h.frames(1);
    assert_eq!(h.requests, [AppRequest::QuitGame]);
    let at = play(&h).quit.as_ref().unwrap().at;
    assert_eq!(at, h.now, "2 回目の「はい」のフレーム");
    let text = h.draw_text();
    assert!(
        text.contains(&fill_at(crate::draw::rect(0, 0, 640, 360))),
        "うすい黒"
    );
    assert!(
        !box_drawn(&h, POWER_BOX),
        "保存を頼んだ直後(状態が古い)には電源の箱を出さない"
    );
    assert!(!text.contains("ui_win_tl 192,36"), "せってい は閉じた");
    h.key_up(Action::Confirm);
    status.saving = true;
    h.set_app_status(status);
    h.frames(1);
    assert!(box_drawn(&h, SAVING_BOX) && !box_drawn(&h, POWER_BOX));
    let q = play(&h).quit.as_ref().unwrap();
    assert_eq!(q.shown(h.now, status), QuitBox::Saving);
    let text = h.draw_text();
    let icon = text
        .lines()
        .find(|l| {
            l.starts_with("Sprite ico_save ")
                && !l.contains(&format!(" {},{} ", SAVE_MARK.x, SAVE_MARK.y))
        })
        .expect("箱の中の印");
    let r = parse_rect(icon);
    assert!(
        r.x >= SAVING_BOX.x && r.x + r.w as i32 <= SAVING_BOX.x + SAVING_BOX.w as i32,
        "{r:?}"
    );
    status.saving = false;
    h.set_app_status(status);
    h.frames(1);
    assert!(box_drawn(&h, POWER_BOX) && !box_drawn(&h, SAVING_BOX));
    status.power_off_on_quit = false;
    h.set_app_status(status);
    h.frames(1);
    assert!(
        !box_drawn(&h, POWER_BOX) && !box_drawn(&h, SAVING_BOX),
        "開発機は うすい黒だけ"
    );
    assert!(h.ui.is_paused(h.tutorial.as_ref()));
}

#[test]
fn quit_box_rules() {
    use crate::scene::QuitView;
    let q = QuitView::new(tm_platform_api::MonoTime(1_000));
    let later = tm_platform_api::MonoTime(1_000 + FRAME_US);
    let off = AppStatus {
        power_off_on_quit: true,
        ..AppStatus::default()
    };
    assert_eq!(q.shown(q.at, off), QuitBox::None);
    assert_eq!(
        q.shown(later, off),
        QuitBox::PowerOff,
        "保存しない設定でも電源を切る"
    );
    assert_eq!(
        q.shown(
            later,
            AppStatus {
                saving: true,
                ..off
            }
        ),
        QuitBox::Saving
    );
    assert_eq!(q.shown(later, AppStatus::default()), QuitBox::None);
    assert_eq!(
        q.shown(
            q.at,
            AppStatus {
                saving: true,
                ..AppStatus::default()
            }
        ),
        QuitBox::Saving
    );
}

fn save_marks(h: &UiHarness) -> usize {
    h.draw_text()
        .matches(&sprite_at("ico_save", SAVE_MARK.x, SAVE_MARK.y))
        .count()
}

fn goal_glyphs(h: &UiHarness) -> Vec<String> {
    let text = h.draw_text();
    text.lines()
        .skip_while(|l| !l.starts_with(&fill_at(crate::layout::HUD)))
        .filter(|l| l.starts_with("Sprite '") && l.contains(&format!(",{} ", HUD_ROW2.y)))
        .map(str::to_owned)
        .collect()
}

#[test]
fn save_mark_shows_while_saving_for_at_least_one_second() {
    let mut h = harness_with_line();
    h.frames(10);
    assert_eq!(save_marks(&h), 0);
    let goal = goal_glyphs(&h);
    assert!(!goal.is_empty());
    h.set_app_status(AppStatus {
        saving: true,
        ..SAVE
    });
    h.frames(1);
    assert_eq!(save_marks(&h), 1, "固定の枠に ico_save");
    assert_eq!(goal_glyphs(&h), goal, "目標の文は動かない");
    h.set_app_status(SAVE);
    h.frames(1);
    assert_eq!(save_marks(&h), 1, "最短 1 秒は残す");
    wait(&mut h, 900);
    assert_eq!(save_marks(&h), 1);
    wait(&mut h, 120);
    assert_eq!(save_marks(&h), 0, "1 秒を過ぎたら消す");
    assert_eq!(goal_glyphs(&h), goal);

    h.set_app_status(AppStatus {
        saving: true,
        ..SAVE
    });
    wait(&mut h, 2000);
    assert_eq!(save_marks(&h), 1, "書き込みが長ければその間ずっと");
    h.set_app_status(SAVE);
    h.frames(1);
    assert_eq!(save_marks(&h), 0, "1 秒を過ぎていればすぐ消す");
}

#[test]
fn save_mark_restarts_its_second_on_a_new_save() {
    let mut h = harness_with_line();
    let saving = AppStatus {
        saving: true,
        ..SAVE
    };
    h.set_app_status(saving);
    h.frames(1);
    h.set_app_status(SAVE);
    wait(&mut h, 800);
    h.set_app_status(saving);
    h.frames(1);
    h.set_app_status(SAVE);
    wait(&mut h, 500);
    assert_eq!(save_marks(&h), 1, "2 回目の出始めから 1 秒");
}

#[test]
fn settings_round_trip_and_set_before_boot_sets_the_volumes() {
    let mut h = UiHarness::new_title();
    assert_eq!(h.ui.settings(), Settings::default());
    let s = Settings {
        music: 3,
        sound: 12,
        show_explanations: false,
    };
    h.ui.set_settings(s);
    assert_eq!(
        h.ui.settings(),
        Settings { sound: 10, ..s },
        "音量は 0..=10 に収める"
    );
    h.frames(1);
    assert_eq!(
        h.audio[..2],
        [
            AudioCmd::SetVolume {
                bus: AudioBus::Bgm,
                level: 3
            },
            AudioCmd::SetVolume {
                bus: AudioBus::Se,
                level: 10
            },
        ]
    );
    assert!(!h.ui.show_explanations());
}

#[test]
fn is_settings_open_covers_sub_screens() {
    let mut h = UiHarness::new_title();
    h.frames(1);
    assert!(!h.ui.is_settings_open(), "タイトル");
    let mut h = harness_with_line();
    assert!(!h.ui.is_settings_open());
    open_quit(&mut h);
    assert!(h.ui.is_settings_open(), "終わる確認も せってい の中");
    h.press(Action::Menu);
    assert!(h.ui.is_settings_open());
    h.press(Action::Menu);
    assert!(!h.ui.is_settings_open());
}

#[test]
fn settings_changed_in_the_menu_are_read_back() {
    let mut h = harness_with_line();
    open_settings(&mut h);
    select_item(&mut h, SettingsItem::MusicVolume);
    h.press(Action::MoveLeft);
    assert_eq!(h.ui.settings().music, Settings::default().music - 1);
    select_item(&mut h, SettingsItem::Quit);
    h.press(Action::Confirm);
    yes_twice(&mut h);
    assert_eq!(h.requests, [AppRequest::QuitGame]);
    assert_eq!(h.ui.settings().music, Settings::default().music - 1);
}

fn read_stage_15(h: &mut UiHarness) {
    while play(h)
        .explain
        .shown
        .is_some_and(|s| s.directive.stage.0 == 15 && s.boxed)
    {
        wait(h, 320);
        h.press(Action::Confirm);
    }
}

fn quit_from_settings(h: &mut UiHarness) {
    open_settings(h);
    read_stage_15(h);
    select_item(h, SettingsItem::Quit);
    h.press(Action::Confirm);
    yes_twice(h);
    assert_eq!(h.requests.last(), Some(&AppRequest::QuitGame));
    assert!(play(h).quitting());
}

#[test]
fn no_fanfare_finished_while_quitting() {
    let mut h = harness_with_line();
    let now = h.now;
    h.ui.observe(
        now,
        &tm_core::GameEvent::CompanyLevelUp {
            new_level: tm_core::CompanyLevel(1),
            all_features_unlocked: false,
        },
        &mut Vec::new(),
    );
    quit_from_settings(&mut h);
    let ev = h.ui_events.len();
    wait(&mut h, 8000);
    assert_eq!(h.ui_events[ev..], [], "終了中に UiEvent を出さない");
}

#[test]
fn no_bgm_rotation_while_quitting() {
    let mut saved = cheap_with_company(5);
    let game = saved.game.take().expect("ゲーム");
    let stage = bgm_stage(game.view().company().level.0);
    let first_us = bgm_rotation(stage)[0].1;
    assert!(bgm_rotation(stage).len() > 1, "ローテーションのある段階");
    let mut h = title(SAVE);
    h.ui.set_settings(Settings {
        show_explanations: false,
        ..Settings::default()
    });
    h.saved_game = Some(game);
    wait(&mut h, 500);
    h.press(Action::Confirm);
    h.frames(2);
    open_settings(&mut h);
    select_item(&mut h, SettingsItem::Quit);
    h.press(Action::Confirm);
    wait(&mut h, 300);
    h.press(Action::MoveRight);
    h.press(Action::Confirm);
    wait(&mut h, first_us / 1000 - 1000);
    h.press(Action::MoveRight);
    h.press(Action::Confirm);
    assert!(play(&h).quitting());
    let a = h.audio.len();
    wait(&mut h, 3000);
    assert!(bgms(&h, a).is_empty(), "StopBgm の後に曲を流し直さない");
}

#[test]
fn quitting_draws_no_explanation_or_chips() {
    let mut h = harness_with_line();
    quit_from_settings(&mut h);
    let d = tm_tutorial::TutorialDirective {
        stage: tm_tutorial::TutorialStageId(6),
        kind: tm_tutorial::DirectiveKind::Explanation,
        message: txt::TUT_S06_P1,
        page: 0,
        page_count: 1,
        highlight: Some(tm_tutorial::UiAnchor::Station(tm_core::StationId(0))),
        pauses_game: true,
    };
    let now = h.now;
    let p = h.ui.play_mut().expect("ゲームの画面");
    p.fake_directive = Some(d);
    p.prepare_explain(now, h.game.as_ref().unwrap().view(), None);
    assert!(play(&h).explain.shown.is_some_and(|s| s.boxed));
    h.frames(1);
    let text = h.draw_text();
    assert!(!text.contains("ui_popup"), "説明の箱を描かない");
    assert!(!text.contains("fx_arrow"), "highlight を描かない");
    assert_eq!(text.matches(" #00000080").count(), 1, "うすい黒は 1 まい");
    assert_eq!(play(&h).band.chips(), BandChips::None);
    assert!(
        !text.contains("btn_key_b") && !text.contains("btn_key_e"),
        "下の帯のチップを出さない"
    );
}

#[test]
fn confirm_ignores_up_and_down() {
    let mut h = harness_with_line();
    open_quit(&mut h);
    h.press(Action::MoveRight);
    let a = h.audio.len();
    for act in [Action::MoveUp, Action::MoveDown] {
        h.press(act);
    }
    assert!(ses(&h, a).is_empty(), "上下では音も出さない");
    assert!(matches!(
        menu_page(&h),
        Some(Page::Quit {
            step: 1,
            yes: true,
            ..
        })
    ));
    assert_eq!(
        crate::scene::settings_menu::ITEMS[play(&h).settings_menu.as_ref().unwrap().sel],
        SettingsItem::Quit
    );

    let mut t = title(SAVE);
    t.press(Action::MoveDown);
    wait(&mut t, 500);
    t.press(Action::Confirm);
    t.press(Action::MoveRight);
    let a = t.audio.len();
    for act in [Action::MoveUp, Action::MoveDown] {
        t.press(act);
    }
    assert!(ses(&t, a).is_empty());
    let s = title_scene(&t);
    assert_eq!(s.confirm.map(|c| (c.step, c.yes)), Some((1, true)));
    assert_eq!(s.selected, TitleItem::NewGame);
}
