//! APP-T-23: 実際の SDL(dummy ドライバ)で `App` を回す。あわせて UI-V-08 の比較(`tm-ui` の画面を SDL と `SoftRaster` で描く)。
//! `mise run test:sdl`(`SDL_VIDEODRIVER=dummy`、`SDL_AUDIODRIVER=dummy`)で動かす。

#![cfg(feature = "sdl")]

use std::sync::{Arc, Mutex};

use tm_app::save::encode_settings;
use tm_app::{
    App, AppConfig, AppExit, ExitAction, FrameOutcome, MemStore, SavePolicy, SeedPolicy,
    ShutdownReason, run,
};
use tm_core::{
    BalanceTable, Bend, BuyMode, Command, Game, GameConfig, GridPos, ParamKey, StationId,
    UpgradeScope,
};
use tm_input::Action;
use tm_platform_api::{
    AssetBundle, AudioCmd, DrawCmd, DrawList, Key, MonoTime, PhysicalInput, Platform,
    PlatformError, SoftRaster,
};
use tm_platform_sdl::{DisplayMode, SdlConfig, SdlCore, SdlPlatform};
use tm_ui::testing::{UiHarness, draw_text};
use tm_ui::{AppStatus, Settings};

static SDL_LOCK: Mutex<()> = Mutex::new(());

fn sdl_config() -> SdlConfig {
    let mut c = SdlConfig::dev_default();
    c.display = DisplayMode::Windowed { scale: 1 };
    // dummy では最初の 120 フレームが待たずに回るので、初めからソフトウェアのフレーム制限にする(タイトルの 500ms のガード)
    c.vsync = false;
    c.dev_hotkeys = false;
    c
}

/// 決まったフレームで入力を足す。
struct Scripted<P> {
    inner: P,
    frame: u64,
    script: Vec<(u64, PhysicalInput)>,
    audio: Vec<(u64, AudioCmd)>,
}

impl<P: Platform> Platform for Scripted<P> {
    fn now(&self) -> MonoTime {
        self.inner.now()
    }
    fn poll_input(&mut self, out: &mut Vec<PhysicalInput>) {
        self.inner.poll_input(out);
        out.extend(
            self.script
                .iter()
                .filter(|(f, _)| *f == self.frame)
                .map(|(_, e)| *e),
        );
    }
    fn load_assets(&mut self, assets: &AssetBundle) -> Result<(), PlatformError> {
        self.inner.load_assets(assets)
    }
    fn present(&mut self, frame: &DrawList) -> Result<(), PlatformError> {
        self.frame += 1;
        self.inner.present(frame)
    }
    fn audio(&mut self, cmds: &[AudioCmd]) {
        self.audio.extend(cmds.iter().map(|&c| (self.frame, c)));
        self.inner.audio(cmds)
    }
    fn logical_size(&self) -> (u32, u32) {
        self.inner.logical_size()
    }
}

#[test]
fn app_t_23_sdl_smoke() {
    let _guard = SDL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cfg = sdl_config();
    let core = SdlCore::new(&cfg).expect("SdlCore::new");
    let tc = core.texture_creator();
    let platform = SdlPlatform::new(core, &tc, &cfg).expect("SdlPlatform::new");
    let mut p = Scripted {
        inner: platform,
        frame: 0,
        script: vec![
            (40, PhysicalInput::KeyDown(Key::Z)),
            (41, PhysicalInput::KeyUp(Key::Z)),
            (80, PhysicalInput::KeyDown(Key::Escape)),
            (81, PhysicalInput::KeyUp(Key::Escape)),
        ],
        audio: Vec::new(),
    };
    let app_cfg = AppConfig {
        seed: SeedPolicy::Fixed(1),
        quit_after_frames: Some(120),
        ..AppConfig::default()
    };
    let mut app = App::new(&mut p, app_cfg).expect("App::new");
    let mut frames = 0;
    loop {
        frames += 1;
        match app.frame(&mut p).expect("frame") {
            FrameOutcome::Continue => assert!(frames < 120),
            FrameOutcome::Quit(e) => {
                assert_eq!(e, AppExit::Normal);
                break;
            }
        }
    }
    assert_eq!(frames, 120);
    assert!(app.game().is_some(), "ゲームが始まった");
    assert!(app.ui().is_paused(app.tutorial()), "せってい が開いている");
    app.shutdown(ShutdownReason::Quit);
    drop(app);
    drop(p);
    drop(tc);
}

