//! APP-T-17〜20: 環境変数、シード、devtools の変数、ロガー(16 §2.4、§1.11、§1.12)。

use log::{Level, LevelFilter, Log, Record};
use tm_app::logging::{LINE_MAX_BYTES, RING_LINES, RingLogger, parse_level};
use tm_app::seed::{mix, splitmix64};
use tm_app::{
    AppConfig, AppError, ConfigWarning, DEVTOOLS_VARS, DisplayChoice, ExitAction, PlatformEnv,
    ReplayPolicy, SavePolicy, SeedPolicy, SeedSource,
};
use tm_platform_api::MonoTime;

fn from(pairs: &[(&str, &str)]) -> (AppConfig, PlatformEnv, Vec<ConfigWarning>) {
    AppConfig::from_env(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())))
        .expect("from_env")
}

/// `HOME` がなければ足す(`TM_SAVE_DIR` の既定が決まるように)。
fn env(pairs: &[(&str, &str)]) -> (AppConfig, PlatformEnv, Vec<ConfigWarning>) {
    let mut v = pairs.to_vec();
    if !v.iter().any(|(k, _)| *k == "HOME") {
        v.push(("HOME", "/home/tm"));
    }
    from(&v)
}

fn save_dir(app: &AppConfig) -> Option<&str> {
    match &app.save {
        SavePolicy::Dir(d) => d.to_str(),
        SavePolicy::Off => None,
        SavePolicy::Memory(_) => panic!("Memory"),
    }
}

fn invalid(var: &'static str, value: &str) -> ConfigWarning {
    ConfigWarning::Invalid {
        var,
        value: value.to_owned(),
    }
}

fn dev_default() -> PlatformEnv {
    PlatformEnv {
        target: false,
        display: DisplayChoice::Windowed { scale: 2 },
        window_scale: 2,
        fps30: false,
        vsync: true,
        copy_batch: false,
        render_driver: None,
        audio: true,
        audio_buffer: 1024,
        dev_hotkeys: None,
    }
}

