//! 線路の値段(10 TD-41、TD-42、11 §4.8、§11.1)。

use super::*;
use crate::ids::CompanyLevel;
use crate::sim::economy::fixtures::{bal, custom, sid, world, yen};
use crate::state::{TrackGrid, lay_track};

const S: Bend = Bend::StraightFirst;
const D: Bend = Bend::DiagonalFirst;

fn create(ids: &[u32], bends: &[Bend]) -> Command {
    Command::CreateLine {
        stations: ids.iter().map(|&i| sid(i)).collect(),
        bends: bends.to_vec(),
    }
}

fn extend(l: u32, end: LineEnd, s: u32, bend: Bend) -> Command {
    Command::ExtendLine {
        line: LineId(l),
        end,
        station: sid(s),
        bend,
    }
}

const CELLS: [(i32, i32, u8); 8] = [
    (2, 5, 0),
    (10, 5, 0),
    (6, 5, 0),
    (6, 1, 0),
    (6, 9, 0),
    (14, 5, 0),
    (2, 8, 0),
    (8, 5, 0),
];

/// L0 = S0 (2,5) → S1 (10,5) のまっすぐな 9 マス。会社 Lv `c`、路線の枠 2、所持金は十分。
fn line_world(b: &BalanceTable, c: u8) -> GameState {
    let mut st = world(b, &CELLS, &[(&[0, 1], 0)], c);
    st.company.line_slots = Level(1);
    st.money = Money::pow10(60);
    st
}

fn preview(st: &GameState, b: &BalanceTable, cmd: &Command) -> CommandPreview {
    check_command(st, b, cmd).expect("通る").preview()
}

fn new_tiles(st: &GameState, b: &BalanceTable, cmd: &Command) -> u32 {
    preview(st, b, cmd).new_track_tiles
}

fn apply(st: &mut GameState, b: &BalanceTable, cmd: &Command) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    apply_command(st, b, cmd, &mut SimScratch::default(), &mut ev);
    ev
}

#[test]
fn counts_only_tiles_without_track() {
    let b = bal();
    let st = line_world(b, 1);
    assert_eq!(st.company.track_tiles, 9);
    assert_eq!(new_tiles(&st, b, &extend(0, LineEnd::Tail, 5, S)), 4);
    assert_eq!(
        new_tiles(&st, b, &create(&[3, 4], &[S])),
        8,
        "交差するマス (6,5) はただ"
    );
    assert_eq!(new_tiles(&st, b, &create(&[6, 7], &[S])), 6);
    assert_eq!(
        new_tiles(&st, b, &create(&[6, 7], &[D])),
        3,
        "曲がり方で重なりが変わる"
    );
    let p = preview(&st, b, &create(&[6, 7], &[D]));
    assert_eq!(p.cost, b.track_cost(CompanyLevel(1), 9, 3));
    assert_eq!(p.track_tiles, 6);
    assert!(
        p.cost
            .total_cmp(&preview(&st, b, &create(&[6, 7], &[S])).cost)
            == Ordering::Less
    );
}

#[test]
fn slot_is_checked_before_money() {
    let b = bal();
    let st = world(b, &CELLS, &[(&[0, 1], 0)], 1);
    assert!(st.money.is_zero());
    assert_eq!(
        check_command(&st, b, &create(&[3, 4], &[S])),
        Err(CommandError::NoFreeLineSlot { used: 1, slots: 1 })
    );
}

#[test]
fn already_laid_tiles_are_free() {
    let b = bal();
    let st = line_world(b, 3);
    for end in [LineEnd::Tail, LineEnd::Head] {
        let p = preview(&st, b, &extend(0, end, 2, D));
        assert_eq!((p.new_track_tiles, p.cost), (0, Money::ZERO), "{end:?}");
        assert_eq!(p.track_tiles, 4);
    }
}

#[test]
fn shared_tiles_in_one_command_count_once() {
    let b = bal();
    let cells = [(3, 8, 0), (9, 8, 0), (6, 11, 0), (6, 6, 0)];
    let st = world(b, &cells, &[], 1);
    let cmd = create(&[0, 1, 2, 3], &[S, S, S]);
    let p = check_command(&st, b, &cmd).expect_err("お金が足りない");
    let want = b.track_cost(CompanyLevel(1), 0, 7 + 3 + 4);
    assert_eq!(
        p,
        CommandError::InsufficientFunds {
            required: want,
            available: Money::ZERO
        }
    );
}

