//! tm-simrun の CLI(`run`、`flow`、`record`、`replay`、`resume`)。

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::Arc;

use tm_core::{BalanceTable, Game, ReplayLog, Tick, replay};
use tm_simrun::report::BUILDS_HEADER;
use tm_simrun::save::read_snapshot;
use tm_simrun::scenario::{Pause, SAVE_S, Scenario, TPS};
use tm_simrun::{FlowSim, Idle, Player, Report, Route, Runner, csv_header, run_many};

const USAGE: &str = "使い方:
  tm-simrun run    [--player a|b|c|all] [--pause none|standard|long|all|T0+Tu] [--route far|near|all] [--seeds 1..=3]
                   [--minutes 30] [--save 300] [--sessions 90] [--idle 開始-終了[,…]] [--stop-at-goal]
                   [--balance FILE] [--routes] [--builds]
  tm-simrun flow   [--player a|b|c|all] [--pause none|standard|long|all|T0+Tu] [--minutes 40] [--save 300]
                   [--idle 開始-終了[,…]] [--balance FILE]
  tm-simrun record [--player b] [--pause none] [--route far] [--save 300] [--seed 2] [--minutes 10]
                   [--balance FILE] --out PREFIX [--snapshot FILE]
  tm-simrun replay FILE [--until TICK] [--balance FILE]
  tm-simrun resume FILE [--minutes 30] [--player b] [--route far] [--save 300] [--sessions 90]
                   [--idle 開始-終了[,…]] [--balance FILE] [--out PREFIX] [--builds]";

const FLAGS: [&str; 3] = ["routes", "builds", "stop-at-goal"];

struct Args {
    cmd: String,
    file: Option<String>,
    opts: BTreeMap<String, String>,
    flags: Vec<String>,
}

impl Args {
    fn flag(&self, f: &str) -> bool {
        self.flags.iter().any(|x| x == f)
    }
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().ok_or("コマンドがない")?;
    let mut args = Args {
        cmd,
        file: None,
        opts: BTreeMap::new(),
        flags: Vec::new(),
    };
    while let Some(a) = it.next() {
        if let Some(k) = a.strip_prefix("--") {
            if FLAGS.contains(&k) {
                args.flags.push(k.to_string());
                continue;
            }
            let v = it.next().ok_or(format!("--{k} の値がない"))?;
            args.opts.insert(k.to_string(), v);
        } else if args.file.is_none() {
            args.file = Some(a);
        } else {
            return Err(format!("余分な引数 {a}"));
        }
    }
    Ok(args)
}

fn parse_seeds(s: &str) -> Result<Vec<u64>, String> {
    let num = |t: &str| {
        t.trim()
            .parse::<u64>()
            .map_err(|e| format!("シード {t}: {e}"))
    };
    if let Some((a, b)) = s.split_once("..=") {
        Ok((num(a)?..=num(b)?).collect())
    } else if let Some((a, b)) = s.split_once("..") {
        Ok((num(a)?..num(b)?).collect())
    } else {
        s.split(',').map(num).collect()
    }
}

fn players(a: &Args, default: &str) -> Result<Vec<Player>, String> {
    match a.opts.get("player").map_or(default, String::as_str) {
        "all" => Ok(Player::ALL.to_vec()),
        s => Player::parse(s)
            .map(|p| vec![p])
            .ok_or(format!("プレイヤー {s}")),
    }
}

fn pauses(a: &Args, default: &str) -> Result<Vec<Pause>, String> {
    match a.opts.get("pause").map_or(default, String::as_str) {
        "all" => Ok(Pause::ALL.to_vec()),
        s => Pause::parse(s).map(|p| vec![p]).ok_or(format!("停止 {s}")),
    }
}

fn routes(a: &Args) -> Result<Vec<Route>, String> {
    match a.opts.get("route").map_or("far", String::as_str) {
        "all" => Ok(Route::ALL.to_vec()),
        s => Route::parse(s)
            .map(|r| vec![r])
            .ok_or(format!("つなぎ方 {s}")),
    }
}