#[test]
fn app_t_17_defaults() {
    let (app, p, w) = env(&[("HOME", "/root"), ("PATH", "/bin")]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(p, dev_default());
    assert_eq!(save_dir(&app), Some("/root/.local/share/trainmaster"));
    assert_eq!(app.time_scale, 1);
    assert!(!app.overlay);
    assert_eq!(app.quit_after_frames, None);
    assert_eq!(app.exit_action, ExitAction::Exit);

    let (_, p, w) = env(&[("SDL_VIDEODRIVER", "kmsdrm")]);
    assert!(w.is_empty());
    assert_eq!(
        p,
        PlatformEnv {
            target: true,
            display: DisplayChoice::Auto,
            render_driver: Some("opengles2".into()),
            ..dev_default()
        },
        "実機の既定"
    );
}

#[test]
fn app_t_17_valid_values() {
    let (_, p, w) = env(&[
        ("TM_DISPLAY", "window"),
        ("TM_WINDOW_SCALE", "3"),
        ("TM_FPS", "30"),
        ("TM_VSYNC", "off"),
        ("TM_RENDER_BATCH", "copy"),
        ("TM_RENDER_DRIVER", "opengl"),
        ("TM_AUDIO", "off"),
        ("TM_AUDIO_BUFFER", "2048"),
        ("TM_LOG", "debug"),
    ]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(
        p,
        PlatformEnv {
            target: false,
            display: DisplayChoice::Windowed { scale: 3 },
            window_scale: 3,
            fps30: true,
            vsync: false,
            copy_batch: true,
            render_driver: Some("opengl".into()),
            audio: false,
            audio_buffer: 2048,
            dev_hotkeys: None,
        }
    );
    let (_, p, _) = env(&[
        ("SDL_VIDEODRIVER", "kmsdrm"),
        ("TM_DISPLAY", "fullscreen"),
        ("TM_RENDER_DRIVER", "auto"),
        ("TM_FPS", "60"),
        ("TM_VSYNC", "on"),
        ("TM_RENDER_BATCH", "geometry"),
        ("TM_AUDIO", "on"),
        ("TM_AUDIO_BUFFER", "512"),
    ]);
    assert_eq!(p.display, DisplayChoice::Fullscreen);
    assert_eq!(p.render_driver, None);
    assert_eq!(p.audio_buffer, 512);
    let (_, p, _) = env(&[("TM_DISPLAY", "auto"), ("TM_WINDOW_SCALE", "6")]);
    assert_eq!(
        p.display,
        DisplayChoice::Auto,
        "開発機の auto はフルスクリーン(ドライバで決める)。TM_WINDOW_SCALE は window のときだけ"
    );
    let (_, p, _) = env(&[("TM_DISPLAY", "window"), ("TM_WINDOW_SCALE", "3")]);
    assert_eq!(
        p.display,
        DisplayChoice::Windowed { scale: 3 },
        "ウィンドウに戻せる"
    );
    let (_, p, _) = env(&[("SDL_VIDEODRIVER", "kmsdrm"), ("TM_DISPLAY", "window")]);
    assert_eq!(p.display, DisplayChoice::Windowed { scale: 2 });
    for level in ["error", "warn", "info", "debug", "trace"] {
        assert!(parse_level(level).is_some());
    }
}

#[test]
fn app_t_17_invalid_values_warn_and_use_defaults() {
    let bad = [
        ("TM_DISPLAY", "big"),
        ("TM_WINDOW_SCALE", "7"),
        ("TM_FPS", "45"),
        ("TM_VSYNC", "yes"),
        ("TM_RENDER_BATCH", "fast"),
        ("TM_RENDER_DRIVER", "open gl"),
        ("TM_AUDIO", "1"),
        ("TM_AUDIO_BUFFER", "1000"),
        ("TM_LOG", "loud"),
    ];
    let (_, p, w) = env(&bad);
    assert_eq!(p, dev_default());
    assert_eq!(w.len(), bad.len(), "{w:?}");
    for (k, v) in bad {
        assert!(
            w.iter().any(
                |x| matches!(x, ConfigWarning::Invalid { var, value } if *var == k && value == v)
            ),
            "{k}"
        );
    }
    let (_, p, w) = env(&[("TM_WINDOW_SCALE", "0"), ("SDL_VIDEODRIVER", "kmsdrm")]);
    assert_eq!(w, [invalid("TM_WINDOW_SCALE", "0")]);
    assert_eq!(p.render_driver.as_deref(), Some("opengles2"));
    let (_, p, w) = env(&[("TM_RENDER_DRIVER", ""), ("SDL_VIDEODRIVER", "kmsdrm")]);
    assert_eq!(w, [invalid("TM_RENDER_DRIVER", "")]);
    assert_eq!(p.render_driver.as_deref(), Some("opengles2"));
    assert!(w[0].to_string().contains("TM_RENDER_DRIVER"));
}

/// `TM_EXIT_ACTION` の値と、アプリの解釈。
const EXIT_ACTIONS: [(&str, Option<ExitAction>); 18] = [
    ("poweroff", Some(ExitAction::PowerOff)),
    ("PowerOff", Some(ExitAction::PowerOff)),
    (" POWEROFF ", Some(ExitAction::PowerOff)),
    ("\tpoweroff\n", Some(ExitAction::PowerOff)),
    ("exit", Some(ExitAction::Exit)),
    (" Exit", Some(ExitAction::Exit)),
    ("", None),
    (" ", None),
    ("power off", None),
    ("power_off", None),
    ("poweroff10", None),
    ("reboot", None),
    ("1", None),
    ("\x0bpoweroff\x0c\r", Some(ExitAction::PowerOff)),
    ("power\toff", None),
    ("poweroff\u{3000}", None),
    ("\u{a0}poweroff", None),
    ("poweroff\u{85}", None),
];

/// 前後の ASCII の空白(tm-launch の `[:space:]`)を除いた値。
fn shell_trim(v: &str) -> &str {
    v.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\x0B' | '\x0C' | '\r'))
}

#[test]
fn app_t_17_exit_action() {
    for (v, want) in EXIT_ACTIONS {
        let (app, _, w) = env(&[("TM_EXIT_ACTION", v)]);
        assert_eq!(app.exit_action, want.unwrap_or(ExitAction::Exit), "{v:?}");
        let want_w: Vec<_> = match want {
            Some(_) => vec![],
            None => vec![invalid("TM_EXIT_ACTION", shell_trim(v))],
        };
        assert_eq!(w, want_w, "{v:?}");
    }
}

/// tm-launch(03 §5.3)の `exit_action=` の行を `sh` で動かした結果。
fn launch_exit_action(script: &str, value: &str) -> String {
    let line = script
        .lines()
        .find(|l| l.starts_with("exit_action="))
        .expect("tm-launch の exit_action= の行");
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{line}\nprintf '%s' \"$exit_action\""))
        .env("TM_EXIT_ACTION", value)
        .output()
        .expect("sh");
    assert!(out.status.success());
    String::from_utf8(out.stdout).expect("UTF-8")
}

