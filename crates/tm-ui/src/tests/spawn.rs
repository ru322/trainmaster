//! 駅の出現の演出(13 §3.11、§3.12、§5.3。IMP-405): ジングルの後に 4px → 14px、その間は静的な層に入れない。

use tm_assets::sound::SE_STATION_NEW;
use tm_core::{BuyMode, Command, GridPos, ParamKey, UpgradeScope};
use tm_platform_api::{AudioCmd, DrawCmd, Rect};

use super::screens::play;
use super::{cheap_game, create_line, plan};
use crate::testing::UiHarness;

fn buy_round(h: &mut UiHarness) {
    let g = h.game.as_mut().unwrap();
    for p in ParamKey::ALL {
        let scope = match p {
            ParamKey::R2LineSlots => UpgradeScope::Company,
            _ if p.category() == tm_core::ParamCategory::Station => UpgradeScope::AllStations,
            _ => UpgradeScope::AllLines,
        };
        let cmd = Command::LevelUp {
            param: p,
            scope,
            mode: BuyMode::One,
        };
        if g.validate(&cmd).is_ok() {
            g.enqueue(cmd);
        }
    }
}

fn stations(h: &UiHarness) -> usize {
    h.game.as_ref().unwrap().view().stations().len()
}

/// 駅の絵(層の中と外の両方)のうち、マス `pos` の中心にあるもの。
fn station_sprites(h: &UiHarness, pos: GridPos) -> (Vec<Rect>, Vec<Rect>) {
    use crate::layout::{LAYER_W, MAP_X, MAP_Y, TILE};
    let (mut layer, mut fx) = (Vec::new(), Vec::new());
    let mut inside = None;
    let (lx, ly) = crate::layout::tile_layer(pos);
    let (cx, cy) = (lx + TILE / 2, ly + TILE / 2);
    let cam = play(h).camera.pos;
    for c in &h.draw_list().cmds {
        match *c {
            DrawCmd::BeginCachedLayer { layer: l, .. } => inside = Some(i32::from(l.0) * LAYER_W),
            DrawCmd::EndCachedLayer => inside = None,
            DrawCmd::Sprite { id, dst, .. }
                if id == tm_assets::sprite::STN_BODY || id == tm_assets::sprite::STN_BODY_OFF =>
            {
                let (mx, my) = (dst.x + dst.w as i32 / 2, dst.y + dst.h as i32 / 2);
                match inside {
                    Some(x0) if (mx + x0, my) == (cx, cy) => layer.push(dst),
                    None if (mx - MAP_X + cam.x, my - MAP_Y + cam.y) == (cx, cy) => fx.push(dst),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    (layer, fx)
}

struct Run {
    se_frame: usize,
    frames: Vec<(u64, Vec<Rect>, Vec<Rect>)>,
    builds_before: u32,
    builds_after: u32,
}

/// 安い表で 1 つだけ会社レベルを上げ、駅が出てから 3 秒の様子を記録する。カメラは出る駅のほうへ先に向けておく。
fn level_up_once() -> Run {
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    let n0 = stations(&h);
    h.move_cursor_to(plan()[n0]);
    h.frames(30);
    for _ in 0..200 {
        if stations(&h) > n0 {
            break;
        }
        buy_round(&mut h);
        h.frames(1);
    }
    assert_eq!(stations(&h), n0 + 1, "会社レベルが上がって駅が 1 つ出る");
    let pos = plan()[n0];
    let builds_before = play(&h).map_builds;
    let mut frames = Vec::new();
    let mut se_frame = None;
    for i in 0..400 {
        let a = h.audio.len();
        h.frames(1);
        if h.audio[a..].contains(&AudioCmd::PlaySe(SE_STATION_NEW)) {
            se_frame = Some(i);
        }
        let (layer, fx) = station_sprites(&h, pos);
        frames.push((h.now.0, layer, fx));
    }
    Run {
        se_frame: se_frame.expect("se_station_new が鳴る"),
        frames,
        builds_before,
        builds_after: play(&h).map_builds,
    }
}

#[test]
fn new_station_grows_from_the_station_sound() {
    let r = level_up_once();
    assert!(r.se_frame > 30, "ジングルの後に鳴る");
    for (i, (_, layer, fx)) in r.frames.iter().enumerate().take(r.se_frame) {
        assert!(
            layer.is_empty() && fx.is_empty(),
            "フレーム {i}: ジングルの間は出さない"
        );
    }
    let (_, layer, fx) = &r.frames[r.se_frame];
    assert!(layer.is_empty(), "演出の間は層に入れない");
    assert_eq!(fx.len(), 1, "音と同じフレームから描く");
    assert_eq!((fx[0].w, fx[0].h), (8, 8), "絵の 4px(2 倍で 8px)から");
    let grow: Vec<u32> = r.frames[r.se_frame..]
        .iter()
        .take_while(|f| !f.2.is_empty())
        .map(|f| f.2[0].w)
        .collect();
    assert!(
        grow.windows(2).all(|w| w[0] <= w[1]),
        "広がるだけ: {grow:?}"
    );
    assert!(
        *grow.last().unwrap() >= 26,
        "絵の 14px(2 倍で 28px)近くまで: {grow:?}"
    );
    assert_eq!(grow.len(), 24, "400ms(24 フレーム)");
    let end = r.se_frame + grow.len();
    let (_, layer, fx) = &r.frames[end];
    assert!(fx.is_empty());
    assert_eq!(layer.len(), 1, "終わったら層に入れる");
    assert_eq!((layer[0].w, layer[0].h), (28, 28));
}

#[test]
fn layer_is_rebuilt_only_when_the_station_appears_and_when_it_settles() {
    let r = level_up_once();
    assert_eq!(
        r.builds_after - r.builds_before,
        2,
        "出現(map_revision)と演出の終わりの 2 回だけ(13 §9)"
    );
}

#[test]
#[ignore = "目で確かめるための画像を書く。TM_UI_DUMP_DIR がなければ何もしない"]
fn dump_spawn_screens() {
    use super::raster::{rasterize, write_ppm};
    let Ok(dir) = std::env::var("TM_UI_DUMP_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let mut h = UiHarness::from_game(cheap_game(), false);
    h.game.as_mut().unwrap().enqueue(create_line(&[0, 1, 2]));
    h.frames(2);
    let n0 = stations(&h);
    while stations(&h) == n0 {
        buy_round(&mut h);
        h.frames(1);
    }
    let mut shot = 0;
    for _ in 0..400 {
        let a = h.audio.len();
        h.frames(1);
        if shot == 0 && h.audio[a..].contains(&AudioCmd::PlaySe(SE_STATION_NEW)) {
            shot = 1;
        }
        if shot > 0 {
            if [1, 8, 16, 30].contains(&shot) {
                write_ppm(
                    &dir.join(format!("spawn_{shot:02}.ppm")),
                    &rasterize(h.draw_list()),
                );
            }
            shot += 1;
        }
    }
}
