//! `GameView` と `Game` の公開 API(10 §7)。

#[allow(dead_code, reason = "テストのファイルごとに使う補助が違う")]
mod common;

use common::{builtin, cheap, create_line, driver_full_build, new_game, rich};
use tm_core::{
    Bend, BuyMode, Cars, Command, CommandError, CompanyLevel, Dir8, Feature, GameEvent, GridPos,
    Level, LineEnd, LineId, MilliPeople, Money, POS_UNITS_PER_TILE, ParamKey, People, Permille,
    SpeedMilli, StationId, TICKS_PER_SEC, Tick, TrainId, TrainPhaseView, UpgradeScope,
    spawn_per_sec,
};

fn sid(i: u32) -> StationId {
    StationId(i)
}

fn steps(g: &mut tm_core::Game, n: u64) -> Vec<GameEvent> {
    let mut ev = Vec::new();
    for _ in 0..n {
        g.step(&mut ev);
    }
    ev
}

#[test]
fn view_of_a_new_game() {
    let table = builtin();
    let g = new_game(5, &table);
    let v = g.view();
    let map = &table.spec().map;
    assert_eq!(v.tick(), Tick(0));
    assert_eq!(v.money(), Money::ZERO);
    assert_eq!(v.total_earned(), Money::ZERO);
    assert_eq!(v.income_per_sec(), Money::ZERO);
    let c = v.company();
    assert_eq!(
        (c.level, c.max_level, c.cap, c.station_count),
        (CompanyLevel(0), CompanyLevel(24), 4, 3)
    );
    assert_eq!(
        (
            c.gauge.sum_level,
            c.gauge.sum_limit,
            c.gauge.display_percent
        ),
        (0, 4 * 2 * 3, 0)
    );
    assert_eq!(v.line_slots(), (0, 1));
    assert!(v.is_unlocked(Feature::BulkBuy) && !v.is_unlocked(Feature::Transfer));
    assert_eq!(v.unlocked(), g.view().unlocked());
    assert!(v.is_new(ParamKey::S1Spawn) && !v.is_new(ParamKey::S3StationCap));
    assert_eq!(v.level_limit(ParamKey::T1Speed), Level(4));
    assert_eq!((v.max_money_unit(), v.goal_reached()), (0, false));
    assert_eq!(v.map_size(), (map.width, map.height));
    assert_eq!(v.map_revision(), 0);
    assert_eq!(v.stations().len(), 3);
    assert_eq!(v.lines().len(), 0);
    assert_eq!(v.trains().len(), 0);
    assert!(v.track_grid().masks.iter().all(|m| m.is_empty()));
    assert_eq!(
        v.track_grid().masks.len(),
        usize::from(map.width) * usize::from(map.height)
    );
    assert_eq!(v.recommendation(), None);
    let s = v.station(sid(1)).unwrap();
    assert_eq!(v.station_at(s.pos), Some(sid(1)));
    assert_eq!(v.station_at(GridPos { x: 0, y: 0 }), None);
    assert!(v.station(sid(3)).is_none());
    assert!(!s.connected);
    assert_eq!(s.spawn_per_sec, MilliPeople::ZERO);
}

#[test]
fn line_slot_level_follows_r2_and_resets_with_a_new_game() {
    let table = cheap();
    let mut g = new_game(3, &table);
    let mut seen = vec![g.view().line_slot_level()];
    let mut ev = Vec::new();
    while g.now().0 < 7_200 {
        let one = Command::LevelUp {
            param: ParamKey::R2LineSlots,
            scope: UpgradeScope::Company,
            mode: BuyMode::One,
        };
        for cmd in driver_full_build(&g.view()) {
            if !matches!(
                cmd,
                Command::LevelUp {
                    param: ParamKey::R2LineSlots,
                    ..
                }
            ) {
                g.enqueue(cmd);
            }
        }
        if g.validate(&one).is_ok() {
            g.enqueue(one);
        }
        ev.clear();
        g.step(&mut ev);
        let v = g.view();
        let level = v.line_slot_level();
        assert_eq!(v.balance().line_slots(level), v.line_slots().1);
        if seen.last() != Some(&level) {
            seen.push(level);
        }
    }
    let top = g.view().level_limit(ParamKey::R2LineSlots);
    assert_eq!(seen.first(), Some(&Level(0)));
    assert_eq!(seen.last(), Some(&top));
    assert!(seen.len() > 3, "{seen:?}");
    assert!(seen.windows(2).all(|w| w[1].0 == w[0].0 + 1));
    g = new_game(3, &table);
    assert_eq!(g.view().line_slot_level(), Level(0));
}

