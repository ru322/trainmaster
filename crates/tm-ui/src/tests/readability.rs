//! 画面の読みやすさ(最終監査の指摘): 情報の段、モードごとの操作のヒント、説明の箱の位置、帯の位置、ポップアップの重なり。

use tm_assets::txt;
use tm_core::{GridPos, StationId};
use tm_input::Action;
use tm_platform_api::Rect;
use tm_tutorial::{DirectiveKind, MenuAnchor, TutorialDirective, TutorialStageId, UiAnchor};

use super::screens::{play, play_mut, wait};
use super::{cheap_game, create_line, fill_at, harness_with_line, plan};
use crate::anchor::{Mark, Toward};
use crate::anim::overlaps;
use crate::hud::BandChips;
use crate::layout::{
    BAND_ROW2, BOX_BOTTOM, BOX_TOP_Y, EXPLAIN_X, INFO_ROW, INFO_TEXT_W, INFO_TEXT_X, MAP_H, MAP_W,
    MAP_W_PANEL, MAP_X, MAP_Y, SUB_CONFIRM, explain_h, right, tile_center,
};
use crate::scene::settings_menu::Page;
use crate::testing::UiHarness;

/// `Sprite '<字>'12 x,y` の位置(情報の段の中だけ。左から順)。
fn info_glyphs(h: &UiHarness, ch: char) -> Vec<(i32, i32)> {
    h.draw_text()
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix(&format!("Sprite '{ch}'12 "))?;
            let (x, y) = rest.split(' ').next()?.split_once(',')?;
            let (x, y): (i32, i32) = (x.parse().ok()?, y.parse().ok()?);
            (y >= INFO_ROW.y && y < INFO_ROW.y + INFO_ROW.h as i32).then_some((x, y))
        })
        .collect()
}

fn info_glyph(h: &UiHarness, ch: char) -> Option<(i32, i32)> {
    info_glyphs(h, ch).first().copied()
}

/// 情報の段に描いた文字の矩形(段の 1 行目のものだけ)。
fn info_rects(h: &UiHarness) -> Vec<Rect> {
    let text = h.draw_text();
    let lines: Vec<&str> = text
        .lines()
        .skip_while(|l| !l.starts_with(&fill_at(INFO_ROW)))
        .skip(1)
        .take_while(|l| !l.starts_with(&fill_at(BAND_ROW2)))
        .collect();
    lines.iter().map(|l| parse_rect(l)).collect()
}

#[test]
fn station_info_is_one_row_in_order() {
    let mut h = harness_with_line();
    h.move_cursor_to(plan()[0]);
    h.frames(2);
    let (kx, ky) = info_glyph(&h, '数').expect("「お客さんの数」");
    let lvs = info_glyphs(&h, 'v');
    assert!(
        lvs.iter().any(|&(x, _)| x > kx),
        "「数」の後に Lv が同じ行: {kx},{ky} / {lvs:?}"
    );
    let (nx, _) = info_glyph(&h, '長').expect("「ホームの長さ」");
    let (rx, _) = info_glyph(&h, '両').expect("2両");
    assert!(
        nx > kx && rx > nx,
        "左から 運賃 → お客さんの数 → ホームの長さ"
    );
    for r in info_rects(&h) {
        assert!(
            r.x >= INFO_TEXT_X - 1
                && right(r) <= INFO_TEXT_X + i32::from(INFO_TEXT_W) + 1
                && r.y >= INFO_ROW.y
                && r.y + r.h as i32 <= INFO_ROW.y + INFO_ROW.h as i32,
            "情報の段の中: {r:?}"
        );
    }
}

fn hint(h: &UiHarness) -> BandChips {
    play(h).band.chips()
}

