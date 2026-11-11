//! 路線のコマンドの `validate`(線路の値段と新しいマスの数え方を含む)がメモリを確保しない(11 §1.1、§4.8、10 TD-42)。
//! 確保の回数はテストのバイナリの数えるアロケータで測る(16 APP-D-24 の例外)。

#[allow(dead_code, reason = "テストのファイルごとに使う補助が違う")]
mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use tm_core::{Bend, Command, LineEnd, LineId, StationId};

struct CountingAlloc;

thread_local! {
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
}

// SAFETY: `System` に委ねるだけで、この thread の回数を足す。
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn allocs_during(f: impl FnOnce()) -> u64 {
    let before = ALLOCS.with(Cell::get);
    f();
    ALLOCS.with(Cell::get) - before
}

#[test]
fn validating_line_commands_does_not_allocate() {
    let bal = common::rich();
    let fresh = common::new_game(1, &bal);
    let mut g = fresh.clone();
    g.enqueue(common::create_line(
        &g.view(),
        vec![StationId(0), StationId(1)],
    ));
    g.step(&mut Vec::new());
    let create = Command::CreateLine {
        stations: vec![StationId(2), StationId(0), StationId(1)],
        bends: vec![Bend::DiagonalFirst, Bend::StraightFirst],
    };
    let mut cmds = Vec::new();
    for end in [LineEnd::Head, LineEnd::Tail] {
        for bend in [Bend::StraightFirst, Bend::DiagonalFirst] {
            cmds.push(Command::ExtendLine {
                line: LineId(0),
                end,
                station: StationId(2),
                bend,
            });
        }
    }
    let mut results = Vec::with_capacity(cmds.len() + 1);
    let n = allocs_during(|| {
        results.push(fresh.validate(&create));
        for c in &cmds {
            results.push(g.validate(c));
        }
    });
    assert_eq!(n, 0, "{results:?}");
    assert!(
        results
            .iter()
            .all(|r| r.is_ok_and(|p| p.new_track_tiles > 0)),
        "{results:?}"
    );
}
