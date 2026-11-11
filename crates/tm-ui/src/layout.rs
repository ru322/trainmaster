//! 画面の領域の定数 [13 §3.5、§3.5.1]。論理解像度 640 × 360。

use tm_core::GridPos;
use tm_platform_api::Rect;

/// 画面の幅。
pub(crate) const SCREEN_W: i32 = 640;
/// 画面の高さ。
pub(crate) const SCREEN_H: i32 = 360;
/// マップの絵(16px のマス用に描いたスプライト)を拡大する整数倍。最近傍で描くのでドットはにじまない。
pub(crate) const MAP_SCALE: i32 = 2;
/// マス 1 つの大きさ(px)。スプライトのマス 16px の `MAP_SCALE` 倍。
pub(crate) const TILE: i32 = 16 * MAP_SCALE;
/// マップの横のマス数の上限(バランスのデータと同じ)。
pub(crate) const MAP_COLS: i32 = 40;
/// マップの縦のマス数の上限。
pub(crate) const MAP_ROWS: i32 = 18;
/// マップの表示域の左端。
pub(crate) const MAP_X: i32 = 0;
/// マップの表示域の上端。
pub(crate) const MAP_Y: i32 = HUD.h as i32;
/// マップの表示域の幅(マップ全体はこれより広く、カメラで動かして見る)。
pub(crate) const MAP_W: i32 = SCREEN_W - MAP_X;
/// マップの表示域の高さ。
pub(crate) const MAP_H: i32 = BAND.y - MAP_Y;
/// 強化パネルを開いたときのマップの表示域の幅。
pub(crate) const MAP_W_PANEL: i32 = PANEL.x - MAP_X;
/// マップ全体(40 × 18 マス)の幅(層の座標)。
pub(crate) const WORLD_W: i32 = MAP_COLS * TILE;
/// マップ全体の高さ。
pub(crate) const WORLD_H: i32 = MAP_ROWS * TILE;
/// 静的な層を縦に切った 1 枚の幅(1 枚は 1024px 以下。14 §3.5.4)。
pub(crate) const LAYER_W: i32 = 20 * TILE;
/// 静的な層の枚数。
pub(crate) const LAYER_COUNT: usize = ((WORLD_W + LAYER_W - 1) / LAYER_W) as usize;

// 層は 1 枚 1024px 以下、4 枚まで、合計 4MiB 以下(14 §3.5.4)
const _: () = assert!(LAYER_W <= 1024 && WORLD_H <= 1024 && LAYER_COUNT <= 4);
const _: () = assert!(LAYER_COUNT * (LAYER_W * WORLD_H * 4) as usize <= 4 * 1024 * 1024);

/// HUD 全体(2 段)。
pub(crate) const HUD: Rect = rect(0, 0, 640, 32);
/// HUD 1 段目。
pub(crate) const HUD_ROW1: Rect = rect(0, 0, 640, 16);
/// HUD 2 段目。
pub(crate) const HUD_ROW2: Rect = rect(0, 16, 640, 16);
/// HUD の左右の余白。
pub(crate) const HUD_PAD: i32 = 4;
/// コインの絵の左上。
pub(crate) const COIN_AT: (i32, i32) = (3, 2);
/// 所持金の文の左端。
pub(crate) const MONEY_X: i32 = 20;
/// 会社ゲージ(10 区切り)。
pub(crate) const GAUGE: Rect = rect(556, 4, 80, 8);
/// 「会社レベル N」の右端。
pub(crate) const LEVEL_RIGHT: i32 = 548;
/// 保存中の印の枠 [UI-D-34]。
pub(crate) const SAVE_MARK: Rect = rect(624, 18, 12, 12);
/// 目標の文の右端(保存中の印の出入りで動かない)。
pub(crate) const GOAL_RIGHT: i32 = SAVE_MARK.x - HUD_PAD;