#[test]
fn every_menu_and_dialog_shows_its_own_hint() {
    let mut h = harness_with_line();
    assert_eq!(hint(&h), BandChips::Map);
    h.press(Action::Menu);
    assert_eq!(
        hint(&h),
        BandChips::Hint(txt::BUILD_CHOOSE_HINT),
        "せってい"
    );
    let menu = play_mut(&mut h).settings_menu.as_mut().unwrap();
    menu.set_stages(&[TutorialStageId(1)]);
    menu.page = Page::Replay { sel: 0, first: 0 };
    h.frames(1);
    assert_eq!(
        hint(&h),
        BandChips::Hint(txt::BUILD_CHOOSE_HINT),
        "読み直しの一覧"
    );
    h.press(Action::Confirm);
    assert!(matches!(
        play(&h).settings_menu.as_ref().unwrap().page,
        Page::Read { .. }
    ));
    assert_eq!(
        hint(&h),
        BandChips::Hint(txt::BUILD_END_HINT),
        "読む画面は もどる だけ"
    );
    let text = h.draw_text();
    let dim = text.find("Fill 0,0 640x360 #00000080").expect("うすい黒");
    assert!(
        text[dim..].contains(&fill_at(BAND_ROW2)) && !text[dim..].contains(&fill_at(INFO_ROW)),
        "帯の 2 段目だけを うすい黒の上に描き直す(情報の段は下のまま)"
    );
    h.press(Action::Menu);
    h.press(Action::Menu);
    assert_eq!(hint(&h), BandChips::Map);

    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(1);
    h.press(Action::BuildMode);
    h.press(Action::Confirm);
    for &i in &[0, 1, 1] {
        h.move_cursor_to(plan()[i]);
        h.press(Action::Confirm);
    }
    assert!(match &play(&h).mode {
        crate::scene::PlayMode::Build(b) => b.confirm.is_some(),
        _ => false,
    });
    assert_eq!(
        hint(&h),
        BandChips::Hint(txt::BUILD_CHOOSE_HINT),
        "路線の確認"
    );
    let mut d = fake(Some(UiAnchor::Menu(MenuAnchor::BuildMode)));
    d.kind = DirectiveKind::Guide;
    play_mut(&mut h).fake_directive = Some(d);
    h.frames(1);
    assert_eq!(
        play(&h).explain.shown.unwrap().mark,
        None,
        "確認の間は ろせん の入口を指さない(13 §3.13)"
    );
}

fn fake(highlight: Option<UiAnchor>) -> TutorialDirective {
    TutorialDirective {
        stage: TutorialStageId(6),
        kind: DirectiveKind::Explanation,
        message: txt::TUT_S09_P2,
        page: 0,
        page_count: 3,
        highlight,
        pauses_game: true,
    }
}

fn area(h: &UiHarness) -> Rect {
    play(h).explain.shown.expect("説明").area
}

#[test]
fn explanation_box_never_covers_the_open_panel_or_its_mark() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    h.press(Action::TabNext);
    play_mut(&mut h).fake_directive = Some(fake(Some(UiAnchor::Menu(MenuAnchor::BuyMaxHint))));
    h.frames(1);
    let a = area(&h);
    assert!(right(a) <= MAP_W_PANEL, "パネルを隠さない: {a:?}");
    assert_eq!(a.h, explain_h(2), "2 行の説明は 2 行の高さ");
    let block = &play(&h).explain.shown.unwrap();
    assert!(block.boxed);
    for anchor in [
        UiAnchor::Station(StationId(2)),
        UiAnchor::Station(StationId(0)),
    ] {
        h.press(Action::Cancel);
        play_mut(&mut h).fake_directive = Some(fake(Some(anchor)));
        h.frames(30);
        let s = play(&h).explain.shown.unwrap();
        let Some(Mark::Tile(m)) = s.mark else {
            panic!("カメラが寄った後は枠: {:?}", s.mark)
        };
        assert!(
            !overlaps(s.area, m),
            "{anchor:?} を隠さない: {:?} / {m:?}",
            s.area
        );
    }
}

#[test]
fn stage_15_page_2_leaves_the_reset_row_visible() {
    let mut h = UiHarness::with_game(plan(), true);
    h.frames(1);
    h.press(Action::Menu);
    wait(&mut h, 320);
    h.press(Action::Confirm);
    let s = play(&h).explain.shown.unwrap();
    assert_eq!(s.directive.page, 1);
    let Some(Mark::Side(row)) = s.mark else {
        panic!("リセットの行")
    };
    assert!(!overlaps(s.area, row), "{:?} / {row:?}", s.area);
}

#[test]
fn banner_and_toast_stay_over_the_map() {
    use crate::layout::{BANNER, BANNER_H3, GOAL_BAND, TOAST};
    for r in [BANNER, TOAST, GOAL_BAND] {
        assert!(r.x >= MAP_X && right(r) <= MAP_X + MAP_W, "{r:?}");
        assert!(r.y >= MAP_Y && r.y + r.h as i32 <= MAP_Y + MAP_H, "{r:?}");
    }
    assert!(BANNER.y + BANNER_H3 as i32 <= TOAST.y);
}

