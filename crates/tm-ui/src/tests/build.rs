//! ろせんモード(13 §3.9、§10.2): 作る、のばす、曲がり方、確認、送信、`CommandRejected`。

use tm_assets::sound::{SE_CANCEL, SE_CONFIRM, SE_CURSOR, SE_ERROR};
use tm_assets::txt;
use tm_core::{Bend, Command, GameEvent, GridPos, LineEnd, LineId, StationId};
use tm_input::{Action, ActionFrame};
use tm_tutorial::{MenuAnchor, UiEvent};

use super::screens::{notice, play, play_mut, ses, wait};
use super::{cheap_game, cheap_with_company, create_line, harness_with_line, plan};
use crate::scene::PlayMode;
use crate::scene::build::{BuildState, Step};
use crate::testing::UiHarness;

fn build(h: &UiHarness) -> &BuildState {
    match &play(h).mode {
        PlayMode::Build(b) => b,
        _ => panic!("ろせんモードでない"),
    }
}

fn selected(h: &UiHarness) -> Vec<StationId> {
    match &build(h).step {
        Step::Create(d) => d.selected.clone(),
        s => panic!("作るでない: {s:?}"),
    }
}

fn pick(h: &mut UiHarness, i: usize) {
    h.move_cursor_to(plan()[i]);
    h.press(Action::Confirm);
}

fn start_create(h: &mut UiHarness) {
    h.press(Action::BuildMode);
    assert_eq!(
        build(h).step,
        Step::Choose { sel: 0 },
        "空き枠があれば 作る"
    );
    h.press(Action::Confirm);
    assert!(matches!(build(h).step, Step::Create(_)));
}

fn yes(h: &mut UiHarness) {
    wait(h, 300);
    h.press(Action::Confirm);
}

#[test]
fn create_with_two_stations() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    let ev = h.ui_events.len();
    start_create(&mut h);
    assert_eq!(play(&h).band.mode_text(), Some(txt::BUILD_CREATE_FIRST));
    pick(&mut h, 0);
    assert_eq!(play(&h).band.mode_text(), Some(txt::BUILD_CREATE_NEXT));
    pick(&mut h, 1);
    assert_eq!(selected(&h), [StationId(0), StationId(1)]);
    h.press(Action::Confirm);
    let c = build(&h).confirm.as_ref().expect("確認");
    assert!(c.yes, "初期のカーソルは はい");
    assert!(!c.top, "完成図が上半分なら下に出す");
    h.press(Action::Confirm);
    assert!(h.commands.is_empty(), "出てから 300ms は決定を受けない");
    wait(&mut h, 400);
    let mark = h.audio.len();
    yes(&mut h);
    let view = h.game.as_ref().unwrap().view();
    assert_eq!(
        h.commands,
        [Command::CreateLine {
            stations: vec![StationId(0), StationId(1)],
            bends: vec![view.default_bend(StationId(0), StationId(1))]
        }]
    );
    assert!(!ses(&h, mark).contains(&SE_CONFIRM), "確定の「はい」は無音");
    assert!(
        matches!(play(&h).mode, PlayMode::Map),
        "結果を受けてマップへ"
    );
    assert_eq!(view.lines().len(), 1);
    let menus: Vec<UiEvent> = h.ui_events[ev..]
        .iter()
        .copied()
        .filter(|e| !matches!(e, UiEvent::CursorOn(_)))
        .collect();
    assert_eq!(
        menus,
        [
            UiEvent::MenuOpened(MenuAnchor::BuildMode),
            UiEvent::MenuOpened(MenuAnchor::BuildCreate),
            UiEvent::MenuOpened(MenuAnchor::LineConfirm),
            UiEvent::MenuClosed(MenuAnchor::LineConfirm),
            UiEvent::MenuClosed(MenuAnchor::BuildMode)
        ]
    );
}

