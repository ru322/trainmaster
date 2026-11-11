//! 流量モデルだけの進行(01 付録 A.2〜A.4。1 秒刻み、駅間はすべて 6 マス、値は実数)。

use tm_core::{BalanceTable, CompanyLevel, Feature, ParamKey, ParamOwner, StationId, ValueFormula};

use super::{Dwell, LineFlow, StopFlow, line_income, money_f64, ride_stations, sat};
use crate::report::{BuildRecord, Milestones, Report};
use crate::scenario::{CREATE_S, EXTEND_S, Scenario, TPS};
use crate::strategy::{self, Decision, World};

/// 01 付録 A.1 の駅間の線路の長さ(マス)。
pub const SEGMENT_TILES: f64 = 6.0;
/// 1 区間で新しく敷くマスの数(つなぐ先の駅のマスはもう線路がある。最初の路線は両端の分で 1 つ多い)。
pub const SEGMENT_NEW_TILES: u32 = 6;

type Levels = [u8; 12];

#[derive(Clone, Debug)]
struct FlowLine {
    stations: Vec<usize>,
    lv: Levels,
    born: CompanyLevel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Build {
    OpeningCreate,
    OpeningExtend,
    Create(usize, usize),
    Extend(usize, usize),
}

/// 流量モデルの盤面と進行。`World` として戦略に見せる。
pub struct FlowSim<'b> {
    bal: &'b BalanceTable,
    scenario: Scenario,
    dwell: Dwell,
    last_unlock: u8,
    t: u64,
    pause_left: u64,
    money: f64,
    total: f64,
    goal: f64,
    company: u8,
    stations: Vec<Levels>,
    lines: Vec<FlowLine>,
    r2: u8,
    bought: [bool; 12],
    build: Option<(Build, u64)>,
    milestones: Milestones,
    tiles: u32,
    spent: f64,
    saved_s: u64,
    builds: Vec<BuildRecord>,
}

fn unit(p: ParamKey) -> f64 {
    match p {
        ParamKey::S1Spawn | ParamKey::T1Speed | ParamKey::R3Attractiveness => 1000.0,
        ParamKey::T2Crowding => 4.0,
        _ => 1.0,
    }
}

impl<'b> FlowSim<'b> {
    /// 新しい盤面(駅は `stations_base` 個、路線なし)。
    pub fn new(bal: &'b BalanceTable, scenario: Scenario) -> Self {
        let spec = bal.spec();
        let tr = &spec.transit;
        let last_unlock = Feature::all_in_order()
            .map(|f| bal.unlock_level(f).0)
            .max()
            .unwrap_or(0);
        FlowSim {
            bal,
            pause_left: u64::from(scenario.pause.start_s),
            scenario,
            dwell: Dwell::from_ms(tr.dwell_base_ms, tr.dwell_max_ms),
            last_unlock,
            t: 0,
            money: money_f64(bal.initial_money()),
            total: 0.0,
            goal: money_f64(bal.goal()),
            company: 0,
            stations: vec![[0; 12]; usize::from(spec.company.stations_base)],
            lines: Vec::new(),
            r2: 0,
            bought: [false; 12],
            build: Some((Build::OpeningCreate, u64::from(CREATE_S))),
            milestones: Milestones::new(spec.company.max_level),
            tiles: 0,
            spent: 0.0,
            saved_s: 0,
            builds: Vec::new(),
        }
    }

    fn price(&self, new_tiles: u32) -> f64 {
        let c = CompanyLevel(self.company);
        money_f64(self.bal.track_cost(c, self.tiles, new_tiles))
    }

    /// レベル `l` の値(01 §3.2 の単位の実数)。
    pub fn value(&self, p: ParamKey, l: u8) -> f64 {
        let def = self.bal.param(p);
        let v = match def.value {
            ValueFormula::Linear { v0, step } => v0 as f64 + step as f64 * f64::from(l),
            ValueFormula::Geometric { v0, .. } => (0..l).fold(v0 as f64, |v, i| {
                v * def.value_ratio_at(i).expect("Geometric の比") as f64
            }),
        };
        v / unit(p)
    }

    fn unlocked(&self, p: ParamKey) -> bool {
        self.bal.param(p).unlock <= self.company
    }

    fn limit(&self, p: ParamKey) -> u8 {
        self.bal.limit(p, CompanyLevel(self.company)).0
    }

    fn connected(&self, s: usize) -> bool {
        self.lines.iter().any(|l| l.stations.contains(&s))
    }

