use super::*;
use crate::scenario::SAVE_S;

#[derive(Clone)]
struct Fake {
    money: f64,
    unconnected: Option<StationId>,
    connected: Option<StationId>,
    free: u32,
    sizes: Vec<usize>,
    steps: [Option<f64>; 12],
    bought: [bool; 12],
    base: f64,
    bumped: [f64; 12],
    connected_ids: Vec<StationId>,
    ends: Vec<[StationId; 2]>,
    pos: Vec<i32>,
    price: Option<f64>,
    earning: f64,
}

impl Fake {
    fn new() -> Self {
        Fake {
            money: 100.0,
            unconnected: None,
            connected: Some(StationId(0)),
            free: 0,
            sizes: vec![3],
            steps: [None; 12],
            bought: [true; 12],
            base: 10.0,
            bumped: [10.0; 12],
            connected_ids: vec![StationId(0)],
            ends: vec![[StationId(0), StationId(0)]],
            pos: Vec::new(),
            price: None,
            earning: 0.0,
        }
    }

    fn cand(mut self, p: ParamKey, step: f64, bumped: f64) -> Self {
        self.steps[p.index()] = Some(step);
        self.bumped[p.index()] = bumped;
        self
    }
}

impl World for Fake {
    fn money(&self) -> f64 {
        self.money
    }
    fn first_unconnected(&self) -> Option<StationId> {
        self.unconnected
    }
    fn first_connected(&self) -> Option<StationId> {
        self.connected
    }
    fn connected_stations(&self) -> Vec<StationId> {
        self.connected_ids.clone()
    }
    fn line_end_stations(&self) -> Vec<[StationId; 2]> {
        self.ends.clone()
    }
    fn distance(&self, a: StationId, b: StationId) -> u32 {
        self.pos[a.index()].abs_diff(self.pos[b.index()])
    }
    fn free_slots(&self) -> u32 {
        self.free
    }
    fn line_sizes(&self) -> Vec<usize> {
        self.sizes.clone()
    }
    fn step_cost(&self, p: ParamKey) -> Option<f64> {
        self.steps[p.index()]
    }
    fn upgraded_once(&self, p: ParamKey) -> bool {
        self.bought[p.index()]
    }
    fn income(&self, bump: Option<ParamKey>) -> f64 {
        bump.map_or(self.base, |p| self.bumped[p.index()])
    }
    fn build_price(&self, d: Decision) -> Option<f64> {
        self.price.filter(|_| d.is_build())
    }
    fn earning(&self) -> f64 {
        self.earning
    }
}

fn up(param: ParamKey, affordable: bool) -> Decision {
    Decision::Upgrade { param, affordable }
}

#[test]
fn intervals_follow_a4() {
    let got: Vec<u32> = Player::ALL.iter().map(|p| p.interval_s()).collect();
    assert_eq!(got, [2, 4, 8]);
    assert_eq!(Player::parse("c"), Some(Player::C));
    assert_eq!(Player::parse("d"), None);
}

#[test]
fn unconnected_station_creates_buys_slot_or_extends() {
    let mut w = Fake::new().cand(ParamKey::R2LineSlots, 50.0, 10.0);
    w.unconnected = Some(StationId(4));
    w.free = 1;
    let create = Decision::CreateLine {
        new: StationId(4),
        with: StationId(0),
    };
    assert_eq!(decide(Player::B, &w), create);
    w.free = 0;
    assert_eq!(decide(Player::A, &w), Decision::BuyLineSlots);
    w.money = 49.0;
    w.sizes = vec![5, 3, 4, 3];
    let extend = |line| Decision::Extend {
        line: LineId(line),
        station: StationId(4),
    };
    assert_eq!(decide(Player::C, &w), extend(1));
    w.sizes = vec![6, 7, 5];
    assert_eq!(decide(Player::B, &w), extend(2));
    w.steps[ParamKey::R2LineSlots.index()] = None;
    w.money = 1e9;
    assert_eq!(decide(Player::B, &w), extend(2));
}

#[test]
fn new_params_first_cheapest_then_table_order() {
    let mut w = Fake::new()
        .cand(ParamKey::S2Fare, 5.0, 11.0)
        .cand(ParamKey::T1Speed, 5.0, 99.0)
        .cand(ParamKey::R2LineSlots, 1.0, 10.0)
        .cand(ParamKey::T4CarCapacity, 200.0, 10.0);
    w.bought = [false; 12];
    for p in Player::ALL {
        assert_eq!(decide(p, &w), up(ParamKey::S2Fare, true), "{p:?}");
    }
    w.bought[ParamKey::S2Fare.index()] = true;
    assert_eq!(decide(Player::B, &w), up(ParamKey::T1Speed, true));
    w.bought[ParamKey::T1Speed.index()] = true;
    assert_eq!(decide(Player::B, &w), up(ParamKey::T1Speed, true));
}