#[test]
fn toggle_bend_changes_the_newest_visible_line_and_the_command_matches() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    start_create(&mut h);
    pick(&mut h, 0);
    h.move_cursor_to(plan()[2]);
    h.frames(1);
    let view = h.game.as_ref().unwrap().view();
    let d02 = view.default_bend(StationId(0), StationId(2));
    wait(&mut h, 400);
    let mark = h.audio.len();
    h.press(Action::ToggleBend);
    assert_eq!(ses(&h, mark), [SE_CURSOR]);
    match &build(&h).step {
        Step::Create(d) => assert_eq!(
            d.hover,
            Some((StationId(2), d02.reversed())),
            "カーソルの先の点線を切り替える"
        ),
        _ => unreachable!(),
    }
    h.press(Action::Confirm);
    h.move_cursor_to(GridPos { x: 20, y: 15 });
    h.press(Action::ToggleBend);
    match &build(&h).step {
        Step::Create(d) => assert_eq!(d.bends, [d02], "点線がなければ最後に決めた区間"),
        _ => unreachable!(),
    }
    h.press(Action::ToggleBend);
    pick(&mut h, 1);
    h.press(Action::Confirm);
    yes(&mut h);
    let view = h.game.as_ref().unwrap().view();
    let bends = vec![
        d02.reversed(),
        view.default_bend(StationId(2), StationId(1)),
    ];
    assert_eq!(
        h.commands,
        [Command::CreateLine {
            stations: vec![StationId(0), StationId(2), StationId(1)],
            bends: bends.clone()
        }]
    );
    let line = view.line(LineId(0)).expect("路線");
    let mut want = Vec::new();
    view.preview_track(StationId(0), StationId(2), bends[0], &mut want);
    assert_eq!(line.segments[0].tiles, want, "完成図と同じ線路");
    let mut other = Vec::new();
    view.preview_track(StationId(0), StationId(2), d02, &mut other);
    assert_ne!(want, other, "曲がり方で線路が変わる配置で確かめる");
}

#[test]
fn same_station_empty_tile_and_cancel() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    start_create(&mut h);
    pick(&mut h, 0);
    wait(&mut h, 400);
    let mark = h.audio.len();
    h.press(Action::Confirm);
    assert_eq!(ses(&h, mark), [SE_ERROR]);
    assert_eq!(notice(&h), Some(txt::ERR_DUPLICATE));
    h.move_cursor_to(GridPos { x: 20, y: 15 });
    h.press(Action::Confirm);
    assert_eq!(notice(&h), Some(txt::ERR_NOT_STATION));
    pick(&mut h, 1);
    pick(&mut h, 0);
    assert_eq!(
        notice(&h),
        Some(txt::ERR_DUPLICATE),
        "同じ駅は 2 回えらべない"
    );
    wait(&mut h, 400);
    let mark = h.audio.len();
    h.press(Action::Cancel);
    assert_eq!(selected(&h), [StationId(0)], "最後の駅を戻す");
    wait(&mut h, 400);
    h.press(Action::Cancel);
    assert_eq!(selected(&h), [] as [StationId; 0]);
    assert_eq!(ses(&h, mark), [SE_CANCEL, SE_CANCEL]);
    let ev = h.ui_events.len();
    h.press(Action::Cancel);
    assert!(
        matches!(play(&h).mode, PlayMode::Map),
        "0 駅で Cancel なら終える"
    );
    assert_eq!(
        h.ui_events[ev..],
        [UiEvent::MenuClosed(MenuAnchor::BuildMode)]
    );
    assert!(h.commands.is_empty());
}

#[test]
fn permits_is_asked_before_adding_a_station() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    play_mut(&mut h).forbid = Some(
        |c: &Command| !matches!(c, Command::CreateLine { stations, .. } if stations.len() > 2),
    );
    start_create(&mut h);
    pick(&mut h, 0);
    pick(&mut h, 1);
    wait(&mut h, 400);
    let mark = h.audio.len();
    pick(&mut h, 2);
    assert_eq!(
        selected(&h),
        [StationId(0), StationId(1)],
        "3 駅目は選べない"
    );
    assert_eq!(ses(&h, mark), [SE_ERROR]);
    assert_eq!(notice(&h), Some(txt::ERR_TUTORIAL));
}

#[test]
fn line_confirm_no_and_cancel_go_back_to_the_draft() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    start_create(&mut h);
    pick(&mut h, 0);
    pick(&mut h, 1);
    h.press(Action::Confirm);
    wait(&mut h, 400);
    let mark = h.audio.len();
    h.press(Action::MoveLeft);
    assert!(!build(&h).confirm.as_ref().unwrap().yes);
    assert_eq!(ses(&h, mark), [SE_CURSOR]);
    h.press(Action::MoveLeft);
    assert_eq!(ses(&h, mark), [SE_CURSOR], "端で止まる");
    wait(&mut h, 300);
    let ev = h.ui_events.len();
    wait(&mut h, 400);
    let mark = h.audio.len();
    h.press(Action::Confirm);
    assert!(build(&h).confirm.is_none());
    assert_eq!(ses(&h, mark), [SE_CONFIRM]);
    assert_eq!(selected(&h), [StationId(0), StationId(1)], "作りかけに戻る");
    assert_eq!(
        h.ui_events[ev..],
        [UiEvent::MenuClosed(MenuAnchor::LineConfirm)]
    );
    h.press(Action::Confirm);
    assert!(
        build(&h).confirm.as_ref().unwrap().yes,
        "開き直すと はい から"
    );
    wait(&mut h, 400);
    let mark = h.audio.len();
    h.press(Action::Cancel);
    assert!(build(&h).confirm.is_none());
    assert_eq!(ses(&h, mark), [SE_CANCEL]);
    h.press(Action::BuildMode);
    assert!(matches!(play(&h).mode, PlayMode::Map));
    assert!(h.commands.is_empty());
}