fn tap(script: &mut Vec<(u64, PhysicalInput)>, frame: u64, key: Key) {
    script.push((frame, PhysicalInput::KeyDown(key)));
    script.push((frame + 1, PhysicalInput::KeyUp(key)));
}

#[test]
fn app_t_30_sdl_quit_game_from_settings() {
    let _guard = SDL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cfg = sdl_config();
    let core = SdlCore::new(&cfg).expect("SdlCore::new");
    let tc = core.texture_creator();
    let platform = SdlPlatform::new(core, &tc, &cfg).expect("SdlPlatform::new");
    let mut script = Vec::new();
    tap(&mut script, 40, Key::Z);
    tap(&mut script, 80, Key::Escape);
    for i in 0..6 {
        tap(&mut script, 84 + 4 * i, Key::Down);
    }
    tap(&mut script, 108, Key::Up);
    tap(&mut script, 112, Key::Z);
    tap(&mut script, 116, Key::Right);
    tap(&mut script, 150, Key::Z);
    tap(&mut script, 154, Key::Right);
    tap(&mut script, 190, Key::Z);
    let mut p = Scripted {
        inner: platform,
        frame: 0,
        script,
        audio: Vec::new(),
    };
    let store = MemStore::new();
    let settings = Settings {
        show_explanations: false,
        ..Settings::default()
    };
    store.set_settings(Some(encode_settings(settings).into_bytes()));
    let app_cfg = AppConfig {
        seed: SeedPolicy::Fixed(1),
        save: SavePolicy::Memory(store.clone()),
        exit_action: ExitAction::PowerOff,
        ..AppConfig::default()
    };
    let exit = run(&mut p, app_cfg).expect("run");
    assert_eq!(exit, AppExit::PowerOff);
    let stops: Vec<_> = p
        .audio
        .iter()
        .filter(|(_, c)| *c == AudioCmd::StopBgm { fade_ms: 500 })
        .collect();
    assert_eq!(stops.len(), 1, "{stops:?}");
    assert_eq!(stops[0].0, 190, "2 回目の「はい」のフレーム");
    assert!(
        p.audio.iter().all(|(f, _)| *f <= 190),
        "終了中は音の命令を出さない"
    );
    let after = p.frame - 190;
    assert!((90..300).contains(&after), "電源の箱の 1500ms: {after}");
    let ticks: Vec<u64> = store.history().iter().filter_map(|w| w.tick).collect();
    assert!(ticks.last().is_some_and(|&t| t > 0), "{ticks:?}");
    drop(p);
    drop(tc);
}

fn play() -> UiHarness {
    let mut h = UiHarness::with_seed(1, false, MonoTime(1_000));
    h.frames(2);
    h
}

/// 部品の値段を 1 円、線路の 1 マスめを `c1_yen` にした表のゲーム。会社レベル 2 まで上げた画面にする。
fn priced(c1_yen: u64) -> UiHarness {
    let mut spec = BalanceTable::builtin().spec().clone();
    for p in &mut spec.params {
        p.base_yen = 1;
        p.cost_ratio = 1;
        p.cost_steps.clear();
    }
    spec.economy.initial_money_yen = 1_000_000;
    spec.economy.track.c1_yen = c1_yen;
    spec.economy.track.level_ratio = 1;
    spec.economy.track.tile_ratio = (1, 1);
    // 駅 2 を未接続のまま会社 Lv2 に上げるため、ゲージのしきい値を 10% にする。
    spec.company.gauge_num = 1;
    let game = Game::new(GameConfig {
        seed: 1,
        balance: Arc::new(BalanceTable::from_spec(spec).expect("表")),
    });
    let mut h = UiHarness::from_game(game, false);
    h.game
        .as_mut()
        .expect("ゲーム")
        .enqueue(Command::CreateLine {
            stations: vec![StationId(0), StationId(1)],
            bends: vec![Bend::StraightFirst],
        });
    h.frames(2);
    for _ in 0..60 {
        if h.game.as_ref().expect("ゲーム").view().company().level.0 >= 2 {
            break;
        }
        for param in ParamKey::ALL {
            let scope = match param {
                ParamKey::S1Spawn
                | ParamKey::S2Fare
                | ParamKey::S3StationCap
                | ParamKey::S4Platform => UpgradeScope::AllStations,
                ParamKey::R2LineSlots => UpgradeScope::Company,
                _ => UpgradeScope::AllLines,
            };
            let cmd = Command::LevelUp {
                param,
                scope,
                mode: BuyMode::Max,
            };
            let g = h.game.as_mut().expect("ゲーム");
            if g.validate(&cmd).is_ok() {
                g.enqueue(cmd);
            }
        }
        h.frames(2);
    }
    assert!(h.game.as_ref().expect("ゲーム").view().company().level.0 >= 2);
    h.frames(900);
    h
}

