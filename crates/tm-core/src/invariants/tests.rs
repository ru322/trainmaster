use std::sync::Arc;

use super::*;
use crate::command::{Command, LineEnd};
use crate::game::{Game, GameConfig};
use crate::geom::{Bend, GridPos};
use crate::ids::TrainId;
use crate::param::{BuyMode, UpgradeScope};
use crate::time::Ticks;
use crate::units::{MilliPeople, PosUnits};

fn rich() -> Arc<BalanceTable> {
    let mut spec = BalanceTable::builtin().spec().clone();
    spec.economy.initial_money_yen = 1_000_000_000;
    Arc::new(BalanceTable::from_spec(spec).expect("検証を通る表"))
}

/// 路線 L0 = S0–S1(S2 は路線に入っていない)、列車・乗客のある状態。
fn valid() -> Game {
    let mut g = Game::new(GameConfig {
        seed: 11,
        balance: rich(),
    });
    g.enqueue(Command::CreateLine {
        stations: vec![StationId(0), StationId(1)],
        bends: vec![Bend::DiagonalFirst],
    });
    g.enqueue(Command::LevelUp {
        param: ParamKey::S1Spawn,
        scope: UpgradeScope::AllStations,
        mode: BuyMode::Max,
    });
    let mut ev = Vec::new();
    for _ in 0..400 {
        g.step(&mut ev);
    }
    g.check_invariants().unwrap();
    assert!(g.state.stations[0].waiting_total.0 + g.state.stations[1].waiting_total.0 > 0);
    g
}

fn expect(id: &str, edit: impl FnOnce(&mut GameState)) {
    let g = valid();
    let mut st = g.state.clone();
    edit(&mut st);
    let got = check_all(&st, &g.config.balance).expect_err(id);
    assert_eq!(got.id, id, "{}", got.detail);
}

fn add_waiting(st: &mut GameState, s: u32, t: u32, n: u64) {
    st.waiting.add(s, StationId(t), People(n));
    st.stations[s as usize].waiting_total.0 += n;
}

#[test]
fn valid_state_passes() {
    let g = valid();
    assert_eq!(check_all(&g.state, &g.config.balance), Ok(()));
}

#[test]
fn inv01_station_count_and_plan() {
    expect("INV-01", |st| st.stations[1].pos = st.station_plan[5]);
    expect("INV-01", |st| {
        let mut s = st.stations[2].clone();
        s.id = StationId(3);
        s.pos = st.station_plan[3];
        st.stations.push(s);
        st.waiting.push_row();
    });
    expect("INV-01", |st| st.stations[2].id = StationId(7));
}

#[test]
fn inv02_positions() {
    expect("INV-02", |st| {
        st.station_plan[2] = GridPos { x: 0, y: 5 };
        st.stations[2].pos = GridPos { x: 0, y: 5 };
    });
    expect("INV-02", |st| {
        st.station_plan[2] = st.stations[0].pos;
        st.stations[2].pos = st.stations[0].pos;
    });
}

#[test]
fn inv03_line_slots() {
    expect("INV-03", |st| {
        let mut l = st.lines[0].clone();
        l.id = LineId(1);
        st.lines.push(l);
    });
}

#[test]
fn inv04_line_shape() {
    expect("INV-04", |st| st.lines[0].segments[0].tiles.reverse());
    expect("INV-04", |st| {
        let b = &mut st.lines[0].segments[0].bend;
        *b = b.reversed();
    });
    expect("INV-04", |st| st.lines[0].segments[0].to = StationId(2));
    expect("INV-04", |st| st.lines[0].stations[1] = StationId(0));
}

#[test]
fn inv05_trains() {
    expect("INV-05", |st| st.lines[0].trains.clear());
    expect("INV-05", |st| st.trains[0].id = TrainId(3));
    expect("INV-05", |st| {
        let mut t = st.trains[0];
        t.id = TrainId(1);
        st.trains.push(t);
        st.onboard.push_row();
    });
}