    fn line_limit(&self, line: &FlowLine, p: ParamKey) -> u8 {
        let lim = self.limit(p);
        if p != ParamKey::T5Cars {
            return lim;
        }
        let min_platform = line
            .stations
            .iter()
            .map(|&s| {
                self.value(
                    ParamKey::S4Platform,
                    self.stations[s][ParamKey::S4Platform.index()],
                )
            })
            .fold(f64::INFINITY, f64::min);
        (0..=lim)
            .rev()
            .find(|&k| self.value(ParamKey::T5Cars, k) <= min_platform)
            .unwrap_or(0)
    }

    /// `p` の「ぜんぶ」1 段で上がる対象(駅、路線、会社)。
    fn raisable(&self, p: ParamKey) -> (Vec<usize>, Vec<usize>, bool) {
        let i = p.index();
        match p.owner() {
            ParamOwner::Station => {
                let lim = self.limit(p);
                let by_capacity = p == ParamKey::S1Spawn && self.unlocked(ParamKey::S3StationCap);
                let margin = self.bal.spec().transit.spawn_level_margin;
                let s3 = ParamKey::S3StationCap.index();
                let s = (0..self.stations.len())
                    .filter(|&s| {
                        let l = if by_capacity {
                            lim.min(self.stations[s][s3].saturating_add(margin))
                        } else {
                            lim
                        };
                        self.connected(s) && self.stations[s][i] < l
                    })
                    .collect();
                (s, Vec::new(), false)
            }
            ParamOwner::Line => {
                let l = (0..self.lines.len())
                    .filter(|&l| self.lines[l].lv[i] < self.line_limit(&self.lines[l], p))
                    .collect();
                (Vec::new(), l, false)
            }
            ParamOwner::Company => (Vec::new(), Vec::new(), self.r2 < self.limit(p)),
        }
    }

    fn step_of(&self, p: ParamKey, r: &(Vec<usize>, Vec<usize>, bool)) -> Option<f64> {
        let i = p.index();
        let cost = |(l, born): (u8, CompanyLevel)| {
            money_f64(self.bal.upgrade_cost(p, tm_core::Level(l), born))
        };
        let station_born = |s: usize| self.bal.station_born_level(StationId::from_index(s));
        let mut levels: Vec<(u8, CompanyLevel)> =
            r.0.iter()
                .map(|&s| (self.stations[s][i], station_born(s)))
                .collect();
        levels.extend(
            r.1.iter()
                .map(|&l| (self.lines[l].lv[i], self.lines[l].born)),
        );
        if r.2 {
            levels.push((self.r2, CompanyLevel::ZERO));
        }
        if levels.is_empty() {
            return None;
        }
        Some(levels.into_iter().fold(0.0, |acc, l| sat(acc + cost(l))))
    }

    fn buy_max(&mut self, p: ParamKey) {
        let i = p.index();
        loop {
            let r = self.raisable(p);
            let Some(step) = self.step_of(p, &r) else {
                break;
            };
            if self.money < step {
                break;
            }
            self.money -= step;
            r.0.iter().for_each(|&s| self.stations[s][i] += 1);
            r.1.iter().for_each(|&l| self.lines[l].lv[i] += 1);
            if r.2 {
                self.r2 += 1;
            }
            self.bought[i] = true;
        }
        if self.bought.iter().all(|&b| b) {
            Milestones::mark(&mut self.milestones.all_params, self.t * TPS);
        }
    }

    fn income_with(&self, stations: &[Levels], lines: &[FlowLine]) -> f64 {
        let s4 = ParamKey::S4Platform.index();
        let t5 = ParamKey::T5Cars.index();
        let stops_at = |l: &FlowLine, s: usize| {
            self.value(ParamKey::S4Platform, stations[s][s4])
                >= self.value(ParamKey::T5Cars, l.lv[t5])
        };
        let connected = (0..stations.len())
            .filter(|&s| lines.iter().any(|l| l.stations.contains(&s)))
            .count();
        let h = ride_stations(connected);
        let lv = |l: &FlowLine, p: ParamKey| self.value(p, l.lv[p.index()]);
        let st = |s: usize, p: ParamKey| self.value(p, stations[s][p.index()]);
        let mut income = 0.0;
        let mut stops = Vec::new();
        for line in lines {
            let idx: Vec<usize> = (0..line.stations.len())
                .filter(|&i| stops_at(line, line.stations[i]))
                .collect();
            if idx.len() < 2 {
                continue;
            }
            stops.clear();
            for &i in &idx {
                let s = line.stations[i];
                let a = lines
                    .iter()
                    .filter(|o| o.stations.contains(&s) && stops_at(o, s))
                    .map(|o| lv(o, ParamKey::R3Attractiveness))
                    .fold(0.0, f64::max);
                stops.push(StopFlow {
                    spawn: st(s, ParamKey::S1Spawn) * a,
                    capacity: st(s, ParamKey::S3StationCap),
                    fare: st(s, ParamKey::S2Fare),
                });
            }
            let lf = LineFlow {
                run_tiles: (idx[idx.len() - 1] - idx[0]) as f64 * SEGMENT_TILES,
                speed: lv(line, ParamKey::T1Speed),
                trains: lv(line, ParamKey::R1MaxTrains),
                cap: lv(line, ParamKey::T4CarCapacity)
                    * lv(line, ParamKey::T5Cars)
                    * lv(line, ParamKey::T2Crowding),
                boarding: lv(line, ParamKey::T3Boarding),
                stops: &stops,
            };
            income = sat(income + line_income(&lf, h, self.dwell));
        }
        income
    }