#[test]
fn head_and_tail_extensions_cost_the_same() {
    let b = bal();
    let fwd = line_world(b, 2);
    let mut rev = world(b, &CELLS, &[(&[1, 0], 0)], 2);
    rev.money = fwd.money;
    for s in [3, 4, 6] {
        for bend in [S, D] {
            let h = preview(&fwd, b, &extend(0, LineEnd::Head, s, bend));
            let t = preview(&rev, b, &extend(0, LineEnd::Tail, s, bend));
            assert_eq!(h, t, "S{s} {bend:?}");
            let mut tiles = Vec::new();
            lay_track(
                fwd.stations[0].pos,
                fwd.stations[s as usize].pos,
                bend,
                &mut tiles,
            );
            let k = tiles
                .iter()
                .filter(|&&p| fwd.tracks.get(p).is_empty())
                .count();
            assert_eq!(h.new_track_tiles as usize, k);
        }
    }
}

#[test]
fn company_level_0_is_free_but_counts_tiles() {
    let b = bal();
    let mut st = line_world(b, 0);
    st.money = Money::ZERO;
    let cmd = create(&[3, 4], &[S]);
    let p = preview(&st, b, &cmd);
    assert_eq!((p.cost, p.new_track_tiles), (Money::ZERO, 8));
    let ev = apply(&mut st, b, &cmd);
    assert_eq!(
        ev[0],
        GameEvent::LineCreated {
            line: LineId(1),
            station_count: 2,
            cost: Money::ZERO,
            new_tiles: 8
        }
    );
    assert_eq!(st.company.track_tiles, 17);
    assert_eq!(st.tracks.occupied(), 17);
    assert_eq!(st.money, Money::ZERO);
}

#[test]
fn insufficient_funds_reports_the_price_and_changes_nothing() {
    let b = bal();
    let mut st = line_world(b, 1);
    let cmd = extend(0, LineEnd::Tail, 5, S);
    let price = b.track_cost(CompanyLevel(1), 9, 4);
    let below = price.checked_sub(yen(1)).unwrap();
    st.money = below;
    assert_eq!(
        check_command(&st, b, &cmd),
        Err(CommandError::InsufficientFunds {
            required: price,
            available: below
        })
    );
    let before = st.clone();
    let ev = apply(&mut st, b, &cmd);
    assert!(matches!(
        ev.as_slice(),
        [GameEvent::CommandRejected {
            reason: CommandError::InsufficientFunds { .. },
            ..
        }]
    ));
    assert_eq!(st, before);
    st.money = price;
    assert!(check_command(&st, b, &cmd).is_ok(), "ちょうどなら買える");
    apply(&mut st, b, &cmd);
    assert_eq!(st.money, Money::ZERO);
}

#[test]
fn second_build_in_the_same_tick_is_rechecked() {
    let b = bal();
    let mut st = line_world(b, 1);
    let first = extend(0, LineEnd::Tail, 5, S);
    let second = create(&[3, 4], &[S]);
    let p1 = preview(&st, b, &first).cost;
    st.money = p1.saturating_add(preview(&st, b, &second).cost);
    let mut sc = SimScratch::default();
    let mut ev = Vec::new();
    apply_command(&mut st, b, &first, &mut sc, &mut ev);
    let left = st.money;
    let p2 = b.track_cost(CompanyLevel(1), st.company.track_tiles, 8);
    assert!(
        left.total_cmp(&p2) == Ordering::Less,
        "N が増えて高くなった"
    );
    apply_command(&mut st, b, &second, &mut sc, &mut ev);
    assert_eq!(
        ev.last(),
        Some(&GameEvent::CommandRejected {
            kind: crate::command::CommandKind::CreateLine,
            reason: CommandError::InsufficientFunds {
                required: p2,
                available: left
            }
        })
    );
    assert_eq!(st.lines.len(), 1);
}