/// 強化パネル。
pub(crate) const PANEL: Rect = rect(416, 32, 224, 288);
/// 下の帯(2 段)。
pub(crate) const BAND: Rect = rect(0, 320, 640, 40);
/// 下の帯の 1 段目(情報の段)。
pub(crate) const INFO_ROW: Rect = rect(0, 320, 640, 16);
/// 情報の段の文の左端。
pub(crate) const INFO_TEXT_X: i32 = 4;
/// 情報の段の文の幅(x 4〜636)。
pub(crate) const INFO_TEXT_W: u16 = 632;
/// 情報の段の項目の間。
pub(crate) const INFO_GAP: i32 = 12;
/// 下の帯の 2 段目。
pub(crate) const BAND_ROW2: Rect = rect(0, 336, 640, 24);
/// 下の帯の 2 段目の左(案内・エラー・モードの説明)。
pub(crate) const BAND_LEFT: Rect = rect(6, 336, 412, 24);
/// 下の帯の左の文の開始位置(印 6 + 間 6 の後)。
pub(crate) const BAND_TEXT_X: i32 = 18;
/// 下の帯の左の文の幅。
pub(crate) const BAND_TEXT_W: u16 = 400;
/// 下の帯の 2 段目の右(ボタンのチップ・操作のヒント)。
pub(crate) const BAND_RIGHT: Rect = rect(424, 336, 212, 24);

/// 説明の箱の左端。
pub(crate) const EXPLAIN_X: i32 = 120;
/// 強化パネルを開いているときの説明の箱の左端(表示域の中)。
pub(crate) const EXPLAIN_X_PANEL: i32 = 8;
/// 説明の箱の幅。
pub(crate) const EXPLAIN_W: u32 = 400;
/// 説明の箱の左右の余白(内側の幅 376)。
pub(crate) const EXPLAIN_PAD: i32 = 12;
/// 説明の本文の行数の上限(文言の幅のテストで確かめる)。
#[cfg(test)]
pub(crate) const EXPLAIN_LINES: u8 = 2;
/// 下に出す箱の下端(下の帯の 4px 上)。
pub(crate) const BOX_BOTTOM: i32 = BAND.y - 4;
/// 上に出す箱の上端。
pub(crate) const BOX_TOP_Y: i32 = 36;
/// せってい。
pub(crate) const SETTINGS: Rect = rect(192, 36, 256, 188);
/// せってい の行の高さ。
pub(crate) const SETTINGS_ROW_H: i32 = 20;
/// せってい の行の数(と、読み直しの一覧の見える行の数)。
pub(crate) const SETTINGS_ROWS: i32 = 7;
/// せってい の下位の確認(リセット確認)。せってい の下に出す。
pub(crate) const SUB_CONFIRM: Rect = rect(168, 236, 304, 80);
/// 路線の確認の左端。
pub(crate) const LINE_CONFIRM_X: i32 = 168;
/// 路線の確認の幅。
pub(crate) const LINE_CONFIRM_W: u32 = 304;
/// ろせんモードの選択肢の箱(下端 316)。
pub(crate) const BUILD_CHOICES: Rect = rect(8, 268, 136, 48);
/// 会社レベルの帯(2 行のとき)。マップの中央。
pub(crate) const BANNER: Rect = rect(160, 40, 320, 52);
/// 会社レベルの帯の高さ(機能が解放されたレベルは 3 行)。
pub(crate) const BANNER_H3: u32 = 68;
/// トースト。
pub(crate) const TOAST: Rect = rect(160, 116, 320, 24);
/// ゴールの帯(Head16 を 2 倍で描く)。
pub(crate) const GOAL_BAND: Rect = rect(192, 148, 256, 56);
/// タイトルのメニューの行(「続きから」「初めから」。セーブがなければ 1 行目に「初めから」だけ)。
pub(crate) const TITLE_ROWS: [Rect; 2] = [rect(256, 236, 128, 24), rect(256, 268, 128, 24)];
/// 終了中の「保存しているよ」の箱。
pub(crate) const SAVING_BOX: Rect = rect(232, 164, 176, 32);
/// 終了中の電源の箱(実機だけ)。
pub(crate) const POWER_BOX: Rect = rect(160, 150, 320, 60);

/// 本文の行数 `lines` の説明の箱の高さ(36 + 20 × 行数)。
pub(crate) const fn explain_h(lines: u8) -> u32 {
    36 + 20 * lines as u32
}

/// 文の行数 `lines` の路線の確認の高さ(32 + 16 × 行数)。
pub(crate) const fn line_confirm_h(lines: u8) -> u32 {
    32 + 16 * lines as u32
}

const fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

/// 右端の座標。
pub(crate) const fn right(r: Rect) -> i32 {
    r.x + r.w as i32
}

/// 下端の座標。
pub(crate) const fn bottom(r: Rect) -> i32 {
    r.y + r.h as i32
}

/// カメラの位置。表示域の左上に映る、マップ全体の中の点(層の座標)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cam {
    /// 横。
    pub x: i32,
    /// 縦。
    pub y: i32,
}