    fn gauge(&self) -> (u64, u64) {
        let (mut sum, mut lim) = (0u64, 0u64);
        for p in ParamKey::ALL.into_iter().filter(|&p| self.unlocked(p)) {
            let m = self.limit(p);
            let mut add = |l: u8| {
                sum += u64::from(l.min(m));
                lim += u64::from(m);
            };
            match p.owner() {
                ParamOwner::Station => self.stations.iter().for_each(|s| add(s[p.index()])),
                ParamOwner::Line => self.lines.iter().for_each(|l| add(l.lv[p.index()])),
                ParamOwner::Company => add(self.r2),
            }
        }
        (sum, lim)
    }

    fn settle_company(&mut self) {
        let bal = self.bal;
        let rules = &bal.spec().company;
        while self.company < rules.max_level {
            let (sum, lim) = self.gauge();
            if lim == 0 || u64::from(rules.gauge_den) * sum < u64::from(rules.gauge_num) * lim {
                break;
            }
            self.company += 1;
            let wall = self.t * TPS;
            if let Some(slot) = self.milestones.level.get_mut(usize::from(self.company)) {
                Milestones::mark(slot, wall);
            }
            if self.company == self.last_unlock {
                Milestones::mark(&mut self.milestones.all_features, wall);
            }
            if self.company <= self.last_unlock {
                self.pause_left += u64::from(self.scenario.pause.unlock_s);
            }
            for _ in 0..rules.stations_per_level {
                if self.stations.len() < self.bal.max_stations() as usize {
                    self.stations.push([0; 12]);
                }
            }
        }
    }

    fn apply_build(&mut self, b: Build) {
        let k = SEGMENT_NEW_TILES + u32::from(b == Build::OpeningCreate);
        let cost = self.price(k);
        self.money = (self.money - cost).max(0.0);
        self.spent = sat(self.spent + cost);
        self.tiles += k;
        self.builds.push(BuildRecord {
            wall: self.t * TPS,
            level: self.company,
            new_tiles: k,
            cost,
            income: self.income(None),
        });
        match b {
            Build::OpeningCreate => {
                self.lines.push(FlowLine {
                    stations: vec![0, 1],
                    lv: [0; 12],
                    born: CompanyLevel(self.company),
                });
                if self.stations.len() > 2 {
                    self.build = Some((Build::OpeningExtend, self.t + u64::from(EXTEND_S)));
                }
            }
            Build::OpeningExtend => self.lines[0].stations.push(2),
            Build::Create(new, with) => self.lines.push(FlowLine {
                stations: vec![new, with],
                lv: [0; 12],
                born: CompanyLevel(self.company),
            }),
            Build::Extend(line, s) => self.lines[line].stations.push(s),
        }
    }

    fn act(&mut self) {
        let sc = &self.scenario;
        match strategy::decide_with(sc.player, sc.route, sc.save_s, &*self) {
            Decision::CreateLine { new, with } => {
                let b = Build::Create(new.index(), with.index());
                self.build = Some((b, self.t + u64::from(CREATE_S)));
            }
            Decision::BuyLineSlots => self.buy_max(ParamKey::R2LineSlots),
            Decision::Extend { line, station } => {
                let b = Build::Extend(line.index(), station.index());
                self.build = Some((b, self.t + u64::from(EXTEND_S)));
            }
            Decision::Upgrade {
                param,
                affordable: true,
            } => self.buy_max(param),
            Decision::Save => self.saved_s += u64::from(self.scenario.player.interval_s()),
            Decision::Upgrade { .. } | Decision::Wait => {}
        }
    }