#[test]
fn inv06_levels() {
    expect("INV-06", |st| st.stations[0].levels.spawn = Level(5));
    expect("INV-06", |st| st.lines[0].levels.crowding = Level(1));
    expect("INV-06", |st| st.stations[2].levels.fare = Level(1));
    expect("INV-06", |st| st.company.line_slots = Level(5));
}

#[test]
fn inv08_caches_and_columns() {
    expect("INV-08", |st| st.stations[0].waiting_total.0 += 1);
    expect("INV-08", |st| st.trains[0].riders.0 += 1);
    expect("INV-08", |st| add_waiting(st, 0, 0, 1));
    expect("INV-08", |st| add_waiting(st, 0, 5, 1));
    expect("INV-08", |st| st.onboard.push_row());
}

#[test]
fn inv09_capacity() {
    expect("INV-09", |st| {
        st.onboard.add(0, StationId(1), People(1000));
        st.trains[0].riders.0 += 1000;
    });
}

#[test]
fn inv10_train_state() {
    expect("INV-10", |st| {
        st.trains[0].phase = TrainPhase::Running;
        st.trains[0].pos = st.lines[0].sections[0].length;
    });
    expect("INV-10", |st| st.trains[0].section = 3);
    expect("INV-10", |st| {
        st.trains[0].pos = PosUnits(0);
        st.trains[0].phase = TrainPhase::Dwelling {
            at: 0,
            remaining: Ticks(600),
        };
        st.trains[0].dir = crate::state::Direction::Forward;
    });
    expect("INV-10", |st| {
        st.trains[0].pos = PosUnits(0);
        st.trains[0].phase = TrainPhase::Dwelling {
            at: 1,
            remaining: Ticks(0),
        };
        st.trains[0].dir = crate::state::Direction::Forward;
    });
}

#[test]
fn inv11_spawn_acc() {
    expect("INV-11", |st| st.stations[0].spawn_acc = MilliPeople(1000));
}

#[test]
fn inv12_company_level_settled() {
    expect("INV-12", |st| {
        for s in &mut st.stations[..2] {
            s.levels.spawn = Level(4);
            s.levels.fare = Level(4);
        }
        st.lines[0].levels.speed = Level(4);
        st.lines[0].levels.car_capacity = Level(4);
    });
}

#[test]
fn inv13_derived_data_and_routes() {
    expect("INV-13", |st| {
        st.tracks.masks.iter_mut().for_each(|m| m.0 = 0)
    });
    expect("INV-13", |st| st.stations[2].serving_lines.push(LineId(0)));
    expect("INV-13", |st| {
        st.lines[0].sections[0].length = PosUnits(st.lines[0].sections[0].length.0 + 1)
    });
    expect("INV-13", |st| st.routing.mark_dirty());
    // 速度を変えて印を付け忘れた(12 RT-D-14)。
    expect("INV-13", |st| st.lines[0].levels.speed = Level(1));
}

#[test]
fn inv14_unlocked() {
    expect("INV-14", |st| {
        st.progress.unlocked.insert(Feature::Transfer)
    });
}

#[test]
fn inv15_money_unit_and_goal() {
    expect("INV-15", |st| st.money = Money::from_u64(1_000_000_000_000));
    expect("INV-15", |st| st.progress.goal_reached = true);
}

#[test]
fn inv16_stops() {
    expect("INV-16", |st| {
        st.lines[0].stops = vec![true, false];
        st.lines[0].sections.clear();
    });
}

#[test]
fn inv17_offsets() {
    expect("INV-17", |st| st.lines[0].offsets[1] += 1);
}

#[test]
fn inv18_reachable() {
    expect("INV-18", |st| add_waiting(st, 0, 2, 3));
}

#[test]
fn inv19_unconnected() {
    expect("INV-19", |st| st.stations[2].spawn_acc = MilliPeople(5));
}

#[test]
fn inv20_track_tiles() {
    let g = valid();
    assert!(g.state.company.track_tiles > 0);
    assert_eq!(g.state.company.track_tiles, g.state.tracks.occupied());
    expect("INV-20", |st| st.company.track_tiles += 1);
    expect("INV-20", |st| st.company.track_tiles -= 1);
}