fn with_line() -> UiHarness {
    let mut h = play();
    h.game
        .as_mut()
        .expect("ゲーム")
        .enqueue(Command::CreateLine {
            stations: vec![StationId(0), StationId(1)],
            bends: vec![Bend::StraightFirst],
        });
    h.frames(300);
    h
}

fn station_pos(h: &UiHarness, id: u32) -> GridPos {
    let view = h.game.as_ref().expect("ゲーム").view();
    view.station(StationId(id)).expect("駅").pos
}

fn pick(h: &mut UiHarness, id: u32) {
    h.move_cursor_to(station_pos(h, id));
    h.press(Action::Confirm);
}

/// 路線ののばし方で端の駅を選び、新しい駅にカーソルを置いた(値段が出る)画面。
fn line_price(c1_yen: u64) -> UiHarness {
    let mut h = priced(c1_yen);
    h.press(Action::BuildMode);
    h.press(Action::MoveDown);
    h.press(Action::Confirm);
    pick(&mut h, 0);
    h.move_cursor_to(station_pos(&h, 2));
    h.frames(2);
    h
}

/// 始めの 3 駅が表示域(20 × 9 マス)に入りきらない駅の位置。始めのカメラは 3 駅の外接矩形の中心(マップの中央)なので、
/// 最初の駅 (2, 9) は表示域の外になり、#2 の案内は表示域の縁の矢印から始まる。
fn far_plan() -> Vec<GridPos> {
    let mut v = vec![
        GridPos { x: 2, y: 9 },
        GridPos { x: 37, y: 9 },
        GridPos { x: 20, y: 9 },
    ];
    v.extend((0..24).map(|k| GridPos {
        x: 21 + 3 * (k % 6),
        y: [1, 4, 13, 16][k as usize / 6],
    }));
    v
}

fn saving() -> AppStatus {
    AppStatus {
        save_enabled: true,
        saving: true,
        ..AppStatus::default()
    }
}