#[test]
fn applying_pays_the_validated_price() {
    let b = bal();
    for (cmd, c) in [
        (extend(0, LineEnd::Tail, 5, S), 1),
        (extend(0, LineEnd::Head, 6, D), 4),
        (extend(0, LineEnd::Head, 2, S), 4),
        (create(&[3, 4, 6], &[S, D]), 7),
        (create(&[6, 7, 3], &[D, S]), 2),
    ] {
        let mut st = line_world(b, c);
        let money = st.money;
        let n = st.company.track_tiles;
        let p = preview(&st, b, &cmd);
        assert_eq!(p.cost, b.track_cost(CompanyLevel(c), n, p.new_track_tiles));
        let ev = apply(&mut st, b, &cmd);
        let (cost, new) = match ev[0] {
            GameEvent::LineCreated {
                cost, new_tiles, ..
            }
            | GameEvent::LineExtended {
                cost, new_tiles, ..
            } => (cost, new_tiles),
            e => panic!("{e:?}"),
        };
        assert_eq!((cost, new), (p.cost, p.new_track_tiles), "{cmd:?}");
        assert_eq!(st.money, money.checked_sub(cost).unwrap());
        assert_eq!(st.company.track_tiles, n + new);
        assert_eq!(st.tracks.occupied(), st.company.track_tiles, "INV-20");
    }
}

#[test]
fn price_stops_growing_past_tile_cap() {
    let capped = custom(|s| s.economy.track.tile_cap = 10);
    let cmd = extend(0, LineEnd::Tail, 5, S);
    let at = |b: &BalanceTable, n| {
        let mut st = line_world(b, 2);
        st.company.track_tiles = n;
        preview(&st, b, &cmd).cost
    };
    assert_eq!(at(&capped, 10), at(&capped, 50));
    assert_ne!(at(&capped, 9), at(&capped, 10));
    assert_ne!(at(bal(), 10), at(bal(), 50));
}

#[test]
fn line_draft_does_not_check_money() {
    let b = bal();
    let mut st = world(b, &CELLS, &[], 1);
    st.money = Money::ZERO;
    assert_eq!(check_line_draft(&st, b, &[sid(0)], sid(1)), Ok(()));
    assert!(matches!(
        check_command(&st, b, &create(&[0, 1], &[S])),
        Err(CommandError::InsufficientFunds { .. })
    ));
}

#[test]
fn count_new_tiles_dedups_with_tile_bits() {
    let b = bal();
    let grid = TrackGrid::new(b.spec().map.width, b.spec().map.height);
    let p = |x, y| crate::geom::GridPos { x, y };
    assert_eq!(count_new_tiles(&grid, [(p(0, 0), p(5, 0), S)]), 6);
    assert_eq!(
        count_new_tiles(&grid, [(p(0, 0), p(5, 0), S), (p(5, 0), p(0, 0), S)]),
        6
    );
    assert_eq!(
        count_new_tiles(&grid, [(p(0, 0), p(16, 16), D), (p(16, 0), p(0, 16), S)]),
        17 + 17 - 1,
        "(8,8) で交差"
    );
}

/// 会社 Lv × N × 新しいマスの値段のゴールデン(16 §2.7。`to_bits`)。表か式を変えたら更新する。
#[test]
fn track_price_golden() {
    let b = bal();
    let got: Vec<u64> = PRICE_POINTS
        .iter()
        .map(|&(c, n, k)| b.track_cost(CompanyLevel(c), n, k).to_bits())
        .collect();
    assert_eq!(got, PRICE_GOLDEN, "{got:#018x?}");
}

const PRICE_POINTS: [(u8, u32, u32); 9] = [
    (0, 0, 720),
    (1, 0, 1),
    (1, 14, 8),
    (4, 40, 7),
    (12, 90, 5),
    (18, 120, 6),
    (24, 150, 6),
    (24, 200, 1),
    (24, 710, 10),
];

const PRICE_GOLDEN: [u64; 9] = [
    0x0000_0000_0000_0000,
    0x4049_0000_0000_0000,
    0x40c4_afde_b224_2751,
    0x429b_9fe0_d139_5d1d,
    0x4733_a143_8f58_dd52,
    0x4a96_170a_f08a_ef8c,
    0x4df2_a111_4fc9_7ee7,
    0x4e90_ac40_92ad_6d61,
    0x4ec4_d750_b758_c8b8,
];