#[test]
fn view_after_building_a_line() {
    let mut g = new_game(5, &rich());
    let cmd = create_line(&g.view(), vec![sid(0), sid(1)]);
    g.enqueue(cmd);
    steps(&mut g, 1);
    let v = g.view();
    assert_eq!(v.line_slots(), (1, 1));
    assert_eq!(v.map_revision(), 1);
    let s = v.station(sid(0)).unwrap();
    assert!(s.connected);
    assert_eq!(s.lines, &[LineId(0)]);
    assert_eq!(
        (s.capacity, s.fare, s.platform),
        (People(10), Money::ONE, Cars(2))
    );
    assert_eq!(s.spawn_per_sec, MilliPeople(500));
    assert_eq!(v.station(sid(2)).unwrap().spawn_per_sec, MilliPeople::ZERO);
    let l = v.line(LineId(0)).unwrap();
    assert_eq!(l.stations, &[sid(0), sid(1)]);
    assert_eq!(l.stops, &[true, true]);
    assert_eq!(l.trains, &[TrainId(0)]);
    assert_eq!(
        (
            l.speed,
            l.cars,
            l.train_capacity,
            l.boarding_rate,
            l.attractiveness,
            l.min_platform
        ),
        (
            SpeedMilli(1000),
            Cars(1),
            People(10),
            People(5),
            Permille(1000),
            Cars(2)
        )
    );
    assert!(v.line(LineId(1)).is_none());
    assert_eq!(
        v.line_ends(LineId(0)),
        [(LineEnd::Head, sid(0)), (LineEnd::Tail, sid(1))]
    );
    let t = v.train(TrainId(0)).unwrap();
    assert!(
        matches!(t.phase, TrainPhaseView::Dwelling { at, remaining } if at == sid(0) && remaining.0 == 59)
    );
    let u = POS_UNITS_PER_TILE as i64;
    assert_eq!(
        (t.point.x, t.point.y),
        (i64::from(s.pos.x) * u, i64::from(s.pos.y) * u)
    );
    assert_eq!((t.riders, t.capacity), (People(0), People(10)));
    let c = v.company();
    assert_eq!((c.gauge.sum_level, c.gauge.sum_limit), (0, 4 * (2 * 3 + 2)));
    assert!(v.recommendation().is_some());
    assert!(!v.track_grid().masks.iter().all(|m| m.is_empty()));
}

#[test]
fn trains_move_along_the_track_by_their_speed() {
    let mut g = new_game(9, &builtin());
    let cmd = create_line(&g.view(), vec![sid(0), sid(1), sid(2)]);
    g.enqueue(cmd);
    steps(&mut g, 60);
    let u = POS_UNITS_PER_TILE as i64;
    let v_per_tick = 1000;
    let mut prev = g.view().train(TrainId(0)).unwrap();
    let mut moved = 0;
    for _ in 0..2000 {
        steps(&mut g, 1);
        let now = g.view().train(TrainId(0)).unwrap();
        let d = (now.point.x - prev.point.x)
            .abs()
            .max((now.point.y - prev.point.y).abs());
        match (prev.phase, now.phase) {
            (TrainPhaseView::Running { .. }, TrainPhaseView::Running { .. }) => {
                assert_eq!(d, v_per_tick, "{prev:?} → {now:?}");
                if prev.heading == now.heading {
                    let (dx, dy) = heading_delta(prev.heading);
                    let step = (
                        (now.point.x - prev.point.x).signum(),
                        (now.point.y - prev.point.y).signum(),
                    );
                    assert_eq!(step, (dx, dy), "{prev:?} → {now:?}");
                }
                moved += 1;
            }
            (_, TrainPhaseView::Dwelling { at, .. }) => {
                let p = g.view().station(at).unwrap().pos;
                assert_eq!(
                    (now.point.x, now.point.y),
                    (i64::from(p.x) * u, i64::from(p.y) * u)
                );
            }
            _ => assert!(d <= v_per_tick),
        }
        prev = now;
    }
    assert!(moved > 1000);
}

fn heading_delta(d: Dir8) -> (i64, i64) {
    let (x, y) = d.delta();
    (i64::from(x), i64::from(y))
}

#[test]
fn validate_matches_the_view_and_does_not_change_state() {
    let g = new_game(5, &builtin());
    let cmd = Command::LevelUp {
        param: ParamKey::S1Spawn,
        scope: UpgradeScope::AllStations,
        mode: BuyMode::One,
    };
    let before = g.state_hash();
    assert_eq!(
        g.validate(&cmd),
        Err(CommandError::NoTarget {
            param: ParamKey::S1Spawn
        })
    );
    assert_eq!(g.view().validate(&cmd), g.validate(&cmd));
    let line = create_line(&g.view(), vec![sid(0), sid(1), sid(2)]);
    let p = g.validate(&line).unwrap();
    assert_eq!((p.steps, p.cost, p.new_pass_through), (0, Money::ZERO, 0));
    assert!(p.track_tiles >= 2);
    assert_eq!(g.state_hash(), before);
    assert!(g.pending().is_empty());
}