fn number<T: std::str::FromStr>(a: &Args, key: &str, default: T) -> Result<T, String> {
    a.opts.get(key).map_or(Ok(default), |v| {
        v.parse().map_err(|_| format!("--{key} {v}"))
    })
}

fn balance(a: &Args) -> Result<Arc<BalanceTable>, String> {
    match a.opts.get("balance") {
        None => Ok(Arc::new(BalanceTable::builtin())),
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            BalanceTable::from_ron_str(&text)
                .map(Arc::new)
                .map_err(|e| format!("{path}: {e:?}"))
        }
    }
}

/// `--save`、`--sessions`、`--idle`、`--stop-at-goal` を `base` に足す。
fn scenario(a: &Args, base: Scenario) -> Result<Scenario, String> {
    let idle = match a.opts.get("idle") {
        None => Vec::new(),
        Some(s) => Idle::parse_minutes(s).ok_or(format!("--idle {s}"))?,
    };
    let session_s = a
        .opts
        .get("sessions")
        .map(|v| {
            v.parse::<u32>()
                .ok()
                .and_then(|m| m.checked_mul(60))
                .filter(|&s| s > 0)
                .ok_or(format!("--sessions {v}"))
        })
        .transpose()?;
    Ok(Scenario {
        save_s: number(a, "save", SAVE_S)?,
        idle,
        session_s,
        stop_at_goal: a.flag("stop-at-goal"),
        ..base
    })
}

fn print_reports(a: &Args, bal: &BalanceTable, reports: &[Report]) {
    println!("{}", csv_header(bal.spec().company.max_level));
    let mut nodes: BTreeMap<u32, u32> = BTreeMap::new();
    for r in reports {
        println!("{}", r.csv_row());
        if let Some(g) = &r.game {
            for (&n, &c) in &g.node_counts {
                *nodes.entry(n).or_default() += c;
            }
            if let Some(m) = &g.first_mismatch {
                eprintln!(
                    "おすすめと食い違い: seed {:?} {}: {m}",
                    r.seed,
                    r.player.name()
                );
            }
        }
    }
    if a.flag("routes") {
        eprintln!("node_count,recomputes");
        for (n, c) in nodes {
            eprintln!("{n},{c}");
        }
    }
    if a.flag("builds") {
        eprintln!("{BUILDS_HEADER}");
        for row in reports.iter().flat_map(Report::build_rows) {
            eprintln!("{row}");
        }
    }
}

fn cmd_run(a: &Args) -> Result<(), String> {
    let bal = balance(a)?;
    let seeds = parse_seeds(a.opts.get("seeds").map_or("1..=3", String::as_str))?;
    let minutes = number(a, "minutes", 30u32)?;
    let mut jobs = Vec::new();
    for p in players(a, "all")? {
        for pause in pauses(a, "all")? {
            for route in routes(a)? {
                for &s in &seeds {
                    let sc = scenario(a, Scenario::new(p, pause).with_route(route))?;
                    jobs.push((s, sc));
                }
            }
        }
    }
    let reports = run_many(&jobs, &bal, minutes);
    print_reports(a, &bal, &reports);
    Ok(())
}

fn cmd_flow(a: &Args) -> Result<(), String> {
    let bal = balance(a)?;
    let minutes = number(a, "minutes", 40u32)?;
    println!("{}", csv_header(bal.spec().company.max_level));
    for p in players(a, "all")? {
        for pause in pauses(a, "all")? {
            let sc = scenario(a, Scenario::new(p, pause))?;
            let r = FlowSim::new(&bal, sc).run_for(minutes);
            println!("{}", r.csv_row());
        }
    }
    Ok(())
}

fn one_player(a: &Args) -> Result<Player, String> {
    Ok(players(a, "b")?[0])
}

fn one_route(a: &Args) -> Result<Route, String> {
    let r = routes(a)?;
    if r.len() == 1 {
        Ok(r[0])
    } else {
        Err("--route は far か near".into())
    }
}

fn write(path: &str, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))
}