#[test]
fn app_t_17_exit_action_matches_tm_launch() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../br2-external/board/rpi3-64/rootfs-overlay/usr/libexec/tm-launch"
    );
    let script = std::fs::read_to_string(path).expect("tm-launch");
    for (v, _) in EXIT_ACTIONS {
        let app = env(&[("TM_EXIT_ACTION", v)]).0.exit_action;
        let launch = launch_exit_action(&script, v) == "poweroff";
        assert_eq!(app == ExitAction::PowerOff, launch, "{v:?}");
    }
}

#[test]
fn app_t_17_save_dir() {
    let (app, _, w) = from(&[("TM_SAVE_DIR", "/data/trainmaster"), ("HOME", "/root")]);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(save_dir(&app), Some("/data/trainmaster"));
    for off in ["off", " OFF ", "Off"] {
        let (app, _, w) = from(&[("TM_SAVE_DIR", off), ("HOME", "/root")]);
        assert!(
            w.is_empty() && matches!(app.save, SavePolicy::Off),
            "{off:?}"
        );
    }
    let (app, _, w) = from(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "/x/data")]);
    assert!(w.is_empty());
    assert_eq!(
        save_dir(&app),
        Some("/x/data/trainmaster"),
        "XDG_DATA_HOME を先に"
    );
    let (app, _, _) = from(&[("HOME", "/home/a"), ("XDG_DATA_HOME", "rel")]);
    assert_eq!(
        save_dir(&app),
        Some("/home/a/.local/share/trainmaster"),
        "相対の XDG_DATA_HOME は使わない"
    );
    let (app, _, w) = from(&[]);
    assert_eq!(w, [ConfigWarning::NoSaveDir]);
    assert!(matches!(app.save, SavePolicy::Off));
    assert!(w[0].to_string().contains("TM_SAVE_DIR"));
    for home in ["", " ", "rel/home"] {
        let (app, _, w) = from(&[("HOME", home)]);
        assert_eq!(w, [ConfigWarning::NoSaveDir], "HOME={home:?}");
        assert!(
            matches!(app.save, SavePolicy::Off),
            "相対の保存先にしない: {home:?}"
        );
    }
    let (app, _, w) = from(&[("TM_SAVE_DIR", " "), ("HOME", "/h")]);
    assert_eq!(w, [invalid("TM_SAVE_DIR", "")]);
    assert_eq!(
        save_dir(&app),
        Some("/h/.local/share/trainmaster"),
        "空なら既定"
    );
    assert!(
        matches!(AppConfig::default().save, SavePolicy::Off),
        "テストの既定は保存しない"
    );
}