impl Cam {
    /// `(x, y)` のカメラ。
    pub(crate) const fn at(x: i32, y: i32) -> Cam {
        Cam { x, y }
    }
}

/// マスの左上の画面座標。`px = x·32 − cam.x`、`py = 32 + y·32 − cam.y`。
pub(crate) const fn tile_screen(pos: GridPos, cam: Cam) -> (i32, i32) {
    (MAP_X + pos.x * TILE - cam.x, MAP_Y + pos.y * TILE - cam.y)
}

/// マスの中心の画面座標。
pub(crate) const fn tile_center(pos: GridPos, cam: Cam) -> (i32, i32) {
    let (x, y) = tile_screen(pos, cam);
    (x + TILE / 2, y + TILE / 2)
}

/// 駅を囲む枠の、マスからのはみ出し。
pub(crate) const RING: i32 = 3 * MAP_SCALE;

/// マスを `RING` だけ広げた矩形(駅を囲む枠と highlight)。
pub(crate) const fn tile_ring(pos: GridPos, cam: Cam) -> Rect {
    let (x, y) = tile_screen(pos, cam);
    rect(
        x - RING,
        y - RING,
        (TILE + 2 * RING) as u32,
        (TILE + 2 * RING) as u32,
    )
}

/// マスが幅 `view_w` の表示域にすべて入っているか。
pub(crate) const fn tile_in_view(pos: GridPos, cam: Cam, view_w: i32) -> bool {
    let (x, y) = tile_screen(pos, cam);
    x >= MAP_X && x + TILE <= MAP_X + view_w && y >= MAP_Y && y + TILE <= MAP_Y + MAP_H
}

/// 画面の点が、幅 `view_w` の表示域のまわり 1 マスの内側にあるか(外のものは描かない)。
pub(crate) const fn near_view(x: i32, y: i32, view_w: i32) -> bool {
    x >= MAP_X - TILE && x < MAP_X + view_w + TILE && y >= MAP_Y - TILE && y < MAP_Y + MAP_H + TILE
}

/// マスの左上の層の座標(静的な層の中身に使う)。
pub(crate) const fn tile_layer(pos: GridPos) -> (i32, i32) {
    (pos.x * TILE, pos.y * TILE)
}

#[cfg(test)]
mod tests {
    use tm_assets::font::{FontId, face};

    use super::*;
    use crate::anim::overlaps;

    fn inside(inner: Rect, outer: Rect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && right(inner) <= right(outer)
            && bottom(inner) <= bottom(outer)
    }

    const SCREEN: Rect = rect(0, 0, SCREEN_W as u32, SCREEN_H as u32);

    #[test]
    fn regions_split_the_screen_without_gaps() {
        assert_eq!(HUD.h + MAP_H as u32 + BAND.h, SCREEN_H as u32);
        assert_eq!((MAP_Y, bottom(HUD)), (HUD.y + HUD.h as i32, MAP_Y));
        assert_eq!(MAP_Y + MAP_H, BAND.y);
        assert_eq!(bottom(BAND), SCREEN_H);
        assert_eq!(
            (MAP_W, HUD.w as i32, BAND.w as i32),
            (SCREEN_W, SCREEN_W, SCREEN_W)
        );
        assert_eq!(MAP_H % TILE, 0, "表示域の高さはマスの整数倍");
        assert_eq!(MAP_W_PANEL + PANEL.w as i32, SCREEN_W);
        assert_eq!((PANEL.y, PANEL.h as i32), (MAP_Y, MAP_H));
        assert_eq!(HUD_ROW1.h + HUD_ROW2.h, HUD.h);
        assert_eq!(
            (HUD_ROW2.y, bottom(HUD_ROW2)),
            (bottom(HUD_ROW1), bottom(HUD))
        );
        assert_eq!(INFO_ROW.h + BAND_ROW2.h, BAND.h);
        assert_eq!((INFO_ROW.y, BAND_ROW2.y), (BAND.y, bottom(INFO_ROW)));
        assert_eq!(
            MAP_W_PANEL % TILE,
            0,
            "パネルを開いても表示域はマスの整数倍"
        );
    }

