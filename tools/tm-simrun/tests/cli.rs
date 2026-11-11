//! CLI の通し(`test:nosdl`): `record --snapshot` → `replay` → `resume --out` → `replay`(v2)、`run` の新しいオプション。

use std::path::PathBuf;
use std::process::{Command, Output};

fn tm_simrun(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tm-simrun"))
        .args(args)
        .output()
        .expect("tm-simrun を起動できる")
}

fn ok(args: &[&str]) -> (String, String) {
    let out = tm_simrun(args);
    let (so, se) = (
        String::from_utf8(out.stdout).expect("UTF-8"),
        String::from_utf8(out.stderr).expect("UTF-8"),
    );
    assert!(out.status.success(), "{args:?}\n{se}");
    (so, se)
}

fn dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&d).expect("作業用のディレクトリ");
    d
}

fn last_line(s: &str) -> &str {
    s.lines().last().expect("1 行以上")
}

#[test]
fn record_resume_and_replay_round_trip() {
    let d = dir("cli-round-trip");
    let p = |f: &str| d.join(f).to_str().expect("UTF-8 のパス").to_string();
    let (rec, snap, res) = (p("rec"), p("snap.ron"), p("res"));
    ok(&[
        "record",
        "--player",
        "b",
        "--pause",
        "none",
        "--seed",
        "3",
        "--minutes",
        "3",
        "--out",
        &rec,
        "--snapshot",
        &snap,
    ]);
    let (replayed, _) = ok(&["replay", &format!("{rec}.ron")]);
    assert!(replayed.starts_with("10800 "), "{replayed}");

    let (csv, err) = ok(&[
        "resume",
        &snap,
        "--minutes",
        "2",
        "--route",
        "near",
        "--out",
        &res,
    ]);
    let ended = last_line(&err);
    assert!(ended.starts_with("18000 "), "{err}");
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 2, "{csv}");
    assert!(rows[0].starts_with("seed,player,pause,route,lv1,"));
    assert!(rows[1].starts_with("3,b,none,near,"), "{}", rows[1]);
    let (again, _) = ok(&["replay", &format!("{res}.ron")]);
    assert_eq!(again.trim(), ended);
    let (mid, _) = ok(&["replay", &format!("{res}.ron"), "--until", "14400"]);
    assert!(mid.starts_with("14400 "), "{mid}");
}

#[test]
fn run_accepts_week_options() {
    let (csv, err) = ok(&[
        "run",
        "--player",
        "b",
        "--pause",
        "none",
        "--route",
        "all",
        "--seeds",
        "1",
        "--minutes",
        "6",
        "--save",
        "600",
        "--sessions",
        "2",
        "--idle",
        "4-",
        "--builds",
    ]);
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows.len(), 3, "{csv}");
    let cols = rows[0].split(',').count();
    for (row, route) in rows[1..].iter().zip(["far", "near"]) {
        assert!(row.starts_with(&format!("1,b,none,{route},")), "{row}");
        assert_eq!(row.split(',').count(), cols);
        let reach = row.rsplit(',').next().expect("reach");
        assert_eq!(reach.split(';').count(), 3, "{reach}");
        assert!(reach.starts_with("2="), "{reach}");
    }
    let mut builds = err
        .lines()
        .skip_while(|l| !l.starts_with("seed,player,route,wall_min"));
    assert_eq!(
        builds.next(),
        Some("seed,player,route,wall_min,level,new_tiles,cost,income,cost_s")
    );
    let paid: Vec<Vec<f64>> = builds
        .map(|l| {
            l.split(',')
                .skip(6)
                .map(|c| c.parse().unwrap_or(f64::NAN))
                .collect()
        })
        .filter(|c: &Vec<f64>| c[0] > 0.0)
        .collect();
    assert!(!paid.is_empty(), "{err}");
    for c in &paid {
        assert!(
            c[1] > 0.0 && (c[2] - c[0] / c[1]).abs() <= 0.01 * c[2] + 0.01,
            "{c:?}"
        );
    }
    for bad in [
        &["run", "--route", "x"][..],
        &["run", "--idle", "9-3"],
        &["resume", "/nonexistent"],
        &["run", "--sessions", "0"],
        &["run", "--sessions", "80000000"],
        &["run", "--sessions", "x"],
    ] {
        assert!(!tm_simrun(bad).status.success(), "{bad:?}");
    }
}

/// ゴールを 1 億円にした表。
fn near_goal_balance(d: &std::path::Path) -> String {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/tm-core/data/balance.ron"
    ))
    .expect("組み込みの表");
    assert_eq!(src.matches("goal_pow10: 68").count(), 1);
    let path = d.join("goal8.ron");
    std::fs::write(&path, src.replace("goal_pow10: 68", "goal_pow10: 8")).expect("表を書く");
    path.to_str().expect("UTF-8 のパス").to_string()
}

fn column<'a>(csv: &'a str, name: &str) -> &'a str {
    let mut lines = csv.lines();
    let i = lines
        .next()
        .and_then(|h| h.split(',').position(|c| c == name))
        .expect("列");
    lines.next().and_then(|r| r.split(',').nth(i)).expect("行")
}

#[test]
fn stop_at_goal_ends_run_and_resume_at_goal() {
    let d = dir("cli-goal");
    let bal = near_goal_balance(&d);
    let base = [
        "--player",
        "b",
        "--pause",
        "none",
        "--balance",
        bal.as_str(),
    ];
    let run = |extra: &[&str]| {
        let mut args = vec!["run", "--seeds", "1", "--minutes", "20"];
        args.extend(base);
        args.extend(extra);
        ok(&args).0
    };
    let full = run(&[]);
    let goal = column(&full, "goal").to_string();
    assert!(goal.parse::<f64>().is_ok_and(|g| g < 20.0), "{full}");
    assert_eq!(column(&full, "end_min"), "20.00");
    let stopped = run(&["--stop-at-goal"]);
    assert_eq!(column(&stopped, "end_min"), goal);

    let snap = d.join("snap.ron").to_str().expect("UTF-8").to_string();
    let rec = d.join("rec").to_str().expect("UTF-8").to_string();
    let mut args = vec![
        "record",
        "--seed",
        "1",
        "--minutes",
        "1",
        "--out",
        &rec,
        "--snapshot",
        &snap,
    ];
    args.extend(base);
    ok(&args);
    let mut args = vec!["resume", snap.as_str(), "--minutes", "20", "--stop-at-goal"];
    args.extend(base);
    let (resumed, _) = ok(&args);
    let g = column(&resumed, "goal");
    assert!(!g.is_empty(), "{resumed}");
    assert_eq!(column(&resumed, "end_min"), g);
}

#[test]
fn run_saves_for_track_up_to_300_seconds_by_default() {
    let run = |extra: &[&str]| {
        let mut args = vec![
            "run",
            "--seeds",
            "2",
            "--player",
            "b",
            "--pause",
            "none",
            "--minutes",
            "10",
        ];
        args.extend(extra);
        ok(&args).0
    };
    let default = run(&[]);
    assert_eq!(default, run(&["--save", "300"]));
    assert_ne!(default, run(&["--save", "1800"]));
}
