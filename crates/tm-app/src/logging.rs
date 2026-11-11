//! ロガー: 標準エラー出力と直近の行のリングバッファ [02 D19、16 §1.14、APP-D-07]。

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::Write as _;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use log::{LevelFilter, Log, Metadata, Record};

/// リングバッファに残す行数。
pub const RING_LINES: usize = 256;
/// リングバッファの 1 行の上限(バイト)。
pub const LINE_MAX_BYTES: usize = 160;

/// `log::Log` の実装。行を標準エラー出力に書き、直近の行を覚える。
#[derive(Debug)]
pub struct RingLogger {
    start: Instant,
    level: LevelFilter,
    echo: bool,
    ring: Mutex<VecDeque<String>>,
}

impl RingLogger {
    /// `level` より詳しい行は捨てる。`echo` なら標準エラー出力にも書く。
    pub fn new(level: LevelFilter, echo: bool) -> Self {
        RingLogger {
            start: Instant::now(),
            level,
            echo,
            ring: Mutex::new(VecDeque::with_capacity(RING_LINES)),
        }
    }

    /// 直近の `n` 行(古い順)。
    pub fn lines(&self, n: usize) -> Vec<String> {
        let ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        ring.iter()
            .skip(ring.len().saturating_sub(n))
            .cloned()
            .collect()
    }

    /// 覚えている行数。
    pub fn len(&self) -> usize {
        self.ring.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// 1 行も覚えていない。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// レベルによらず 1 行出す。
    pub fn always(&self, args: std::fmt::Arguments<'_>) {
        self.write(&always_record(args));
    }

    fn write(&self, record: &Record<'_>) {
        let t = self.start.elapsed();
        let mut line = String::new();
        let _ = write!(
            line,
            "[{:>6}.{:03}s {:<5} {}] {}",
            t.as_secs(),
            t.subsec_millis(),
            record.level(),
            record.target(),
            record.args()
        );
        if self.echo {
            let _ = writeln!(std::io::stderr().lock(), "{line}");
        }
        self.push(line);
    }

    fn push(&self, mut line: String) {
        truncate(&mut line, LINE_MAX_BYTES);
        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        if ring.len() == RING_LINES {
            ring.pop_front();
        }
        ring.push_back(line);
    }
}

fn truncate(s: &mut String, max: usize) {
    if s.len() > max {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
}

impl Log for RingLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            self.write(record);
        }
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

static LOGGER: OnceLock<RingLogger> = OnceLock::new();

/// プロセスのロガーを作り、パニックフックを入れる。2 回目以降は何もしない。
pub fn init(level: LevelFilter) {
    let mut first = false;
    let logger = LOGGER.get_or_init(|| {
        first = true;
        RingLogger::new(level, true)
    });
    if !first {
        return;
    }
    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("パニック: {info}");
        default(info);
    }));
}

/// レベルによらず出す行(起動時の版など)。`init` の前は標準エラー出力だけに書く。
pub fn always(args: std::fmt::Arguments<'_>) {
    match LOGGER.get() {
        Some(l) => l.always(args),
        None => eprintln!("{args}"),
    }
}

fn always_record(args: std::fmt::Arguments<'_>) -> Record<'_> {
    Record::builder()
        .level(log::Level::Info)
        .target("trainmaster")
        .args(args)
        .build()
}

/// `TM_LOG` のレベル。読めなければ `info`。
pub fn level_from_env() -> LevelFilter {
    std::env::var("TM_LOG")
        .ok()
        .and_then(|v| parse_level(&v))
        .unwrap_or(LevelFilter::Info)
}

/// `error` / `warn` / `info` / `debug` / `trace` を読む。
pub fn parse_level(s: &str) -> Option<LevelFilter> {
    match s.trim().to_ascii_lowercase().as_str() {
        "error" => Some(LevelFilter::Error),
        "warn" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

/// プロセスのロガーの直近の `n` 行(古い順)。`init` の前は空。
pub fn recent(n: usize) -> Vec<String> {
    LOGGER.get().map(|l| l.lines(n)).unwrap_or_default()
}