#[test]
fn player_a_takes_cheapest_even_unaffordable() {
    let w = Fake::new()
        .cand(ParamKey::S1Spawn, 500.0, 99.0)
        .cand(ParamKey::T5Cars, 300.0, 10.0)
        .cand(ParamKey::T3Boarding, 300.0, 10.0);
    assert_eq!(decide(Player::A, &w), up(ParamKey::T3Boarding, false));
}

#[test]
fn players_b_c_take_best_value_with_ties() {
    let w = Fake::new()
        .cand(ParamKey::S1Spawn, 10.0, 30.0)
        .cand(ParamKey::S2Fare, 20.0, 50.0)
        .cand(ParamKey::T1Speed, 5.0, 12.0)
        .cand(ParamKey::R2LineSlots, 1.0, 10.0);
    assert_eq!(decide(Player::B, &w), up(ParamKey::S1Spawn, true));
    let w = w.cand(ParamKey::T4CarCapacity, 10.0, 30.0);
    assert_eq!(decide(Player::C, &w), up(ParamKey::S1Spawn, true));
    let w = w.cand(ParamKey::S3StationCap, 5.0, 20.0);
    assert_eq!(decide(Player::C, &w), up(ParamKey::S3StationCap, true));
}

#[test]
fn players_b_c_pool_is_affordable_unless_none() {
    let w = Fake::new()
        .cand(ParamKey::S1Spawn, 1000.0, 1e9)
        .cand(ParamKey::S2Fare, 50.0, 11.0);
    assert_eq!(decide(Player::B, &w), up(ParamKey::S2Fare, true));
    let mut poor = w.clone();
    poor.money = 1.0;
    assert_eq!(decide(Player::B, &poor), up(ParamKey::S1Spawn, false));
}

#[test]
fn no_gain_falls_back_to_cheapest_and_r2_counts_zero() {
    let w = Fake::new()
        .cand(ParamKey::S1Spawn, 30.0, 10.0)
        .cand(ParamKey::S3StationCap, 20.0, 9.0)
        .cand(ParamKey::R2LineSlots, 20.0, 1e9)
        .cand(ParamKey::T1Speed, 40.0, 10.0);
    assert_eq!(decide(Player::B, &w), up(ParamKey::S3StationCap, true));
    assert_eq!(decide(Player::A, &w), up(ParamKey::S3StationCap, true));
}

#[test]
fn nothing_to_do_waits() {
    assert_eq!(decide(Player::B, &Fake::new()), Decision::Wait);
}

#[test]
fn route_names_round_trip() {
    for r in Route::ALL {
        assert_eq!(Route::parse(r.name()), Some(r));
    }
    assert_eq!(Route::parse("x"), None);
}

/// 駅 0〜4 が 1 次元に並ぶ盤面。駅 4 が未接続。
fn line_world() -> Fake {
    let mut w = Fake::new();
    w.unconnected = Some(StationId(4));
    w.pos = vec![0, 20, 9, 6, 7];
    w.connected_ids = vec![StationId(0), StationId(1), StationId(2), StationId(3)];
    w
}

#[test]
fn near_creates_line_to_nearest_connected_station() {
    let mut w = line_world();
    w.free = 1;
    let create = |with| Decision::CreateLine {
        new: StationId(4),
        with: StationId(with),
    };
    assert_eq!(decide(Player::B, &w), create(0));
    assert_eq!(decide_route(Player::B, Route::Far, &w), create(0));
    assert_eq!(decide_route(Player::B, Route::Near, &w), create(3));
    w.pos = vec![0, 20, 8, 6, 7];
    assert_eq!(decide_route(Player::B, Route::Near, &w), create(2));
}

#[test]
fn near_extends_line_with_nearest_end() {
    let mut w = line_world();
    w.sizes = vec![2, 6, 3];
    w.ends = vec![
        [StationId(0), StationId(1)],
        [StationId(2), StationId(0)],
        [StationId(1), StationId(3)],
    ];
    let extend = |line| Decision::Extend {
        line: LineId(line),
        station: StationId(4),
    };
    assert_eq!(decide_route(Player::B, Route::Far, &w), extend(0));
    assert_eq!(decide_route(Player::B, Route::Near, &w), extend(2));
    w.pos = vec![0, 20, 8, 5, 7];
    assert_eq!(decide_route(Player::B, Route::Near, &w), extend(1));
}