fn write_recording(runner: &Runner, out: &str) -> Result<(), String> {
    let rec = runner.recording().expect("記録している");
    write(&format!("{out}.ron"), &rec.ron_text())?;
    write(&format!("{out}.events"), &rec.events_text())?;
    eprintln!(
        "{out}.ron: {} コマンド、{} チェックポイント、最後の state_hash {:016x}",
        rec.log.entries.len(),
        rec.log.checkpoints.len(),
        runner.game().state_hash()
    );
    Ok(())
}

/// `--snapshot` は最後の tick の行動を選ぶ前の写し。その tick に積む行動は入らないので、`resume` は元のプレイと分かれうる。
fn cmd_record(a: &Args) -> Result<(), String> {
    let bal = balance(a)?;
    let pause = pauses(a, "none")?[0];
    let seed = number(a, "seed", 2u64)?;
    let minutes = number(a, "minutes", 10u32)?;
    let out = a.opts.get("out").ok_or("--out がない")?;
    let sc = scenario(
        a,
        Scenario::new(one_player(a)?, pause).with_route(one_route(a)?),
    )?;
    let ticks = u64::from(minutes) * 60 * TPS;
    let mut runner = Runner::new(seed, bal, sc);
    runner.record();
    let mut snap = None;
    while runner.game().now().0 < ticks {
        runner.tick_observing(|g| {
            if g.now().0 == ticks {
                snap = Some(g.snapshot());
            }
        });
    }
    write_recording(&runner, out)?;
    if let (Some(path), Some(snap)) = (a.opts.get("snapshot"), snap) {
        let pretty = ron::ser::PrettyConfig::new().depth_limit(2);
        let text = ron::ser::to_string_pretty(&snap, pretty).map_err(|e| e.to_string())?;
        write(path, &(text + "\n"))?;
    }
    Ok(())
}

fn cmd_replay(a: &Args) -> Result<(), String> {
    let bal = balance(a)?;
    let path = a.file.as_ref().ok_or("ログのファイルがない")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let log = ReplayLog::from_ron(&text).map_err(|e| format!("{e:?}"))?;
    let start = log.start.as_ref().map_or(0, |s| s.tick);
    let last = log
        .checkpoints
        .last()
        .map_or(start, |c| c.tick.0.max(start));
    let until = Tick(number(a, "until", last)?);
    let g = replay(&log, bal, until, &mut Vec::new()).map_err(|e| format!("{e:?}"))?;
    println!("{} {:016x}", g.now().0, g.state_hash());
    Ok(())
}

fn cmd_resume(a: &Args) -> Result<(), String> {
    let bal = balance(a)?;
    let path = a.file.as_ref().ok_or("セーブのファイルがない")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let loaded = read_snapshot(&text)?;
    for w in &loaded.warnings {
        eprintln!("tm-simrun: 警告: {path}: {w}");
    }
    let game = Game::restore(Arc::clone(&bal), &loaded.snapshot).map_err(|e| format!("{e:?}"))?;
    let minutes = number(a, "minutes", 30u32)?;
    let sc = scenario(
        a,
        Scenario::new(one_player(a)?, Pause::NONE).with_route(one_route(a)?),
    )?;
    let mut r = Runner::resume(game, sc);
    if a.opts.contains_key("out") {
        r.record();
    }
    r.advance_until(r.wall() + u64::from(minutes) * 60 * TPS);
    if let Some(out) = a.opts.get("out") {
        write_recording(&r, out)?;
    }
    print_reports(a, &bal, &[r.report()]);
    eprintln!("{} {:016x}", r.game().now().0, r.game().state_hash());
    Ok(())
}

fn main() -> ExitCode {
    let result = parse_args().and_then(|a| match a.cmd.as_str() {
        "run" => cmd_run(&a),
        "flow" => cmd_flow(&a),
        "record" => cmd_record(&a),
        "replay" => cmd_replay(&a),
        "resume" => cmd_resume(&a),
        c => Err(format!("知らないコマンド {c}")),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tm-simrun: {e}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