#[test]
fn app_t_18_seed() {
    let mut s = SeedSource::new(SeedPolicy::Fixed(0xDEAD_BEEF_0123_4567));
    assert_eq!(s.next_seed(MonoTime(5)), 0xDEAD_BEEF_0123_4567);
    assert_eq!(s.next_seed(MonoTime(99)), 0xDEAD_BEEF_0123_4567, "混ぜない");

    assert_eq!(splitmix64(0), 0xE220_A839_7B1D_CDAF, "SplitMix64 の参照値");
    let mut s = SeedSource::with_entropy(SeedPolicy::Os, || Ok(0x0123_4567_89AB_CDEF));
    let seed = s.next_seed(MonoTime(1_000_000));
    assert_eq!(seed, mix(0x0123_4567_89AB_CDEF, MonoTime(1_000_000)));
    assert_eq!(seed, 0x0123_4567_89AB_CDEF ^ splitmix64(1_000_000));
    assert_eq!(seed, 0x692E_59A9_1554_8808, "ゴールデン(Python で別に計算)");
    assert_ne!(s.next_seed(MonoTime(1_000_001)), seed, "時計で変わる");

    let mut s = SeedSource::with_entropy(SeedPolicy::Os, || Err("壊れた".into()));
    let a = s.next_seed(MonoTime(7));
    let b = s.next_seed(MonoTime(7));
    assert_eq!(a, splitmix64(7 ^ splitmix64(1)), "時計から作る");
    assert_ne!(a, b);

    let mut s = SeedSource::new(SeedPolicy::Script(vec![1, 2]));
    let got: Vec<u64> = (0..3).map(|_| s.next_seed(MonoTime(0))).collect();
    assert_eq!(got, [1, 2, 1]);
    assert_ne!(SeedSource::new(SeedPolicy::Os).next_seed(MonoTime(0)), 0);
}

