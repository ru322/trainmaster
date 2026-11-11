//! 長時間テスト(11 §11.4)の検査: 要素数の上限、人数の保存、「減らせない」、メモリが増え続けないこと。

use tm_core::{
    BalanceTable, CompanyLevel, Game, GameView, MonotonicSnapshot, ParamKey, PaxLedgerView,
    check_monotonic,
};

/// 状態の要素数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Census {
    /// 駅。
    pub stations: usize,
    /// 路線。
    pub lines: usize,
    /// 列車。
    pub trains: usize,
}

impl Census {
    /// いまの状態の要素数。
    pub fn of(view: &GameView<'_>) -> Self {
        Census {
            stations: view.stations().len(),
            lines: view.lines().len(),
            trains: view.trains().len(),
        }
    }
}

/// 表から決まる要素数の上限(01 では駅 27、路線 6、列車 48)。
pub fn census_limit(bal: &BalanceTable) -> Census {
    let max_level = CompanyLevel(bal.spec().company.max_level);
    let per_line = bal.max_trains(bal.limit(ParamKey::R1MaxTrains, max_level)) as usize;
    let lines = bal.max_lines() as usize;
    Census {
        stations: bal.max_stations() as usize,
        lines,
        trains: lines * per_line,
    }
}

/// 要素数が上限を超えていないか。
pub fn check_census(c: Census, limit: Census) -> Result<(), String> {
    if c.stations > limit.stations || c.lines > limit.lines || c.trains > limit.trains {
        return Err(format!("要素数 {c:?} が上限 {limit:?} を超えた"));
    }
    Ok(())
}

/// 人数の保存の等式(11 §6.5)。
pub fn check_ledger(l: &PaxLedgerView) -> Result<(), String> {
    if l.is_conserved() {
        Ok(())
    } else {
        Err(format!("人数の保存が破れた: {l:?}"))
    }
}

/// 毎 tick の不変条件が `step` の中で検査されるビルドか(`debug_assertions`。11 §9.2)。
pub const STEP_CHECKS_INVARIANTS: bool = cfg!(debug_assertions);

/// 1 回分の検査: 不変条件(`step` が検査しないビルドならここで)、要素数、保存、前回からの「減らせない」。
pub struct Checker {
    limit: Census,
    prev: MonotonicSnapshot,
    /// 検査した回数。
    pub checks: u64,
}

impl Checker {
    /// `game` のいまの状態から始める。
    pub fn new(game: &Game) -> Self {
        Checker {
            limit: census_limit(&game.config().balance),
            prev: game.monotonic_snapshot(),
            checks: 0,
        }
    }

    /// `game` を検査し、「減らせない」の比較の基準を更新する。
    pub fn check(&mut self, game: &Game) -> Result<(), String> {
        if !STEP_CHECKS_INVARIANTS {
            game.check_invariants()
                .map_err(|v| format!("{}: {}", v.id, v.detail))?;
        }
        self.check_values(
            Census::of(&game.view()),
            &game.pax_ledger(),
            game.monotonic_snapshot(),
        )
    }

    /// 取り出した値の検査(要素数、保存、「減らせない」)。通れば `cur` を次の比較の基準にする。
    pub fn check_values(
        &mut self,
        census: Census,
        ledger: &PaxLedgerView,
        cur: MonotonicSnapshot,
    ) -> Result<(), String> {
        check_census(census, self.limit)?;
        check_ledger(ledger)?;
        check_monotonic(&self.prev, &cur).map_err(|v| format!("{}: {}", v.id, v.detail))?;
        self.prev = cur;
        self.checks += 1;
        Ok(())
    }
}

/// いまのプロセスの常駐メモリ(KiB)。Linux の `/proc/self/status` の `VmRSS` を読む。読めなければ `None`。
pub fn rss_kib() -> Option<u64> {
    parse_vm_rss(&std::fs::read_to_string("/proc/self/status").ok()?)
}

/// `/proc/self/status` の本文から `VmRSS:  <n> kB` の n を取り出す。
pub fn parse_vm_rss(status: &str) -> Option<u64> {
    let line = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?;
    let mut it = line.split_whitespace();
    let kib = it.next()?.parse().ok()?;
    (it.next() == Some("kB")).then_some(kib)
}

/// 慣らしの後の標本(KiB)が、最初の標本から `tolerance_kib` より多く増えていないか。
pub fn check_growth(samples: &[u64], tolerance_kib: u64) -> Result<(), String> {
    let Some(&first) = samples.first() else {
        return Ok(());
    };
    let peak = samples.iter().copied().max().unwrap_or(first);
    if peak > first + tolerance_kib {
        return Err(format!(
            "メモリが {first} KiB から {peak} KiB に増えた(許容 {tolerance_kib} KiB): {samples:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