#[test]
fn near_breaks_distance_ties_by_line_id_not_size() {
    let mut w = line_world();
    w.pos = vec![0, 20, 9, 5, 7];
    w.sizes = vec![2, 6, 3];
    w.ends = vec![
        [StationId(0), StationId(1)],
        [StationId(2), StationId(0)],
        [StationId(1), StationId(3)],
    ];
    let extend = |line| Decision::Extend {
        line: LineId(line),
        station: StationId(4),
    };
    assert_eq!(decide_route(Player::B, Route::Far, &w), extend(0));
    assert_eq!(decide_route(Player::B, Route::Near, &w), extend(1));
    w.sizes = vec![9, 7, 2];
    w.ends = vec![
        [StationId(3), StationId(1)],
        [StationId(1), StationId(3)],
        [StationId(3), StationId(0)],
    ];
    assert_eq!(decide_route(Player::B, Route::Far, &w), extend(2));
    assert_eq!(decide_route(Player::B, Route::Near, &w), extend(0));
}

/// 延伸を選ぶ盤面(値段 150、所持金 100、足りない分 50)。強化の候補は S1。
fn saving_world() -> Fake {
    let mut w = Fake::new().cand(ParamKey::S1Spawn, 10.0, 30.0);
    w.unconnected = Some(StationId(4));
    w.money = 100.0;
    w.price = Some(150.0);
    w.earning = 1.0;
    w
}

const EXTEND_0: Decision = Decision::Extend {
    line: LineId(0),
    station: StationId(4),
};

#[test]
fn saving_compares_shortfall_with_income_times_save_s() {
    let w = saving_world();
    assert_eq!(decide(Player::B, &w), EXTEND_0);
    assert_eq!(decide_with(Player::B, Route::Far, 50, &w), Decision::Save);
    assert_eq!(
        decide_with(Player::B, Route::Far, SAVE_S, &w),
        Decision::Save
    );
    assert_eq!(
        decide_with(Player::B, Route::Far, 49, &w),
        up(ParamKey::S1Spawn, true)
    );
    assert_eq!(
        decide_with(Player::A, Route::Far, 0, &w),
        up(ParamKey::S1Spawn, true)
    );
    let mut idle = w.clone();
    idle.earning = 0.0;
    idle.price = Some(100.5);
    assert_eq!(
        decide_with(Player::B, Route::Far, SAVE_S, &idle),
        up(ParamKey::S1Spawn, true),
        "収入 0 なら不足がわずかでもためない"
    );
    let mut rich = w.clone();
    rich.earning = 2.0;
    assert_eq!(
        decide_with(Player::C, Route::Far, 25, &rich),
        Decision::Save
    );
    assert_eq!(
        decide_with(Player::C, Route::Far, 24, &rich),
        up(ParamKey::S1Spawn, true)
    );
}

#[test]
fn saving_builds_when_affordable_and_passes_other_decisions() {
    let mut w = saving_world();
    w.price = Some(100.0);
    assert_eq!(decide_with(Player::B, Route::Far, 0, &w), EXTEND_0);
    w.price = None;
    assert_eq!(decide_with(Player::B, Route::Far, SAVE_S, &w), EXTEND_0);
    let mut no_build = saving_world();
    no_build.unconnected = None;
    assert_eq!(
        decide_with(Player::B, Route::Far, SAVE_S, &no_build),
        up(ParamKey::S1Spawn, true)
    );
    let mut nothing = Fake::new();
    nothing.price = Some(1e9);
    assert_eq!(
        decide_with(Player::B, Route::Far, SAVE_S, &nothing),
        Decision::Wait
    );
}

#[test]
fn saving_applies_to_new_lines_too() {
    let mut w = saving_world();
    w.free = 1;
    w.pos = vec![0, 20, 9, 6, 7];
    w.connected_ids = vec![StationId(0), StationId(3)];
    assert_eq!(decide_with(Player::B, Route::Near, 50, &w), Decision::Save);
    w.money = 150.0;
    assert_eq!(
        decide_with(Player::B, Route::Near, 0, &w),
        Decision::CreateLine {
            new: StationId(4),
            with: StationId(3)
        }
    );
}

#[test]
fn only_lines_are_builds() {
    assert!(EXTEND_0.is_build());
    for d in [
        Decision::BuyLineSlots,
        Decision::Wait,
        Decision::Save,
        up(ParamKey::S1Spawn, true),
    ] {
        assert!(!d.is_build(), "{d:?}");
    }
}