#[test]
fn income_popups_do_not_overlap_and_hide_the_tags_under_them() {
    let mut p = plan();
    p[1] = GridPos { x: 5, y: 4 };
    p[2] = GridPos { x: 6, y: 4 };
    let mut h = UiHarness::with_game(p, false);
    h.frames(1);
    h.move_cursor_to(GridPos { x: 5, y: 2 });
    wait(&mut h, 500);
    let now = h.now;
    for s in 0..3 {
        h.ui.observe(
            now,
            &tm_core::GameEvent::TrainArrived {
                train: tm_core::TrainId(0),
                line: tm_core::LineId(0),
                station: StationId(s),
                riders: tm_core::People(1),
                alighted: tm_core::People(1),
                boarded: tm_core::People(1),
                income: tm_numeric::BigNum::from_u64(123_450_000),
                dwell: tm_core::Ticks(1),
            },
            &mut Vec::new(),
        );
    }
    h.frames(1);
    let view = h.game.as_ref().unwrap().view();
    let placed: Vec<Rect> = play(&h)
        .anims
        .popups
        .layout(view, h.now, play(&h).camera.pos, crate::layout::MAP_W)
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(placed.len(), 2, "置けない 3 つ目は今は出さない");
    assert!(!overlaps(placed[0], placed[1]));
    let row = i32::from(tm_assets::font::face(tm_assets::font::FontId::Body12).line_height);
    let fills = h
        .draw_text()
        .lines()
        .filter(|l| {
            l.starts_with("Fill ") && l.ends_with("#1E2A4AFF") && l.contains(&format!("x{row} "))
        })
        .count();
    assert_eq!(fills, 2, "ポップアップの地は不透明");

    let mut b = super::budget::busiest();
    b.frames(1);
    let view = b.game.as_ref().unwrap().view();
    let cam = play(&b).camera.pos;
    let pops: Vec<Rect> = play(&b)
        .anims
        .popups
        .layout(view, b.now, cam, crate::layout::MAP_W)
        .into_iter()
        .flatten()
        .collect();
    assert!(pops.len() >= 2);
    for (i, a) in pops.iter().enumerate() {
        for c in &pops[i + 1..] {
            assert!(!overlaps(*a, *c), "{a:?} / {c:?}");
        }
    }
    for l in b.draw_text().lines().filter(|l| l.ends_with("#1E2A4AD0")) {
        let r = parse_rect(l);
        if r.h as i32 == row {
            assert!(
                pops.iter().all(|p| !overlaps(r, *p)),
                "人数の札がポップアップの下に残る: {l}"
            );
        }
    }
}

/// `Fill x,y wxh` と `Sprite <名前> x,y wxh` の矩形。
fn parse_rect(l: &str) -> Rect {
    let skip = if l.starts_with("Sprite ") { 2 } else { 1 };
    let mut it = l.split(' ').skip(skip);
    let (x, y) = it.next().unwrap().split_once(',').unwrap();
    let (w, hh) = it.next().unwrap().split_once('x').unwrap();
    Rect {
        x: x.parse().unwrap(),
        y: y.parse().unwrap(),
        w: w.parse().unwrap(),
        h: hh.parse().unwrap(),
    }
}

#[test]
fn stage_15_box_is_fixed_below_the_settings_for_every_row() {
    use crate::scene::settings_menu::ITEMS;
    let mut h = harness_with_line();
    h.press(Action::Menu);
    for item in ITEMS {
        let mut d = fake(Some(UiAnchor::Menu(MenuAnchor::SettingsItem(item))));
        d.stage = TutorialStageId(15);
        d.pauses_game = false;
        play_mut(&mut h).fake_directive = Some(d);
        h.frames(1);
        let s = play(&h).explain.shown.unwrap();
        let Some(Mark::Side(row)) = s.mark else {
            panic!("せってい の行")
        };
        assert_eq!(
            (s.area.x, s.area.y + s.area.h as i32),
            (EXPLAIN_X, BOX_BOTTOM),
            "{item:?}: highlight の反対側の規則を使わない"
        );
        assert!(!overlaps(s.area, mark_rect(Mark::Side(row))), "{item:?}");
    }
}