    #[test]
    fn rows_match_the_font_line_heights() {
        let body = i32::from(face(FontId::Body12).line_height);
        let head = i32::from(face(FontId::Head16).line_height);
        assert_eq!(HUD_ROW1.h as i32, body);
        assert_eq!(INFO_ROW.h as i32, body);
        assert!(BAND_ROW2.h as i32 >= body.max(16), "ボタンの絵 16px が入る");
        assert_eq!(SETTINGS_ROW_H, head);
        assert_eq!(explain_h(2) - explain_h(1), head as u32);
        assert_eq!(line_confirm_h(2) - line_confirm_h(1), body as u32);
        assert_eq!((BANNER_H3 - BANNER.h) as i32, body);
    }

    #[test]
    fn hud_and_band_parts_follow_13_3_5() {
        assert_eq!(right(GAUGE), SCREEN_W - HUD_PAD);
        assert_eq!(GAUGE.x - LEVEL_RIGHT, 8, "会社レベルとゲージの間");
        assert!(inside(GAUGE, HUD_ROW1) && inside(SAVE_MARK, HUD_ROW2));
        assert_eq!(GOAL_RIGHT, SAVE_MARK.x - HUD_PAD);
        assert_eq!(INFO_TEXT_X + i32::from(INFO_TEXT_W), SCREEN_W - INFO_TEXT_X);
        assert_eq!(BAND_TEXT_X + i32::from(BAND_TEXT_W), right(BAND_LEFT));
        assert!(right(BAND_LEFT) < BAND_RIGHT.x);
        assert_eq!(right(BAND_RIGHT), SCREEN_W - HUD_PAD);
        assert!(inside(BAND_LEFT, BAND_ROW2) && inside(BAND_RIGHT, BAND_ROW2));
    }

    #[test]
    fn boxes_stay_on_screen_and_over_the_map() {
        let map = rect(MAP_X, MAP_Y, MAP_W as u32, MAP_H as u32);
        for r in [
            SETTINGS,
            SUB_CONFIRM,
            BUILD_CHOICES,
            BANNER,
            TOAST,
            GOAL_BAND,
            rect(EXPLAIN_X, BOX_TOP_Y, EXPLAIN_W, explain_h(EXPLAIN_LINES)),
            rect(LINE_CONFIRM_X, BOX_TOP_Y, LINE_CONFIRM_W, line_confirm_h(3)),
        ] {
            assert!(inside(r, SCREEN) && inside(r, map), "{r:?}");
        }
        assert!(
            TOAST.y >= BANNER.y + BANNER_H3 as i32,
            "帯が 3 行でもトーストと重ならない"
        );
        assert_eq!(bottom(BUILD_CHOICES), BOX_BOTTOM);
        assert!(
            EXPLAIN_X_PANEL + EXPLAIN_W as i32 <= MAP_W_PANEL,
            "パネルを隠さない"
        );
        assert_eq!(
            (EXPLAIN_W as i32 - 2 * EXPLAIN_PAD) / i32::from(face(FontId::Head16).full_advance),
            23,
            "内側の幅 376 は全角 23 字"
        );
    }

    #[test]
    fn settings_sub_boxes_sit_below_the_settings_box() {
        assert_eq!(
            SETTINGS.h as i32,
            8 + 20 + 4 + SETTINGS_ROWS * SETTINGS_ROW_H + 8 + 8,
            "見出し + 7 行 + 区切り 8"
        );
        let page = rect(
            EXPLAIN_X,
            BOX_BOTTOM - explain_h(EXPLAIN_LINES) as i32,
            EXPLAIN_W,
            explain_h(EXPLAIN_LINES),
        );
        for r in [SUB_CONFIRM, page] {
            assert!(!overlaps(r, SETTINGS), "{r:?}");
            assert!(r.y > bottom(SETTINGS));
            assert!(bottom(r) <= BOX_BOTTOM);
        }
    }

    #[test]
    fn title_and_quit_boxes_follow_13_3_5() {
        let [a, b] = TITLE_ROWS;
        assert_eq!((a.x, a.w, a.h), (b.x, b.w, b.h));
        assert_eq!(b.y - a.y, 32, "縦 32px 間隔(UW-12)");
        assert_eq!(a.x + a.w as i32 / 2, SCREEN_W / 2, "中央");
        assert!(bottom(b) <= BAND.y);
        assert!(
            !overlaps(SUB_CONFIRM, rect(0, 0, SCREEN_W as u32, a.y as u32)),
            "タイトルの確認はメニューの場所に出し、ロゴと副題を隠さない"
        );
        let head = i32::from(face(FontId::Head16).line_height);
        assert!(a.h as i32 >= head);
        assert!(POWER_BOX.h as i32 >= 2 * head + 16, "Head16 の 2 行");
        for r in [SAVING_BOX, POWER_BOX] {
            assert!(inside(r, SCREEN), "{r:?}");
            assert_eq!(r.x + r.w as i32 / 2, SCREEN_W / 2, "中央: {r:?}");
        }
        assert!(SAVING_BOX.h as i32 >= i32::from(face(FontId::Body12).line_height));
    }