#[test]
fn choose_defaults_and_grey_choices() {
    let mut h = harness_with_line();
    h.press(Action::BuildMode);
    assert_eq!(
        build(&h).step,
        Step::Choose { sel: 1 },
        "空き枠がなければ のばす"
    );
    wait(&mut h, 400);
    let mark = h.audio.len();
    h.press(Action::MoveUp);
    h.press(Action::Confirm);
    assert_eq!(ses(&h, mark), [SE_CURSOR, SE_ERROR]);
    assert_eq!(notice(&h), Some(txt::ERR_NO_SLOT));
    assert_eq!(build(&h).step, Step::Choose { sel: 0 });
    h.press(Action::BuildMode);
    assert!(matches!(play(&h).mode, PlayMode::Map), "BuildMode で終える");

    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    h.press(Action::BuildMode);
    h.press(Action::MoveDown);
    h.press(Action::MoveDown);
    assert_eq!(build(&h).step, Step::Choose { sel: 1 }, "端で止まる");
    h.press(Action::Confirm);
    assert_eq!(notice(&h), Some(txt::ERR_NO_LINE));
    h.press(Action::Cancel);
    assert!(matches!(play(&h).mode, PlayMode::Map));
}

fn start_extend(h: &mut UiHarness) {
    h.press(Action::BuildMode);
    if build(h).step == (Step::Choose { sel: 0 }) {
        h.press(Action::MoveDown);
    }
    h.press(Action::Confirm);
    assert_eq!(build(h).step, Step::PickEnd);
}

#[test]
fn extend_from_an_end() {
    let mut h = cheap_with_company(1);
    let ev = h.ui_events.len();
    start_extend(&mut h);
    assert!(h.ui_events[ev..].contains(&UiEvent::MenuOpened(MenuAnchor::BuildExtend)));
    pick(&mut h, 1);
    assert_eq!(notice(&h), Some(txt::ERR_NOT_END), "端でない駅");
    assert_eq!(build(&h).step, Step::PickEnd);
    h.move_cursor_to(GridPos { x: 20, y: 16 });
    h.press(Action::Confirm);
    assert_eq!(notice(&h), Some(txt::ERR_NOT_END), "駅でないマス");
    pick(&mut h, 0);
    assert!(matches!(
        build(&h).step,
        Step::PickNew(d) if d.line == LineId(0) && d.end == LineEnd::Head && d.from == StationId(0)
    ));
    h.press(Action::Cancel);
    assert_eq!(
        build(&h).step,
        Step::PickEnd,
        "つなぐ駅の前なら 端を選ぶ に戻る"
    );
    pick(&mut h, 0);
    pick(&mut h, 1);
    assert_eq!(notice(&h), Some(txt::ERR_ALREADY_ON_LINE));
    h.move_cursor_to(plan()[3]);
    h.frames(1);
    h.press(Action::ToggleBend);
    let view = h.game.as_ref().unwrap().view();
    let bend = view.default_bend(StationId(0), StationId(3)).reversed();
    h.press(Action::Confirm);
    assert!(build(&h).confirm.is_some());
    yes(&mut h);
    assert_eq!(
        h.commands.last(),
        Some(&Command::ExtendLine {
            line: LineId(0),
            end: LineEnd::Head,
            station: StationId(3),
            bend
        })
    );
    assert!(matches!(play(&h).mode, PlayMode::Map));
    assert_eq!(
        h.game
            .as_ref()
            .unwrap()
            .view()
            .line(LineId(0))
            .unwrap()
            .stations[0],
        StationId(3)
    );
}

