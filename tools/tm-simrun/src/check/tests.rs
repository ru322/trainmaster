use super::*;

#[test]
fn builtin_limits_are_27_6_48() {
    let limit = census_limit(&BalanceTable::builtin());
    assert_eq!(
        limit,
        Census {
            stations: 27,
            lines: 6,
            trains: 48
        }
    );
    assert!(check_census(limit, limit).is_ok());
    for over in [
        Census {
            stations: 28,
            ..limit
        },
        Census { lines: 7, ..limit },
        Census {
            trains: 49,
            ..limit
        },
    ] {
        assert!(check_census(over, limit).is_err(), "{over:?}");
    }
}

#[test]
fn ledger_check_detects_lost_people() {
    let mut l = PaxLedgerView {
        spawned: 100,
        capacity_loss: 9,
        delivered: 70,
        waiting_cap_loss: 5,
        waiting: 20,
        onboard: 5,
    };
    assert!(check_ledger(&l).is_ok());
    l.onboard -= 1;
    assert!(check_ledger(&l).is_err());
}

#[test]
fn growth_check_allows_noise_but_not_leaks() {
    assert!(check_growth(&[], 10).is_ok());
    assert!(check_growth(&[1000, 1008, 1004, 1010], 10).is_ok());
    assert!(check_growth(&[1000, 1004, 1011], 10).is_err());
}

#[test]
fn rss_is_readable_on_linux() {
    if cfg!(target_os = "linux") {
        assert!(rss_kib().is_some_and(|k| k > 0));
    }
}

#[test]
fn checker_detects_going_back() {
    let bal = std::sync::Arc::new(BalanceTable::builtin());
    let mut g = Game::new(tm_core::GameConfig {
        seed: 1,
        balance: bal,
    });
    let before = g.clone();
    g.enqueue(tm_core::Command::CreateLine {
        stations: vec![tm_core::StationId(0), tm_core::StationId(1)],
        bends: vec![tm_core::Bend::StraightFirst],
    });
    for _ in 0..120 {
        g.step(&mut Vec::new());
    }
    let mut forward = Checker::new(&before);
    assert!(forward.check(&g).is_ok());
    assert_eq!(forward.checks, 1);
    let mut back = Checker::new(&g);
    assert!(back.check(&before).is_err());
}

#[test]
fn vm_rss_is_parsed_in_kib() {
    let status = "Name:\ttm\nVmPeak:\t  9000 kB\nVmRSS:\t   4120 kB\nThreads:\t1\n";
    assert_eq!(parse_vm_rss(status), Some(4120));
    assert_eq!(parse_vm_rss("VmRSS:\t12 MB\n"), None);
    assert_eq!(parse_vm_rss("VmSize:\t12 kB\n"), None);
}

#[test]
fn checker_detects_broken_ledger_midway() {
    let bal = std::sync::Arc::new(BalanceTable::builtin());
    let g = Game::new(tm_core::GameConfig {
        seed: 1,
        balance: bal,
    });
    let mut c = Checker::new(&g);
    let census = Census::of(&g.view());
    let mut ledger = g.pax_ledger();
    assert!(
        c.check_values(census, &ledger, g.monotonic_snapshot())
            .is_ok()
    );
    ledger.spawned += 1;
    let err = c.check_values(census, &ledger, g.monotonic_snapshot());
    assert!(err.is_err_and(|e| e.contains("保存")));
    assert_eq!(c.checks, 1);
}