#[test]
fn line_draft_and_track_preview() {
    let mut g = new_game(5, &builtin());
    let v = g.view();
    assert_eq!(v.validate_line_draft(&[], sid(0)), Ok(()));
    assert_eq!(
        v.validate_line_draft(&[sid(0)], sid(0)),
        Err(CommandError::DuplicateStation(sid(0)))
    );
    assert_eq!(
        v.validate_line_draft(&[sid(0)], sid(7)),
        Err(CommandError::UnknownStation(sid(7)))
    );
    let (a, b) = (
        v.station(sid(0)).unwrap().pos,
        v.station(sid(1)).unwrap().pos,
    );
    let mut out = vec![GridPos { x: -1, y: -1 }];
    v.preview_track(sid(0), sid(1), Bend::StraightFirst, &mut out);
    assert_eq!((out[0], *out.last().unwrap()), (a, b));
    assert_eq!(out.len() as u32, a.chebyshev(b) + 1);
    v.preview_track(sid(0), sid(9), Bend::StraightFirst, &mut out);
    assert!(out.is_empty());
    assert_eq!(v.default_bend(sid(0), sid(1)), Bend::StraightFirst);
    let cmd = Command::CreateLine {
        stations: vec![sid(0), sid(1)],
        bends: vec![Bend::DiagonalFirst],
    };
    g.enqueue(cmd);
    steps(&mut g, 1);
    let v = g.view();
    assert_eq!(
        v.validate_line_draft(&[sid(2)], sid(1)),
        Err(CommandError::NoFreeLineSlot { used: 1, slots: 1 })
    );
    let mut diag = Vec::new();
    let mut straight = Vec::new();
    v.preview_track(sid(0), sid(1), Bend::DiagonalFirst, &mut diag);
    v.preview_track(sid(0), sid(1), Bend::StraightFirst, &mut straight);
    if diag != straight {
        assert_eq!(
            v.default_bend(sid(0), sid(1)),
            Bend::DiagonalFirst,
            "線路のある方"
        );
        assert_eq!(
            v.default_bend(sid(1), sid(0)),
            Bend::StraightFirst,
            "逆向きは反転"
        );
    }
}

#[test]
fn is_new_clears_after_buying_and_purchases_follow_previews() {
    let mut g = new_game(5, &rich());
    let line = create_line(&g.view(), vec![sid(0), sid(1)]);
    g.enqueue(line);
    steps(&mut g, 1);
    let cmd = Command::LevelUp {
        param: ParamKey::S2Fare,
        scope: UpgradeScope::AllStations,
        mode: BuyMode::Max,
    };
    let p = g.validate(&cmd).unwrap();
    g.enqueue(cmd);
    let ev = steps(&mut g, 1);
    assert_eq!(
        ev[0],
        GameEvent::Purchased {
            param: ParamKey::S2Fare,
            scope: UpgradeScope::AllStations,
            mode: BuyMode::Max,
            steps: p.steps,
            raised: p.raised,
            cost: p.cost
        }
    );
    assert!(!g.view().is_new(ParamKey::S2Fare));
    assert!(g.view().is_new(ParamKey::S1Spawn));
}

#[test]
fn income_per_sec_is_the_average_of_the_last_ten_seconds() {
    let mut g = new_game(9, &builtin());
    let cmd = create_line(&g.view(), vec![sid(0), sid(1), sid(2)]);
    g.enqueue(cmd);
    let sec = u64::from(TICKS_PER_SEC);
    let mut income_at = vec![Money::ZERO];
    let mut seen_positive = false;
    for _ in 0..4000 {
        let earned = steps(&mut g, 1)
            .iter()
            .filter_map(|e| match e {
                GameEvent::TrainArrived { income, .. } => Some(*income),
                _ => None,
            })
            .fold(Money::ZERO, Money::saturating_add);
        income_at.push(earned);
        let t = g.now().0;
        let b = t - t % sec;
        let window = income_at[b.saturating_sub(10 * sec) as usize + 1..=b as usize]
            .iter()
            .fold(Money::ZERO, |acc, &m| acc.saturating_add(m));
        assert_eq!(g.view().income_per_sec(), window.div_small(10), "tick {t}");
        seen_positive |= !window.is_zero() && t % sec != 0;
    }
    assert!(seen_positive, "その秒の収入と違う値を見る tick がある");
}

#[test]
fn station_spawn_uses_the_attractiveness_of_stopping_lines() {
    let g = common::full_build(1);
    let v = g.view();
    let a = v
        .lines()
        .filter(|l| l.stops.iter().all(|&s| s))
        .map(|l| l.attractiveness)
        .max()
        .unwrap();
    assert!(a > Permille(1000));
    for s in v.stations() {
        let want = spawn_per_sec(v.balance().spawn_rate(s.levels.spawn), a);
        assert_eq!(s.spawn_per_sec, want, "{:?}", s.id);
    }
}