/// 画面ごとに別の `UiHarness`(別の `Ui`)なので、層の版が画面をまたいで同じ番号になりうる。
/// 1 つの `SdlPlatform` で続けて描くので、前の画面の層を使い回さないように版を画面の番号でずらす。
fn screens() -> Vec<(&'static str, DrawList)> {
    let mut out = Vec::new();
    let mut shot = |name, h: &UiHarness| {
        let mut list = h.draw_list().clone();
        let n = out.len() as u64 + 1;
        for cmd in &mut list.cmds {
            if let DrawCmd::BeginCachedLayer { revision, .. } = cmd {
                *revision += n << 32;
            }
        }
        out.push((name, list));
    };

    let mut h = UiHarness::new_title();
    h.frames(2);
    shot("title", &h);
    let mut h = UiHarness::new_title();
    h.set_app_status(AppStatus {
        save_enabled: true,
        has_save: true,
        ..AppStatus::default()
    });
    h.frames(2);
    shot("title_continue", &h);
    h.press(Action::MoveDown);
    h.frames(30);
    shot("title_new_game", &h);
    h.press(Action::Confirm);
    h.frames(2);
    shot("title_new_game_confirm", &h);

    let mut h = UiHarness::with_seed(1, true, MonoTime(1_000));
    h.frames(2);
    shot("explain", &h);
    let mut h = UiHarness::with_game(far_plan(), true);
    h.frames(2);
    for _ in 0..2 {
        h.frames(20);
        h.press(Action::Confirm);
    }
    shot("explain_far", &h);
    h.frames(40);
    shot("explain_mark", &h);
    h.frames(20);
    h.press(Action::Confirm);
    h.frames(40);
    shot("guide", &h);

    let mut h = with_line();
    shot("line", &h);
    h.move_cursor_to(station_pos(&h, 0));
    h.frames(2);
    shot("info_station", &h);
    h.move_cursor_to(GridPos { x: 18, y: 8 });
    h.frames(2);
    shot("info_track", &h);
    h.set_app_status(saving());
    h.frames(2);
    shot("hud_saving", &h);
    h.set_app_status(AppStatus::default());
    h.frames(2);
    h.press(Action::TabNext);
    h.frames(30);
    shot("panel", &h);
    h.press(Action::Cancel);
    h.press(Action::BuildMode);
    h.frames(10);
    shot("build", &h);
    h.press(Action::Cancel);
    h.move_cursor_to(GridPos { x: 38, y: 16 });
    h.frames(30);
    shot("map_far", &h);
    h.press(Action::Menu);
    h.frames(10);
    shot("settings", &h);
    for _ in 0..5 {
        h.press(Action::MoveDown);
    }
    h.press(Action::Confirm);
    h.frames(20);
    shot("quit_confirm_1_nosave", &h);
    h.press(Action::Cancel);
    h.frames(20);
    h.set_app_status(AppStatus {
        save_enabled: true,
        ..AppStatus::default()
    });
    h.press(Action::Confirm);
    h.frames(20);
    shot("quit_confirm_1", &h);
    h.press(Action::MoveRight);
    h.press(Action::Confirm);
    h.frames(20);
    shot("quit_confirm_2", &h);
    h.press(Action::MoveRight);
    h.press(Action::Confirm);
    h.set_app_status(saving());
    h.frames(2);
    shot("quitting_saving", &h);
    h.set_app_status(AppStatus {
        save_enabled: true,
        power_off_on_quit: true,
        ..AppStatus::default()
    });
    h.frames(2);
    shot("quitting_power_off", &h);

    let h = line_price(1);
    shot("line_price", &h);
    let mut h = line_price(10_000_000_000);
    shot("line_price_short", &h);
    h.press(Action::Confirm);
    h.frames(2);
    shot("line_price_refused", &h);
    out
}

/// 画面ごとに、その画面の字かスプライト(`draw_text` の `Sprite` の名前)が出ているか(出ていないか)。
const EXPECT: [(&str, &str, bool); 29] = [
    ("title", "'初'", true),
    ("title", "'続'", false),
    ("title_continue", "'続'", true),
    ("title_new_game", "'続'", true),
    ("title_new_game_confirm", "'遊'", true),
    ("explain", "'次'", true),
    ("explain_far", "ico_tri_left ", true),
    ("explain_mark", "ui_sel_tl ", true),
    ("guide", "ico_guide ", true),
    ("map_far", "ui_map_cursor ", true),
    ("map_far", "'待'", false),
    ("line", "'路'", true),
    ("info_station", "'待'", true),
    ("info_track", "'通'", true),
    ("hud_saving", "ico_save ", true),
    ("line", "ico_save ", false),
    ("panel", "'閉'", true),
    ("build", "'作'", true),
    ("settings", "'設'", true),
    ("quit_confirm_1_nosave", "'今'", true),
    ("quit_confirm_1", "'遊'", true),
    ("quit_confirm_2", "'本'", true),
    ("quitting_saving", "'保'", true),
    ("quitting_power_off", "'抜'", true),
    ("line_price", "'線'", true),
    ("line_price", "'足'", false),
    ("line_price_short", "'足'", true),
    ("line_price_refused", "'足'", true),
    ("line_price_refused", "'選'", false),
];