#[test]
fn when_both_places_are_free_the_box_goes_opposite_the_mark() {
    let mut h = harness_with_line();
    play_mut(&mut h).fake_directive = Some(fake(Some(UiAnchor::MoneyDisplay)));
    h.frames(1);
    let a = area(&h);
    assert_eq!(
        a.y + a.h as i32,
        BOX_BOTTOM,
        "上の HUD を指すときは下に出す"
    );

    let mut p = plan();
    p[2] = GridPos { x: 28, y: 10 };
    let mut h = UiHarness::with_game(p.clone(), false);
    h.frames(1);
    h.press(Action::TabNext);
    play_mut(&mut h).fake_directive = Some(fake(Some(UiAnchor::Station(StationId(2)))));
    h.frames(1);
    let s = play(&h).explain.shown.unwrap();
    let (_, cy) = tile_center(p[2], play(&h).camera.pos);
    assert!(2 * cy >= crate::layout::SCREEN_H, "前提: 画面の下半分");
    assert!(
        matches!(s.mark, Some(Mark::Edge { y, toward: Toward::Right, .. }) if y == cy),
        "{:?}",
        s.mark
    );
    assert_eq!(s.area.y, BOX_TOP_Y, "下半分の端の矢印なら上に出す");
}

#[test]
fn settings_do_not_bring_the_guide_text_above_the_dim() {
    let mut h = harness_with_line();
    let mut d = fake(Some(UiAnchor::MoneyDisplay));
    d.kind = DirectiveKind::Guide;
    d.message = txt::TUT_S06_G1;
    d.pauses_game = false;
    play_mut(&mut h).fake_directive = Some(d);
    h.frames(1);
    assert!(h.draw_text().contains("Sprite ico_guide"));
    h.press(Action::Menu);
    let text = h.draw_text();
    let dim = text.find("Fill 0,0 640x360 #00000080").expect("うすい黒");
    assert!(
        text[..dim].contains("Sprite ico_guide"),
        "案内は下の画面のまま"
    );
    assert!(
        !text[dim..].contains("ico_guide") && !text[dim..].contains("#FFD400FF"),
        "案内の文と印を うすい黒の上に出さない"
    );
    assert!(
        text[dim..].contains("Sprite btn_key_z"),
        "操作のヒントは出す"
    );
}