    #[test]
    fn tile_to_screen() {
        let p = GridPos { x: 3, y: 2 };
        assert_eq!(TILE, 32, "16px のマスの絵を 2 倍で描く");
        assert_eq!(tile_screen(p, Cam::default()), (3 * TILE, MAP_Y + 2 * TILE));
        assert_eq!(
            tile_screen(p, Cam::at(37, 21)),
            (3 * TILE - 37, MAP_Y + 2 * TILE - 21)
        );
        assert_eq!(
            tile_center(p, Cam::default()),
            (3 * TILE + 16, MAP_Y + 2 * TILE + 16)
        );
        assert_eq!(tile_layer(p), (3 * TILE, 2 * TILE));
    }

    #[test]
    fn the_map_is_larger_than_the_view_and_split_into_layers() {
        assert_eq!((WORLD_W, WORLD_H), (1280, 576));
        assert_eq!(
            (WORLD_W / MAP_W, WORLD_H / MAP_H),
            (2, 2),
            "全体は表示域の縦横 2 倍(画面に収まらない)"
        );
        assert_eq!(MAP_W_PANEL / TILE, 13, "パネルを開くと 13 マス");
        assert_eq!(LAYER_COUNT, 2);
        assert_eq!(LAYER_W % TILE, 0);
        assert_eq!(LAYER_W * LAYER_COUNT as i32, WORLD_W, "2 枚で全体を覆う");
        assert_eq!(
            (
                LAYER_W,
                WORLD_H,
                LAYER_COUNT * (LAYER_W * WORLD_H * 4) as usize
            ),
            (640, 576, 2_949_120),
            "1 枚 640 × 576(1024px 以下)、合計 2.8MiB(4MiB 以下。14 §3.5.4)"
        );
    }

    #[test]
    fn near_view_keeps_one_tile_around_each_side() {
        for w in [MAP_W, MAP_W_PANEL] {
            let (l, r, t, b) = (
                MAP_X - TILE,
                MAP_X + w + TILE,
                MAP_Y - TILE,
                MAP_Y + MAP_H + TILE,
            );
            let mid = (MAP_X + w / 2, MAP_Y + MAP_H / 2);
            assert!(
                near_view(l, mid.1, w) && !near_view(l - 1, mid.1, w),
                "左 {w}"
            );
            assert!(
                near_view(r - 1, mid.1, w) && !near_view(r, mid.1, w),
                "右 {w}"
            );
            assert!(
                near_view(mid.0, t, w) && !near_view(mid.0, t - 1, w),
                "上 {w}"
            );
            assert!(
                near_view(mid.0, b - 1, w) && !near_view(mid.0, b, w),
                "下 {w}"
            );
        }
    }

    #[test]
    fn visibility_follows_the_camera_on_both_axes() {
        let p = GridPos { x: 30, y: 15 };
        assert!(!tile_in_view(p, Cam::default(), MAP_W));
        let cam = Cam::at(30 * TILE - MAP_W + TILE, 15 * TILE - MAP_H + TILE);
        assert!(tile_in_view(p, cam, MAP_W));
        assert!(!tile_in_view(p, cam, MAP_W_PANEL), "パネルの下");
        assert!(
            !tile_in_view(p, Cam::at(cam.x, cam.y - 1), MAP_W),
            "下にはみ出す"
        );
        let (x, y) = tile_center(p, Cam::default());
        assert!(!near_view(x, y, MAP_W));
        assert!(near_view(MAP_X - TILE, MAP_Y - TILE, MAP_W));
        assert!(!near_view(MAP_X + MAP_W_PANEL + TILE, MAP_Y, MAP_W_PANEL));
    }

    #[test]
    fn balance_map_matches_the_display_area() {
        let spec = tm_core::BalanceTable::builtin().spec().clone();
        assert_eq!(
            (i32::from(spec.map.width), i32::from(spec.map.height)),
            (MAP_COLS, MAP_ROWS)
        );
    }
}