    /// 1 秒進める(01 付録 A.2 の順)。
    pub fn step_second(&mut self) {
        self.t += 1;
        if self.pause_left > 0 {
            self.pause_left -= 1;
            if let Some((_, due)) = &mut self.build {
                *due += 1;
            }
            return;
        }
        if let Some((b, due)) = self.build
            && due <= self.t
        {
            self.build = None;
            self.apply_build(b);
        }
        let income = self.income(None);
        self.money = sat(self.money + income);
        self.total = sat(self.total + income);
        let wall = self.t * TPS;
        if income >= 1e8 {
            Milestones::mark(&mut self.milestones.income_1e8, wall);
        }
        if income >= 1e12 {
            Milestones::mark(&mut self.milestones.income_1e12, wall);
        }
        if self.total >= self.goal {
            Milestones::mark(&mut self.milestones.goal, wall);
        }
        let iv = u64::from(self.scenario.player.interval_s());
        let idle = self.scenario.is_idle(self.t * TPS);
        if self.build.is_none() && !idle && self.t.is_multiple_of(iv) {
            self.act();
        }
        self.settle_company();
    }

    /// 所持金と累計収入。
    pub fn money_total(&self) -> (f64, f64) {
        (self.money, self.total)
    }

    /// 会社レベル。
    pub fn company_level(&self) -> u8 {
        self.company
    }

    /// 線路のあるマスの数と線路の支出の合計。
    pub fn track(&self) -> (u32, f64) {
        (self.tiles, self.spent)
    }

    /// `minutes` 分まで進めて結果を返す(`stop_at_goal` ならゴールで止める)。
    pub fn run_for(mut self, minutes: u32) -> Report {
        while self.t < u64::from(minutes) * 60
            && !(self.scenario.stop_at_goal && self.milestones.goal.is_some())
        {
            self.step_second();
        }
        Report {
            seed: None,
            player: self.scenario.player,
            pause: self.scenario.pause,
            route: self.scenario.route,
            end_wall: self.t * TPS,
            end_level: self.company,
            milestones: self.milestones,
            track_tiles: self.tiles,
            track_spent: self.spent,
            saved_s: self.saved_s,
            builds: self.builds,
            reach: Vec::new(),
            game: None,
        }
    }
}

impl World for FlowSim<'_> {
    fn money(&self) -> f64 {
        self.money
    }

    fn first_unconnected(&self) -> Option<StationId> {
        (0..self.stations.len())
            .find(|&s| !self.connected(s))
            .map(StationId::from_index)
    }

    fn first_connected(&self) -> Option<StationId> {
        (0..self.stations.len())
            .find(|&s| self.connected(s))
            .map(StationId::from_index)
    }

    fn connected_stations(&self) -> Vec<StationId> {
        (0..self.stations.len())
            .filter(|&s| self.connected(s))
            .map(StationId::from_index)
            .collect()
    }

    fn line_end_stations(&self) -> Vec<[StationId; 2]> {
        self.lines
            .iter()
            .map(|l| {
                let end = |i: usize| StationId::from_index(l.stations[i]);
                [end(0), end(l.stations.len() - 1)]
            })
            .collect()
    }

    fn distance(&self, _: StationId, _: StationId) -> u32 {
        SEGMENT_NEW_TILES
    }

    fn free_slots(&self) -> u32 {
        let slots = self.value(ParamKey::R2LineSlots, self.r2) as usize;
        let pending = usize::from(matches!(self.build, Some((Build::Create(..), _))));
        u32::try_from(slots.saturating_sub(self.lines.len() + pending)).unwrap_or(0)
    }

    fn line_sizes(&self) -> Vec<usize> {
        self.lines.iter().map(|l| l.stations.len()).collect()
    }

    fn step_cost(&self, p: ParamKey) -> Option<f64> {
        if !self.unlocked(p) {
            return None;
        }
        self.step_of(p, &self.raisable(p))
    }

    fn upgraded_once(&self, p: ParamKey) -> bool {
        self.bought[p.index()]
    }

    fn income(&self, bump: Option<ParamKey>) -> f64 {
        let Some(p) = bump else {
            return self.income_with(&self.stations, &self.lines);
        };
        let (rs, rl, _) = self.raisable(p);
        let mut stations = self.stations.clone();
        let mut lines = self.lines.clone();
        rs.into_iter().for_each(|s| stations[s][p.index()] += 1);
        rl.into_iter().for_each(|l| lines[l].lv[p.index()] += 1);
        self.income_with(&stations, &lines)
    }

    fn build_price(&self, d: Decision) -> Option<f64> {
        d.is_build().then(|| self.price(SEGMENT_NEW_TILES))
    }

    fn earning(&self) -> f64 {
        self.income(None)
    }
}

#[cfg(test)]
mod tests;