#[test]
#[ignore = "目で確かめるための画像を書く。TM_UI_DUMP_DIR がなければ何もしない"]
fn dump_settings_over_tutorial() {
    use super::raster::{rasterize, write_ppm};
    let Ok(dir) = std::env::var("TM_UI_DUMP_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let mut shots: Vec<(&str, UiHarness)> = Vec::new();
    let mut h = UiHarness::with_game(plan(), true);
    h.frames(1);
    h.press(Action::Menu);
    h.frames(10);
    shots.push(("settings_over_s15", h));
    let mut h = UiHarness::with_game(plan(), true);
    h.frames(1);
    h.press(Action::Menu);
    wait(&mut h, 320);
    h.press(Action::Confirm);
    h.frames(10);
    shots.push(("settings_over_s15_p2", h));
    let mut h = UiHarness::with_game(plan(), true);
    h.frames(1);
    h.press(Action::Menu);
    for _ in 0..6 {
        h.press(Action::MoveDown);
    }
    h.frames(20);
    h.press(Action::Confirm);
    h.frames(10);
    shots.push(("settings_over_s15_reset", h));
    let mut h = UiHarness::with_game(plan(), true);
    h.frames(1);
    h.press(Action::Menu);
    for _ in 0..tm_tutorial::stage_pages(TutorialStageId(15)).len() {
        wait(&mut h, 320);
        h.press(Action::Confirm);
    }
    h.frames(10);
    shots.push(("settings_over_main_explanation", h));
    let mut h = harness_with_line();
    let mut d = fake(Some(UiAnchor::Menu(MenuAnchor::BuildMode)));
    d.kind = DirectiveKind::Guide;
    d.message = txt::TUT_S03_G1;
    d.pauses_game = false;
    play_mut(&mut h).fake_directive = Some(d);
    h.frames(2);
    h.press(Action::Menu);
    h.frames(10);
    shots.push(("settings_over_guide", h));
    for (name, h) in &shots {
        write_ppm(&dir.join(format!("{name}.ppm")), &rasterize(h.draw_list()));
    }
}

/// 印が占める範囲(枠と、横の矢印の余地)。
fn mark_rect(m: Mark) -> Rect {
    match m {
        Mark::Frame(r) | Mark::Tile(r) | Mark::Line { mid: r, .. } => r,
        Mark::Side(r) => Rect {
            x: r.x - 22,
            w: r.w + 44,
            ..r
        },
        Mark::Edge {
            y,
            toward: Toward::Left | Toward::Right,
            ..
        } => Rect {
            x: 0,
            y: y - 8,
            w: 640,
            h: 16,
        },
        Mark::Edge { x, y, .. } => Rect {
            x: x - 16,
            y: y - 20,
            w: 32,
            h: 40,
        },
    }
}

#[test]
fn settings_stay_on_top_of_every_tutorial_message() {
    let mut h = UiHarness::with_game(plan(), true);
    h.frames(1);
    h.press(Action::Menu);
    let pages = tm_tutorial::stage_pages(TutorialStageId(15)).len() as u8;
    for page in 0..pages {
        let s = play(&h).explain.shown.unwrap();
        assert_eq!((s.directive.stage.0, s.directive.page), (15, page));
        let bounds = play(&h).settings_menu.as_ref().unwrap().bounds();
        let a = s.area;
        assert!(s.under_menu, "#15 は せってい の下に固定した箱");
        assert!(!overlaps(a, bounds), "せってい に重ねない: {a:?}");
        assert!(
            !overlaps(a, mark_rect(s.mark.expect("行"))),
            "指している行と矢印にも重ねない"
        );
        assert!(a.x >= 0 && right(a) <= 640, "画面の中");
        assert!(a.y >= crate::layout::HUD.h as i32 && a.y + a.h as i32 <= crate::layout::BAND.y);
        if page == 0 {
            let text = h.draw_text();
            let dim = text.find("Fill 0,0 640x360 #00000080").expect("うすい黒");
            insta::assert_snapshot!("settings_with_stage_15", &text[dim..]);
        }
        wait(&mut h, 320);
        h.press(Action::Confirm);
    }

    assert_eq!(
        play(&h).explain.shown.unwrap().directive.stage.0,
        1,
        "#15 を読み終えると本線(#1)の説明"
    );
    let text = h.draw_text();
    assert!(
        !text.contains("ui_popup"),
        "本線の説明の箱は せってい の下で描かない"
    );
    let dim = text.find("Fill 0,0 640x360 #00000080").expect("うすい黒");
    assert!(
        !text[dim..].contains("fx_arrow") && !text[dim..].contains("#FFD400"),
        "highlight も出さない"
    );
    h.press(Action::Menu);
    assert!(h.draw_text().contains("ui_popup"), "閉じると説明に戻る");
}

#[derive(Clone, Copy, Debug)]
enum Under {
    Map,
    Panel,
    Draft,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shown15 {
    None,
    Stage15,
    PauseLow,
    PauseHigh,
    GuideMark,
    GuidePlain,
}

fn directive_for(kind: Shown15) -> Option<TutorialDirective> {
    use tm_tutorial::SettingsItem;
    let mut d = fake(None);
    match kind {
        Shown15::None => return None,
        Shown15::Stage15 => {
            d.stage = TutorialStageId(15);
            d.message = txt::TUT_S15_P1;
            d.pauses_game = false;
            d.highlight = Some(UiAnchor::Menu(MenuAnchor::SettingsItem(
                SettingsItem::SoundVolume,
            )));
        }
        Shown15::PauseLow => d.highlight = Some(UiAnchor::MoneyDisplay),
        Shown15::PauseHigh => d.highlight = Some(UiAnchor::Station(StationId(2))),
        Shown15::GuideMark | Shown15::GuidePlain => {
            d.kind = DirectiveKind::Guide;
            d.message = txt::TUT_S03_G1;
            d.pauses_game = false;
            d.highlight = (kind == Shown15::GuideMark).then_some(UiAnchor::Station(StationId(0)));
        }
    }
    Some(d)
}

fn under(mode: Under, kind: Shown15) -> UiHarness {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.frames(2);
    match mode {
        Under::Map => {}
        Under::Panel => h.press(Action::TabNext),
        Under::Draft => {
            h.press(Action::BuildMode);
            h.press(Action::Confirm);
            h.move_cursor_to(plan()[0]);
            h.press(Action::Confirm);
        }
    }
    play_mut(&mut h).fake_directive = directive_for(kind);
    h.frames(1);
    h
}

fn set_page(h: &mut UiHarness, page: Page) {
    let menu = play_mut(h).settings_menu.as_mut().expect("せってい");
    menu.set_stages(&[TutorialStageId(1), TutorialStageId(3)]);
    menu.page = page;
    h.frames(1);
}

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && right(inner) <= right(outer)
        && inner.y + inner.h as i32 <= outer.y + outer.h as i32
}

/// 13 §10.2「せってい が最前面」: すべての組み合わせで、うすい黒より上に描いたもののうち せってい の箱と重なるのは
/// せってい 自身(箱・行・#15 の行の枠)だけ。うすい黒と せってい の間に描くのは帯の 2 段目だけ。
#[test]
fn settings_box_is_overlapped_only_by_the_dim_and_the_band() {
    use crate::input_ctx::Guard;
    use crate::layout::SETTINGS;
    let pages = |now| {
        [
            Page::Top,
            Page::Replay { sel: 1, first: 0 },
            Page::Read {
                sel: 0,
                first: 0,
                stage: TutorialStageId(1),
                page: 0,
            },
            Page::Reset {
                step: 1,
                yes: false,
                guard: Guard::from(now),
            },
            Page::Reset {
                step: 2,
                yes: false,
                guard: Guard::from(now),
            },
            Page::Quit {
                step: 1,
                yes: false,
                guard: Guard::from(now),
            },
            Page::Quit {
                step: 2,
                yes: false,
                guard: Guard::from(now),
            },
        ]
    };
    let kinds = [
        Shown15::None,
        Shown15::Stage15,
        Shown15::PauseLow,
        Shown15::PauseHigh,
        Shown15::GuideMark,
        Shown15::GuidePlain,
    ];
    let mut checked = 0;
    for mode in [Under::Map, Under::Panel, Under::Draft] {
        for kind in kinds {
            for page_no in 0..7 {
                let mut h = under(mode, kind);
                let before = play(&h).explain.shown.map(|s| s.area);
                if kind == Shown15::PauseHigh {
                    assert_eq!(
                        before.map(|a| a.y),
                        Some(BOX_TOP_Y),
                        "{mode:?}: 上に出す説明"
                    );
                }
                h.press(Action::Menu);
                let page = pages(h.now)[page_no];
                set_page(&mut h, page);
                let case = format!("{mode:?} / {kind:?} / {page:?}");
                let text = h.draw_text();
                let lines: Vec<&str> = text.lines().collect();
                let dims: Vec<usize> = (0..lines.len())
                    .filter(|&i| lines[i].ends_with(" #00000080"))
                    .collect();
                assert_eq!(dims.len(), 1, "{case}: うすい黒は 1 まい");
                let dim = dims[0];
                assert_eq!(lines[dim], "Fill 0,0 640x360 #00000080", "{case}");
                let menu = lines
                    .iter()
                    .position(|l| {
                        l.starts_with(&format!("Sprite ui_win_tl {},{} ", SETTINGS.x, SETTINGS.y))
                    })
                    .unwrap_or_else(|| panic!("{case}: せってい の箱"));
                assert!(menu > dim, "{case}");
                for l in &lines[dim + 1..menu] {
                    assert!(
                        inside(parse_rect(l), BAND_ROW2),
                        "{case}: 帯の 2 段目だけ: {l}"
                    );
                }
                for l in &lines[menu..] {
                    let r = parse_rect(l);
                    assert!(
                        !overlaps(r, SETTINGS) || inside(r, SETTINGS),
                        "{case}: せってい の箱に重ねない: {l}"
                    );
                    for banned in ["fx_arrow", "ico_guide", "fx_dot", "#FFD400"] {
                        let side15 = kind == Shown15::Stage15 && page_no == 0;
                        if banned != "#FFD400" || !side15 {
                            assert!(!l.contains(banned), "{case}: {banned} を描かない: {l}");
                        }
                    }
                }
                let popups: Vec<Rect> = lines[menu..]
                    .iter()
                    .filter(|l| l.starts_with("Sprite ui_popup_tl "))
                    .map(|l| parse_rect(l))
                    .collect();
                let read = matches!(page, Page::Read { .. });
                let s15 = kind == Shown15::Stage15 && page == Page::Top;
                assert_eq!(
                    popups.len(),
                    usize::from(read || s15),
                    "{case}: 説明の箱は #15 と読み直しだけ"
                );
                for p in &popups {
                    assert_eq!(p.x, EXPLAIN_X, "{case}");
                    assert!(
                        [1, 2]
                            .iter()
                            .any(|&n| p.y == BOX_BOTTOM - explain_h(n) as i32),
                        "{case}: せってい の下に固定: {p:?}"
                    );
                }
                let confirm = lines[menu..].iter().any(|l| {
                    l.starts_with(&format!(
                        "Sprite ui_win_tl {},{} ",
                        SUB_CONFIRM.x, SUB_CONFIRM.y
                    ))
                });
                assert_eq!(
                    confirm,
                    matches!(page, Page::Reset { .. } | Page::Quit { .. }),
                    "{case}"
                );
                assert!(
                    !confirm || popups.is_empty(),
                    "{case}: #15 と確認は同時に出ない"
                );

                if page != Page::Top {
                    h.press(Action::Menu);
                }
                h.press(Action::Menu);
                assert!(!play(&h).settings_open(), "{case}");
                if matches!(kind, Shown15::PauseLow | Shown15::PauseHigh) {
                    assert_eq!(
                        play(&h).explain.shown.map(|s| s.area),
                        before,
                        "{case}: 閉じると元の位置"
                    );
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 3 * 6 * 7);
}

fn arrive(h: &mut UiHarness, station: u32) {
    let now = h.now;
    h.ui.observe(
        now,
        &tm_core::GameEvent::TrainArrived {
            train: tm_core::TrainId(0),
            line: tm_core::LineId(0),
            station: StationId(station),
            riders: tm_core::People(1),
            alighted: tm_core::People(1),
            boarded: tm_core::People(1),
            income: tm_numeric::BigNum::from_u64(123_456_789),
            dwell: tm_core::Ticks(1),
        },
        &mut Vec::new(),
    );
}

/// 収入のポップアップは表示域(パネルを開いていれば 416)の左右に収める [13 §3.12]。
/// カーソルを右へ動かし、カメラの右端のマスの駅で確かめる。
#[test]
fn income_popups_stay_inside_the_view_with_or_without_the_panel() {
    use crate::layout::TILE;
    let mut p = plan();
    p[1] = GridPos { x: 24, y: 8 };
    p[2] = GridPos { x: 30, y: 6 };
    let mut h = UiHarness::with_game(p.clone(), false);
    h.frames(1);
    h.move_cursor_to(GridPos { x: 28, y: 6 });
    wait(&mut h, 500);
    let (x, _) = tile_center(p[2], play(&h).camera.pos);
    assert_eq!(x, MAP_X + MAP_W - TILE / 2, "前提: 駅は表示域の右端のマス");
    let popup_fills = |h: &UiHarness| -> Vec<Rect> {
        h.draw_text()
            .lines()
            .filter(|l| l.starts_with("Fill ") && l.ends_with("#1E2A4AFF") && l.contains("x16 "))
            .map(parse_rect)
            .collect()
    };
    for s in [1, 2] {
        arrive(&mut h, s);
    }
    h.frames(1);
    let fills = popup_fills(&h);
    assert_eq!(fills.len(), 2);
    for r in &fills {
        assert!(r.x >= MAP_X && right(*r) <= MAP_X + MAP_W, "{r:?}");
    }
    assert!(
        fills.iter().any(|r| right(*r) == MAP_X + MAP_W),
        "右端の駅は端に寄せる"
    );

    h.press(Action::TabNext);
    wait(&mut h, 900);
    for s in [1, 2] {
        arrive(&mut h, s);
    }
    h.frames(1);
    let fills = popup_fills(&h);
    assert!(!fills.is_empty());
    for r in &fills {
        assert!(
            r.x >= MAP_X && right(*r) <= MAP_X + MAP_W_PANEL,
            "パネルの下に入れない: {r:?}"
        );
    }
    assert!(fills.iter().any(|r| right(*r) == MAP_X + MAP_W_PANEL));
}