#[test]
fn ui_v_08_screens_show_what_they_name() {
    let all = screens();
    let names: Vec<&str> = all.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        [
            "title",
            "title_continue",
            "title_new_game",
            "title_new_game_confirm",
            "explain",
            "explain_far",
            "explain_mark",
            "guide",
            "line",
            "info_station",
            "info_track",
            "hud_saving",
            "panel",
            "build",
            "map_far",
            "settings",
            "quit_confirm_1_nosave",
            "quit_confirm_1",
            "quit_confirm_2",
            "quitting_saving",
            "quitting_power_off",
            "line_price",
            "line_price_short",
            "line_price_refused",
        ]
    );
    let texts: Vec<String> = all.iter().map(|(_, l)| draw_text(l)).collect();
    for (name, needle, shown) in EXPECT {
        let i = names.iter().position(|n| *n == name).expect(name);
        let has = texts[i].contains(&format!("Sprite {needle}"));
        assert_eq!(has, shown, "{name}: {needle}");
    }
    for (i, a) in texts.iter().enumerate() {
        for (j, b) in texts.iter().enumerate().skip(i + 1) {
            assert_ne!(a, b, "{} と {} が同じ画面", names[i], names[j]);
        }
    }
}

/// 半透明の重ね描き 1 回ごとに SDL と `SoftRaster` は最大 +2 ずれるので、最大差と ±2 を超える画素の割合で比べる(IMP-209 と同じ基準)。
const MAX_DIFF: u8 = 8;
/// ±2 を超えてよい画素の割合(百分率)。
const OVER_2_PERCENT: usize = 1;

#[test]
fn ui_v_08_ui_screens_match_soft_raster() {
    let _guard = SDL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let assets = tm_assets::bundle().expect("bundle");
    let mut cfg = sdl_config();
    cfg.audio.enabled = false;
    let core = SdlCore::new(&cfg).expect("SdlCore::new");
    let tc = core.texture_creator();
    let mut p = SdlPlatform::new(core, &tc, &cfg).expect("SdlPlatform::new");
    p.load_assets(assets).expect("load_assets");
    for (name, list) in screens() {
        p.present(&list).expect("present");
        let got = p.capture_frame().expect("capture_frame");
        let mut soft = SoftRaster::new();
        soft.draw(assets, &list);
        let want = soft.pixels();
        assert_eq!(got.len(), want.len(), "{name}");
        dump(name, &got);
        let diffs: Vec<u8> = got
            .chunks(4)
            .zip(want.chunks(4))
            .map(|(g, w)| (0..4).map(|k| g[k].abs_diff(w[k])).max().unwrap_or(0))
            .collect();
        let max = diffs.iter().copied().max().unwrap_or(0);
        let over = diffs.iter().filter(|&&d| d > 2).count();
        println!(
            "UI-V-08 {name}: 最大差 {max}、±2 超 {over} / {} ({:.3}%)",
            diffs.len(),
            over as f64 * 100.0 / diffs.len() as f64
        );
        assert!(max <= MAX_DIFF, "{name}: 最大差 {max}");
        assert!(
            over * 100 <= diffs.len() * OVER_2_PERCENT,
            "{name}: ±2 を超える画素 {over} / {}",
            diffs.len()
        );
    }
}

/// `TM_SDL_DUMP_DIR` があれば、SDL の画素を `TM_SDL_DUMP_SCALE`(既定 3)倍の PNG にして書く(目で見る用)。
fn dump(name: &str, rgba: &[u8]) {
    let Ok(dir) = std::env::var("TM_SDL_DUMP_DIR") else {
        return;
    };
    let scale: usize = std::env::var("TM_SDL_DUMP_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let (w, h) = (640, rgba.len() / 4 / 640);
    let mut big = Vec::with_capacity(rgba.len() * scale * scale);
    for row in rgba.chunks(w * 4) {
        let line: Vec<u8> = row
            .chunks(4)
            .flat_map(|px| {
                px.iter()
                    .copied()
                    .cycle()
                    .take(4 * scale)
                    .collect::<Vec<_>>()
            })
            .collect();
        for _ in 0..scale {
            big.extend_from_slice(&line);
        }
    }
    let png = png((w * scale) as u32, (h * scale) as u32, &big);
    std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), png)
        .expect("PNG を書けない");
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// 無圧縮の deflate の RGBA の PNG(依存を足さない)。
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in rgba.chunks(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &v in &raw {
        a = (a + u32::from(v)) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    for (kind, data) in [(&b"IHDR"[..], &ihdr[..]), (b"IDAT", &z), (b"IEND", &[])] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    out
}