#[test]
fn app_t_19_devtools_vars() {
    let dir = std::env::temp_dir().join(format!("tm-app-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let balance = dir.join("balance.ron");
    std::fs::write(&balance, tm_core::BUILTIN_BALANCE_RON).unwrap();
    let vars = [
        ("TM_DEV_HOTKEYS", "off"),
        ("TM_SEED", "0x2A"),
        ("TM_REPLAY", "on"),
        ("TM_REPLAY_DIR", "/tmp/tm-replays"),
        ("TM_BALANCE_FILE", balance.to_str().unwrap()),
        ("TM_DEBUG_OVERLAY", "on"),
        ("TM_TIME_SCALE", "8"),
        ("TM_QUIT_AFTER_FRAMES", "120"),
    ];
    assert_eq!(vars.len(), DEVTOOLS_VARS.len());
    let (app, p, w) = env(&vars);
    if cfg!(feature = "devtools") {
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(app.seed, SeedPolicy::Fixed(42));
        assert!(matches!(&app.replay, ReplayPolicy::Dir(d) if d.as_os_str() == "/tmp/tm-replays"));
        assert!(app.overlay);
        assert_eq!(app.time_scale, 8);
        assert_eq!(app.quit_after_frames, Some(120));
        assert_eq!(p.dev_hotkeys, Some(false));
        assert_eq!(
            app.balance.config_hash(),
            tm_core::BalanceTable::builtin().config_hash()
        );

        let (app, _, w) = env(&[]);
        assert!(w.is_empty());
        assert!(
            matches!(&app.replay, ReplayPolicy::Dir(d) if d.as_os_str() == "/tmp"),
            "devtools ではリプレイが既定で on"
        );
        assert_eq!(app.seed, SeedPolicy::Os);
        let (app, _, w) = env(&[
            ("TM_REPLAY", "off"),
            ("TM_SEED", "12345"),
            ("TM_TIME_SCALE", "17"),
        ]);
        assert!(matches!(app.replay, ReplayPolicy::Off));
        assert_eq!(app.seed, SeedPolicy::Fixed(12345));
        assert_eq!(app.time_scale, 1);
        assert_eq!(w, [invalid("TM_TIME_SCALE", "17")]);

        std::fs::write(&balance, "( broken").unwrap();
        let r = AppConfig::from_env([(
            "TM_BALANCE_FILE".to_string(),
            balance.to_str().unwrap().to_string(),
        )]);
        assert!(matches!(r, Err(tm_app::AppError::Balance(_))));
        let r = AppConfig::from_env([(
            "TM_BALANCE_FILE".to_string(),
            dir.join("none.ron").to_str().unwrap().to_string(),
        )]);
        assert!(matches!(r, Err(tm_app::AppError::Config(_))));
    } else {
        assert_eq!(w.len(), DEVTOOLS_VARS.len(), "{w:?}");
        for var in DEVTOOLS_VARS {
            assert!(w.contains(&ConfigWarning::DevtoolsOnly { var }), "{var}");
        }
        assert_eq!(app.seed, SeedPolicy::Os);
        assert!(matches!(app.replay, ReplayPolicy::Off));
        assert!(!app.overlay);
        assert_eq!(app.time_scale, 1);
        assert_eq!(app.quit_after_frames, None);
        assert_eq!(p.dev_hotkeys, None);
        let r = AppConfig::from_env([("TM_BALANCE_FILE".to_string(), "/none".to_string())]);
        assert!(r.is_ok(), "devtools なしでは読まない");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

fn line(logger: &RingLogger, level: Level, msg: &str) {
    logger.log(
        &Record::builder()
            .level(level)
            .target("t")
            .args(format_args!("{msg}"))
            .build(),
    );
}

#[test]
fn app_t_20_ring_logger() {
    let l = RingLogger::new(LevelFilter::Info, false);
    assert!(l.is_empty());
    for i in 0..RING_LINES + 44 {
        line(&l, Level::Info, &format!("行 {i}"));
    }
    assert_eq!(l.len(), RING_LINES, "容量 256");
    let last = l.lines(3);
    assert_eq!(last.len(), 3);
    assert!(
        last[0].ends_with("行 297") && last[2].ends_with("行 299"),
        "{last:?}"
    );
    assert!(
        l.lines(RING_LINES)[0].ends_with("行 44"),
        "古いものから捨てる"
    );
    assert!(last[2].contains("INFO  t]"), "{}", last[2]);

    line(&l, Level::Debug, "出ない");
    let w = RingLogger::new(LevelFilter::Warn, false);
    line(&w, Level::Info, "出ない");
    w.always(format_args!("trainmaster 版"));
    assert_eq!(w.lines(5).len(), 1, "版の行は TM_LOG=warn でも出る");
    assert!(w.lines(1)[0].ends_with("trainmaster 版"));
    line(&l, Level::Warn, "警告");
    assert!(l.lines(1)[0].ends_with("警告"), "レベルの絞り込み");

    line(&l, Level::Error, &"あ".repeat(100));
    let long = &l.lines(1)[0];
    assert!(long.len() <= LINE_MAX_BYTES && long.len() > LINE_MAX_BYTES - 3);
    assert!(long.ends_with('あ'), "文字の途中で切らない");
    assert_eq!(tm_app::logging::recent(5).len(), 0, "init の前は空");
}

#[test]
fn reincarnation_config_is_a_file() {
    let path = std::env::temp_dir().join(format!(
        "tm-app-reincarnation-{}-{}.toml",
        std::process::id(),
        std::line!()
    ));
    std::fs::write(&path, "format_version = 1\n[gain]\nbase = 9\n").unwrap();
    let (app, _, warnings) = env(&[("TM_REINCARNATION_FILE", path.to_str().unwrap())]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(app.reincarnation.spec().gain.base, 9);
    std::fs::remove_file(&path).unwrap();

    let broken = std::env::temp_dir().join(format!(
        "tm-app-reincarnation-broken-{}",
        std::process::id()
    ));
    std::fs::write(&broken, "format_version = 1\n[gain]\nbase = \"x\"\n").unwrap();
    let vars = [("TM_REINCARNATION_FILE", broken.to_str().unwrap())];
    let err = AppConfig::from_env(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())))
        .expect_err("読めない設定はエラー");
    assert!(matches!(err, AppError::Reincarnation(_)), "{err:?}");
    std::fs::remove_file(&broken).unwrap();

    let missing = std::env::temp_dir().join("tm-app-reincarnation-does-not-exist.toml");
    let vars = [("TM_REINCARNATION_FILE", missing.to_str().unwrap())];
    let err = AppConfig::from_env(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())))
        .expect_err("ないファイルはエラー");
    assert!(matches!(err, AppError::Config(_)), "{err:?}");
}
