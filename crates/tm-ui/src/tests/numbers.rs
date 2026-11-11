//! 数の表示: 10 Hz の更新、購入直後の即時更新、`format_ja` の書式(13 §4.1、§10.2)。

use tm_core::{BuyMode, Command, ParamKey, UpgradeScope};
use tm_numeric::{FormatStyle, format_ja};

use super::{cheap_game, harness_with_line};
use crate::testing::UiHarness;

fn hud_refreshes(h: &UiHarness) -> u32 {
    h.ui.play().unwrap().hud.refreshes
}

fn money_text(h: &UiHarness) -> String {
    h.ui.play().unwrap().hud.money_text().to_string()
}

#[test]
fn hud_numbers_refresh_at_10hz() {
    let mut h = harness_with_line();
    let before = hud_refreshes(&h);
    h.frames(60);
    let n = hud_refreshes(&h) - before;
    assert!((9..=10).contains(&n), "1 秒で {n} 回");
}

#[test]
fn money_shown_is_format_ja_of_the_view_at_the_last_refresh() {
    let mut h = harness_with_line();
    let mut lagged = 0;
    for _ in 0..1200 {
        let before = hud_refreshes(&h);
        let shown = money_text(&h);
        let money_before_update = format_ja(
            h.game.as_ref().unwrap().view().money(),
            &FormatStyle::DEFAULT,
        );
        h.frames(1);
        if hud_refreshes(&h) == before {
            assert_eq!(money_text(&h), shown, "更新の間は表示を変えない");
            if money_before_update != shown {
                lagged += 1;
            }
        } else {
            assert_eq!(
                money_text(&h),
                money_before_update,
                "update の時点の値を出す"
            );
        }
    }
    assert!(lagged > 0, "収入が入っても 100ms までは古い表示のまま");
}

#[test]
fn purchase_refreshes_on_the_next_frame_and_flashes() {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.game
        .as_mut()
        .unwrap()
        .enqueue(super::create_line(&[0, 1, 2]));
    h.frames(10);
    let before = hud_refreshes(&h);
    let g = h.game.as_mut().unwrap();
    let cmd = Command::LevelUp {
        param: ParamKey::S2Fare,
        scope: UpgradeScope::AllStations,
        mode: BuyMode::One,
    };
    g.validate(&cmd).expect("買える");
    g.enqueue(cmd);
    h.frames(1);
    let after_purchase = hud_refreshes(&h);
    h.frames(1);
    assert_eq!(
        hud_refreshes(&h),
        after_purchase + 1,
        "Purchased の次のフレームですぐ作り直す"
    );
    assert!(after_purchase <= before + 1);
    let money = format_ja(
        h.game.as_ref().unwrap().view().money(),
        &FormatStyle::DEFAULT,
    );
    assert_eq!(money_text(&h), money);
    let hud = &h.ui.play().unwrap().hud;
    assert_eq!(
        hud.money_color(h.now),
        crate::draw::palette::TEXT,
        "減ったら 150ms 明るくする"
    );
    h.frames(10);
    let hud = &h.ui.play().unwrap().hud;
    assert_eq!(hud.money_color(h.now), crate::draw::palette::MONEY);
}