#[test]
fn inv21_line_born_level() {
    let g = valid();
    assert_eq!(g.state.lines[0].born_level, g.state.company.level);
    expect("INV-21", |st| {
        st.lines[0].born_level = CompanyLevel(st.company.level.0 + 1)
    });
}

#[test]
fn inv21_born_level_does_not_decrease_by_line_id() {
    let mut g = valid();
    let mut second = g.state.lines[0].clone();
    second.id = crate::ids::LineId(1);
    g.state.lines[0].born_level = CompanyLevel(1);
    g.state.company.level = CompanyLevel(1);
    g.state.lines.push(second);
    assert_eq!(
        super::inv21_line_born_level(&g.state)
            .expect_err("減っている")
            .id,
        "INV-21"
    );
    g.state.lines[1].born_level = CompanyLevel(1);
    assert!(super::inv21_line_born_level(&g.state).is_ok());
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "INV-11")]
fn step_checks_invariants_in_debug_builds() {
    let mut g = valid();
    g.state.stations[2].spawn_acc = MilliPeople(1000);
    g.step(&mut Vec::new());
}

fn snap() -> MonotonicSnapshot {
    let mut g = valid();
    g.enqueue(Command::ExtendLine {
        line: LineId(0),
        end: LineEnd::Tail,
        station: StationId(2),
        bend: Bend::StraightFirst,
    });
    g.step(&mut Vec::new());
    g.monotonic_snapshot()
}

#[test]
fn monotonic_accepts_growth() {
    let a = snap();
    let mut b = a.clone();
    assert_eq!(check_monotonic(&a, &b), Ok(()));
    b.tick.0 += 1;
    b.line_stations[0].insert(0, StationId(5));
    b.line_stations[0].push(StationId(6));
    b.line_stations.push(vec![StationId(0), StationId(1)]);
    b.station_levels[0].fare.0 += 1;
    b.track_tiles += 3;
    b.total_earned = b.total_earned.saturating_add(Money::ONE);
    b.goal_reached = true;
    assert_eq!(check_monotonic(&a, &b), Ok(()));
}

#[test]
fn monotonic_rejects_every_decrease() {
    type Edit = fn(&mut MonotonicSnapshot);
    let edits: Vec<(&str, Edit)> = vec![
        ("tick", |s| s.tick.0 -= 1),
        ("company", |s| s.company_level.0 -= 1),
        ("stations", |s| s.station_count -= 1),
        ("lines", |s| {
            s.line_stations.pop();
        }),
        ("trains", |s| s.train_count -= 1),
        ("line_stations", |s| {
            s.line_stations[0].insert(1, StationId(9))
        }),
        ("line_stations.shrink", |s| {
            s.line_stations[0].pop();
        }),
        ("station_levels", |s| s.station_levels[0].spawn.0 -= 1),
        ("line_levels", |s| s.line_levels[0].car_capacity.0 -= 1),
        ("line_slots", |s| s.line_slots.0 -= 1),
        ("track_tiles", |s| s.track_tiles -= 1),
        ("unlocked", |s| s.unlocked = FeatureSet::default()),
        ("upgraded_once", |s| s.upgraded_once = ParamSet::default()),
        ("total_earned", |s| s.total_earned = Money::ZERO),
        ("max_money_unit", |s| s.max_money_unit -= 1),
        ("goal_reached", |s| s.goal_reached = false),
    ];
    let mut before = snap();
    before.company_level.0 = 1;
    before.line_slots.0 = 1;
    before.max_money_unit = 1;
    before.goal_reached = true;
    before.total_earned = Money::ONE;
    before.station_levels[0].spawn.0 = 1;
    before.line_levels[0].car_capacity.0 = 1;
    for (name, edit) in edits {
        let mut after = before.clone();
        edit(&mut after);
        let err = check_monotonic(&before, &after).expect_err(name);
        assert_eq!(err.id, "MONO", "{name}");
    }
}