#[test]
fn an_end_of_two_lines_asks_which_line() {
    let mut h = cheap_with_company(2);
    super::buy_all(&mut h);
    h.frames(2);
    let g = h.game.as_mut().unwrap();
    let cmd = create_line(&[2, 3]);
    g.validate(&cmd).expect("2 本目を作れる");
    g.enqueue(cmd);
    h.frames(2);
    start_extend(&mut h);
    pick(&mut h, 2);
    assert!(matches!(&build(&h).step, Step::Which { cands, idx: 0, .. } if cands.len() == 2));
    assert_eq!(play(&h).band.mode_text(), Some(txt::BUILD_EXTEND_WHICH));
    wait(&mut h, 400);
    let mark = h.audio.len();
    h.press(Action::MoveRight);
    assert_eq!(ses(&h, mark), [SE_CURSOR]);
    h.press(Action::Confirm);
    assert!(matches!(
        build(&h).step,
        Step::PickNew(d) if d.line == LineId(1) && d.end == LineEnd::Head && d.from == StationId(2)
    ));
    h.press(Action::Cancel);
    pick(&mut h, 2);
    h.press(Action::Cancel);
    assert_eq!(build(&h).step, Step::PickEnd);
    h.press(Action::Cancel);
    assert!(matches!(play(&h).mode, PlayMode::Map));
}

fn pressed(a: Action) -> ActionFrame {
    let mut f = ActionFrame::default();
    f.pressed.insert(a);
    f.held.insert(a);
    f
}

/// ゲームを進めずに 1 回だけ `update` を呼ぶ(送信の結果がまだ届かない間を作る)。
fn update_only(h: &mut UiHarness, f: ActionFrame) -> Vec<Command> {
    h.now = tm_platform_api::MonoTime(h.now.0 + crate::testing::FRAME_US);
    let mut cmds = Vec::new();
    let (game, tutorial) = (h.game.as_ref().unwrap(), h.tutorial.as_mut());
    h.ui.update(
        h.now,
        &f,
        Some(game.view()),
        tutorial,
        &mut cmds,
        &mut Vec::new(),
        &mut Vec::new(),
    );
    cmds
}

#[test]
fn submitting_ignores_confirm_and_waits_for_the_result() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    start_create(&mut h);
    pick(&mut h, 0);
    pick(&mut h, 1);
    h.press(Action::Confirm);
    wait(&mut h, 300);
    let sent = update_only(&mut h, pressed(Action::Confirm));
    assert_eq!(sent.len(), 1);
    assert!(build(&h).confirm.as_ref().unwrap().submitting);
    update_only(&mut h, ActionFrame::default());
    assert!(
        update_only(&mut h, pressed(Action::Confirm)).is_empty(),
        "二重に送らない"
    );
    assert!(update_only(&mut h, pressed(Action::Cancel)).is_empty());
    assert!(build(&h).confirm.is_some(), "送信中は確認を出したまま");
    let now = h.now;
    h.ui.observe(
        now,
        &GameEvent::LineCreated {
            line: LineId(0),
            station_count: 2,
            cost: tm_core::Money::ZERO,
            new_tiles: 0,
        },
        &mut Vec::new(),
    );
    assert!(matches!(play(&h).mode, PlayMode::Map));
}

#[test]
fn rejected_create_goes_back_to_the_draft() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    start_create(&mut h);
    pick(&mut h, 0);
    pick(&mut h, 1);
    pick(&mut h, 2);
    h.press(Action::Confirm);
    wait(&mut h, 300);
    h.game.as_mut().unwrap().enqueue(create_line(&[1, 2]));
    let ev = h.ui_events.len();
    h.press(Action::Confirm);
    assert_eq!(h.commands.len(), 1, "UI は送った");
    assert!(
        matches!(play(&h).mode, PlayMode::Build(_)),
        "ろせんモードのまま"
    );
    assert!(build(&h).confirm.is_none());
    assert_eq!(
        selected(&h),
        [StationId(0), StationId(1), StationId(2)],
        "作るに戻る"
    );
    assert_eq!(notice(&h), Some(txt::ERR_NO_SLOT));
    assert_eq!(
        h.ui_events[ev..],
        [UiEvent::MenuClosed(MenuAnchor::LineConfirm)]
    );
}

#[test]
fn rejected_extend_goes_back_to_choosing_the_new_station() {
    let mut h = cheap_with_company(1);
    start_extend(&mut h);
    pick(&mut h, 2);
    pick(&mut h, 3);
    assert!(build(&h).confirm.is_some());
    wait(&mut h, 300);
    h.game.as_mut().unwrap().enqueue(Command::ExtendLine {
        line: LineId(0),
        end: LineEnd::Head,
        station: StationId(3),
        bend: Bend::StraightFirst,
    });
    h.press(Action::Confirm);
    assert!(build(&h).confirm.is_none());
    assert!(matches!(build(&h).step, Step::PickNew(d) if d.from == StationId(2)));
    assert_eq!(notice(&h), Some(txt::ERR_ALREADY_ON_LINE));
}

#[test]
fn build_mode_keeps_the_game_running() {
    let mut h = harness_with_line();
    h.press(Action::BuildMode);
    let t0 = h.game.as_ref().unwrap().view().tick();
    h.frames(10);
    assert_eq!(h.game.as_ref().unwrap().view().tick().0, t0.0 + 10);
    assert!(!h.ui.is_paused(h.tutorial.as_ref()));
}

#[test]
fn cancel_then_pick_another_station_sends_matching_bends() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    start_create(&mut h);
    pick(&mut h, 0);
    pick(&mut h, 1);
    h.press(Action::Cancel);
    pick(&mut h, 2);
    h.press(Action::Confirm);
    yes(&mut h);
    let view = h.game.as_ref().unwrap().view();
    let bend = view.default_bend(StationId(0), StationId(2));
    assert_eq!(
        h.commands,
        [Command::CreateLine {
            stations: vec![StationId(0), StationId(2)],
            bends: vec![bend]
        }],
        "戻した駅の曲がり方は残さない"
    );
    let mut want = Vec::new();
    view.preview_track(StationId(0), StationId(2), bend, &mut want);
    assert_eq!(view.line(LineId(0)).expect("路線").segments[0].tiles, want);
}

/// パラメータは 1 円・倍率 1、線路は 1 マス `c1` 円(倍率 1)、所持金 100 万円の表で、路線 [0, 1, 2] を作って会社 Lv2 にした画面。
fn priced(c1: u64) -> UiHarness {
    let mut spec = tm_core::BalanceTable::builtin().spec().clone();
    for p in &mut spec.params {
        p.base_yen = 1;
        p.cost_ratio = 1;
        p.cost_steps.clear();
    }
    spec.economy.initial_money_yen = 1_000_000;
    spec.economy.track.c1_yen = c1;
    spec.economy.track.level_ratio = 1;
    spec.economy.track.tile_ratio = (1, 1);
    let table = tm_core::BalanceTable::from_spec(spec).expect("検証を通る表");
    let game = tm_core::Game::with_station_plan(
        tm_core::GameConfig {
            seed: 7,
            balance: std::sync::Arc::new(table),
        },
        plan(),
    );
    let mut h = UiHarness::from_game(game, false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    while h.game.as_ref().unwrap().view().company().level.0 < 2 {
        super::buy_all(&mut h);
        h.frames(2);
    }
    h
}

fn yen(v: tm_core::Money) -> String {
    tm_numeric::format_ja(v, &tm_numeric::FormatStyle::DEFAULT)
}

fn info_lines(h: &UiHarness) -> String {
    use super::fill_at;
    use crate::layout::{BAND_ROW2, INFO_ROW};
    let text = h.draw_text();
    text.lines()
        .skip_while(|l| !l.starts_with(&fill_at(INFO_ROW)))
        .take_while(|l| !l.starts_with(&fill_at(BAND_ROW2)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 情報の段の字(空白は描かないので詰まる)。
fn info_chars(h: &UiHarness) -> String {
    info_lines(h)
        .lines()
        .filter_map(|l| l.strip_prefix("Sprite '")?.split('\'').next())
        .collect()
}

fn validate(
    h: &UiHarness,
    cmd: &Command,
) -> Result<tm_core::CommandPreview, tm_core::CommandError> {
    h.game.as_ref().unwrap().view().validate(cmd)
}

fn extend_cmd(h: &UiHarness, from: u32, to: u32, end: LineEnd, toggled: bool) -> Command {
    let view = h.game.as_ref().unwrap().view();
    let bend = view.default_bend(StationId(from), StationId(to));
    Command::ExtendLine {
        line: LineId(0),
        end,
        station: StationId(to),
        bend: if toggled { bend.reversed() } else { bend },
    }
}

/// 描いた路線の確認の箱の高さ。
fn dialog_h(h: &UiHarness) -> u32 {
    let text = h.draw_text();
    let x = crate::layout::LINE_CONFIRM_X;
    let find = |part: &str| {
        let head = format!("Sprite {part} {x},");
        let l = text
            .lines()
            .rev()
            .find(|l| l.starts_with(&head))
            .expect("確認の箱");
        let mut f = l[head.len()..].split([' ', 'x']);
        let y: i32 = f.next().unwrap().parse().unwrap();
        let h: i32 = f.nth(1).unwrap().parse().unwrap();
        (y, h)
    };
    let ((top, _), (bl, bh)) = (find("ui_win_tl"), find("ui_win_bl"));
    (bl + bh - top) as u32
}

/// 13 §10.2 線路の値段: 値段は `validate` の結果(`preview.cost`)のままで、カーソルの先・曲がり方に合わせて作り直す。
/// 表は線路 1 マス 1 円・会社 Lv2 以上(`cheap_with_company`)なので、値段は新しいマスの数の円。
#[test]
fn track_cost_follows_validate() {
    let mut h = cheap_with_company(2);
    start_extend(&mut h);
    pick(&mut h, 0);
    assert_eq!(
        build(&h).cost.cost,
        None,
        "カーソルの先が駅でなければ出さない"
    );
    h.move_cursor_to(plan()[4]);
    h.frames(1);
    let p = validate(&h, &extend_cmd(&h, 0, 4, LineEnd::Head, false)).expect("足りる");
    assert!(!p.cost.is_zero() && p.new_track_tiles > 0);
    assert_eq!(build(&h).cost.cost, Some(p.cost));
    assert_eq!(build(&h).cost.short, None);
    assert_eq!(info_chars(&h), format!("線路のお金{}", yen(p.cost)));
    h.press(Action::ToggleBend);
    let q = validate(&h, &extend_cmd(&h, 0, 4, LineEnd::Head, true)).expect("足りる");
    assert_ne!(p.cost, q.cost, "曲がり方で新しいマスが変わる配置で確かめる");
    assert_eq!(
        build(&h).cost.cost,
        Some(q.cost),
        "曲がり方を変えたら作り直す"
    );
    h.move_cursor_to(GridPos { x: 20, y: 16 });
    assert_eq!(build(&h).cost.cost, None);
    assert!(
        !info_chars(&h).starts_with("線路のお金"),
        "値段がなければ ふつうの情報の段"
    );
    h.move_cursor_to(plan()[4]);
    h.press(Action::ToggleBend);
    h.press(Action::Confirm);
    let c = build(&h).confirm.as_ref().expect("確認");
    assert_eq!(c.cost, q.cost);
    assert_eq!(
        dialog_h(&h),
        crate::layout::line_confirm_h(2 + u8::from(c.pass)),
        "値段の行を足す"
    );
    assert_eq!(build(&h).cost.cost, Some(q.cost), "確認の間も出す");
}

/// 作るでは、カーソルの先の駅まで足したコマンド、なければ決めた駅だけ(2 駅以上)の値段。
#[test]
fn create_cost_uses_the_hovered_station_or_the_chosen_ones() {
    let mut h = priced(1);
    start_create(&mut h);
    pick(&mut h, 3);
    h.move_cursor_to(GridPos { x: 20, y: 16 });
    assert_eq!(build(&h).cost.cost, None, "1 駅だけなら出さない");
    pick(&mut h, 4);
    h.move_cursor_to(plan()[2]);
    h.frames(1);
    let view = h.game.as_ref().unwrap().view();
    let b34 = view.default_bend(StationId(3), StationId(4));
    let b42 = view.default_bend(StationId(4), StationId(2));
    let three = Command::CreateLine {
        stations: vec![StationId(3), StationId(4), StationId(2)],
        bends: vec![b34, b42],
    };
    let two = Command::CreateLine {
        stations: vec![StationId(3), StationId(4)],
        bends: vec![b34],
    };
    let (p3, p2) = (validate(&h, &three).unwrap(), validate(&h, &two).unwrap());
    assert_ne!(p3.cost, p2.cost);
    assert_eq!(build(&h).cost.cost, Some(p3.cost));
    h.move_cursor_to(GridPos { x: 20, y: 16 });
    assert_eq!(build(&h).cost.cost, Some(p2.cost));
    h.move_cursor_to(plan()[4]);
    h.press(Action::Confirm);
    assert_eq!(build(&h).confirm.as_ref().expect("確認").cost, p2.cost);
    assert_eq!(
        build(&h).cost.cost,
        Some(p2.cost),
        "確認の間も情報の段に出す"
    );
    assert_eq!(info_chars(&h), format!("線路のお金{}", yen(p2.cost)));
}

/// 値段が 0 円(会社 Lv0)なら情報の段にも確認にも出さない。
#[test]
fn zero_cost_is_hidden() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    start_create(&mut h);
    pick(&mut h, 0);
    h.move_cursor_to(plan()[1]);
    h.frames(1);
    let view = h.game.as_ref().unwrap().view();
    let cmd = Command::CreateLine {
        stations: vec![StationId(0), StationId(1)],
        bends: vec![view.default_bend(StationId(0), StationId(1))],
    };
    assert!(validate(&h, &cmd).unwrap().cost.is_zero());
    assert_eq!(build(&h).cost.cost, None);
    assert!(info_chars(&h).starts_with('駅'), "ふつうの情報の段");
    h.press(Action::Confirm);
    h.press(Action::Confirm);
    assert!(build(&h).confirm.as_ref().expect("確認").cost.is_zero());
    assert_eq!(dialog_h(&h), crate::layout::line_confirm_h(1));
}

/// 足りないとき: 駅は足せる、札は灰色、仕上げで `se_cannot_buy` と `err.money`。所持金が届けば同じ下書きで仕上げられる。
#[test]
fn short_money_blocks_only_the_finish_until_money_arrives() {
    let mut h = priced(200_000);
    start_create(&mut h);
    pick(&mut h, 3);
    h.move_cursor_to(plan()[4]);
    h.frames(1);
    let view = h.game.as_ref().unwrap().view();
    let cmd = Command::CreateLine {
        stations: vec![StationId(3), StationId(4)],
        bends: vec![view.default_bend(StationId(3), StationId(4))],
    };
    let (required, available) = match validate(&h, &cmd) {
        Err(tm_core::CommandError::InsufficientFunds {
            required,
            available,
        }) => (required, available),
        r => panic!("足りないはず: {r:?}"),
    };
    let short = required.checked_sub(available).unwrap();
    assert_eq!(build(&h).cost.cost, Some(required));
    assert_eq!(build(&h).cost.short, Some(short));
    assert_eq!(
        info_chars(&h),
        format!(
            "線路のお金{}お金が足りないよ。あと{}",
            yen(required),
            yen(short)
        )
    );
    assert!(info_lines(&h).contains("Sprite ico_coin_x "));
    wait(&mut h, 300);
    let mark = h.audio.len();
    pick(&mut h, 4);
    assert_eq!(
        selected(&h),
        [StationId(3), StationId(4)],
        "駅を足すのは断らない"
    );
    assert_eq!(ses(&h, mark), [SE_CONFIRM]);
    assert!(build(&h).cost.finish_short);
    let grey = format!("#{:06X}FF", 0xA6AEC0);
    assert!(
        h.draw_text()
            .lines()
            .any(|l| l.starts_with("Sprite '完'12") && l.ends_with(&grey)),
        "「もう一度(A)で完成」を灰色に"
    );
    wait(&mut h, 300);
    let mark = h.audio.len();
    h.press(Action::Confirm);
    assert!(build(&h).confirm.is_none(), "確認へ進まない");
    assert_eq!(ses(&h, mark), [tm_assets::sound::SE_CANNOT_BUY]);
    assert_eq!(notice(&h), Some(txt::ERR_MONEY));
    for _ in 0..100 {
        if h.game.as_ref().unwrap().view().money() >= required {
            break;
        }
        h.frames(60);
    }
    assert!(
        h.game.as_ref().unwrap().view().money() >= required,
        "所持金が届く"
    );
    h.frames(6);
    assert_eq!(build(&h).cost.short, None, "10 Hz で作り直す");
    assert!(!build(&h).cost.finish_short);
    h.press(Action::Confirm);
    assert!(build(&h).confirm.is_some(), "同じ下書きのまま仕上げられる");
    yes(&mut h);
    assert_eq!(h.commands.last(), Some(&cmd));
    assert!(matches!(play(&h).mode, PlayMode::Map));
}

/// 3 駅目にカーソルがあっても、札の灰色は決めた駅だけの値段で決める。
#[test]
fn finish_tag_uses_the_chosen_stations_only() {
    let mut h = priced(125_000);
    start_create(&mut h);
    pick(&mut h, 3);
    pick(&mut h, 4);
    let money = h.game.as_ref().unwrap().view().money();
    let two = build(&h).cost.cost.expect("値段");
    assert!(two <= money, "2 駅なら足りる: {two:?} {money:?}");
    assert!(!build(&h).cost.finish_short);
    h.move_cursor_to(plan()[2]);
    h.frames(1);
    assert!(build(&h).cost.short.is_some(), "3 駅目まで足すと足りない");
    assert!(!build(&h).cost.finish_short, "仕上げは足りる");
    h.move_cursor_to(GridPos { x: 20, y: 16 });
    assert_eq!(build(&h).cost.cost, Some(two), "駅の外なら決めた駅だけ");
    assert!(!build(&h).cost.finish_short);
    pick(&mut h, 2);
    assert_eq!(selected(&h).len(), 3, "足りなくても足せる");
    assert!(build(&h).cost.finish_short, "3 駅で仕上げると足りない");
}

/// のばすで足りないとき: 確認へ進まず、つなぐ駅を選ぶ段階のまま。
#[test]
fn short_money_on_extend_stays_on_picking() {
    let mut h = priced(200_000);
    start_extend(&mut h);
    pick(&mut h, 2);
    h.move_cursor_to(plan()[3]);
    h.frames(1);
    assert!(build(&h).cost.short.is_some());
    wait(&mut h, 300);
    let mark = h.audio.len();
    h.press(Action::Confirm);
    assert!(build(&h).confirm.is_none());
    assert!(matches!(build(&h).step, Step::PickNew(d) if d.from == StationId(2)));
    assert_eq!(ses(&h, mark), [tm_assets::sound::SE_CANNOT_BUY]);
    assert_eq!(notice(&h), Some(txt::ERR_MONEY));
}

fn cost_screen() -> UiHarness {
    let mut h = cheap_with_company(2);
    start_extend(&mut h);
    pick(&mut h, 0);
    h.move_cursor_to(plan()[4]);
    h.frames(1);
    h
}

fn short_screen() -> UiHarness {
    let mut h = priced(200_000);
    start_create(&mut h);
    pick(&mut h, 3);
    pick(&mut h, 4);
    h.frames(1);
    h
}

fn cost_confirm_screen() -> UiHarness {
    let mut h = cost_screen();
    h.press(Action::Confirm);
    h
}

/// 会社 Lv3 では電車が駅のホームより長く、新しい駅が通過駅になる。
fn pass_confirm_screen() -> UiHarness {
    let mut h = cheap_with_company(3);
    super::buy_all(&mut h);
    h.frames(2);
    start_extend(&mut h);
    pick(&mut h, 0);
    h.move_cursor_to(plan()[4]);
    h.press(Action::Confirm);
    h
}

/// 値段の行と「通過する駅があるよ」の両方: 箱は 3 行で、行は重ならない。
#[test]
fn confirm_shows_cost_and_pass_rows() {
    let h = pass_confirm_screen();
    let c = build(&h).confirm.as_ref().expect("確認");
    assert!(c.pass && !c.cost.is_zero());
    assert_eq!(dialog_h(&h), crate::layout::line_confirm_h(3));
    let row_y = |ch: &str| -> i32 {
        let head = format!("Sprite '{ch}'12 ");
        let l = h
            .draw_text()
            .lines()
            .rev()
            .find(|l| l.starts_with(&head))
            .expect(ch)
            .to_string();
        l[head.len()..]
            .split([',', ' '])
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    };
    assert_eq!(row_y("通") - row_y("お"), 16, "値段の次の行に通過駅");
}

#[test]
fn snapshot_track_cost() {
    insta::assert_snapshot!("build_cost", info_lines(&cost_screen()));
    insta::assert_snapshot!("build_cost_short", info_lines(&short_screen()));
    let h = cost_confirm_screen();
    let text = h.draw_text();
    let at = super::sprite_at(
        "ui_win_tl",
        crate::layout::LINE_CONFIRM_X,
        crate::layout::BOX_TOP_Y,
    );
    let at_bottom = format!("Sprite ui_win_tl {},", crate::layout::LINE_CONFIRM_X);
    let i = text
        .find(&at)
        .or_else(|| text.rfind(&at_bottom))
        .expect("確認の箱");
    insta::assert_snapshot!("line_confirm_cost", &text[i..]);
    let text = pass_confirm_screen().draw_text();
    let i = text.rfind(&at_bottom).expect("確認の箱");
    insta::assert_snapshot!("line_confirm_cost_pass", &text[i..]);
}

#[test]
#[ignore = "目で確かめるための画像を書く。TM_UI_DUMP_DIR がなければ何もしない"]
fn dump_track_cost_screens() {
    let Ok(dir) = std::env::var("TM_UI_DUMP_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let mut free = UiHarness::from_game(cheap_game(), false);
    free.frames(1);
    start_create(&mut free);
    pick(&mut free, 0);
    free.move_cursor_to(plan()[1]);
    free.frames(1);
    let mut refused = short_screen();
    wait(&mut refused, 300);
    refused.press(Action::Confirm);
    for (name, h) in [
        ("build_cost", &cost_screen()),
        ("build_cost_free", &free),
        ("build_cost_short", &short_screen()),
        ("build_cost_refused", &refused),
        ("line_confirm_cost", &cost_confirm_screen()),
    ] {
        super::raster::write_ppm(
            &dir.join(format!("{name}.ppm")),
            &super::raster::rasterize(h.draw_list()),
        );
    }
}
