//! 01 付録 A.1 の流量モデル。実際のゲームの推定(A.6、`SegTiles::Actual` と同じ式)と、流量モデルだけの進行(`sim`)。

mod sim;

pub use sim::{FlowSim, SEGMENT_NEW_TILES, SEGMENT_TILES};

use core::cmp::Ordering;

use tm_core::{
    BuyMode, Command, CommandError, GameView, LineId, LineLevels, Money, ParamKey, ParamOwner,
    StationId, StationLevels, spawn_per_sec, train_capacity,
};

use crate::strategy::all_scope;

/// `Money` の値(内部は `f64`。`to_bits` で取り出す)。
pub fn money_f64(m: Money) -> f64 {
    f64::from_bits(m.to_bits())
}

/// `BigNum` の飽和(無限大を `f64::MAX` にする)。
pub fn sat(x: f64) -> f64 {
    if x.is_finite() { x } else { f64::MAX }
}

fn min(a: f64, b: f64) -> f64 {
    if b.total_cmp(&a) == Ordering::Less {
        b
    } else {
        a
    }
}

fn max(a: f64, b: f64) -> f64 {
    if b.total_cmp(&a) == Ordering::Greater {
        b
    } else {
        a
    }
}

/// 停車時間の基本と上限(秒)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dwell {
    /// 基本(01 では 1 秒)。
    pub base_s: f64,
    /// 上限(01 では 10 秒)。
    pub max_s: f64,
}

impl Dwell {
    /// 表の値(ms)から。
    pub fn from_ms(base_ms: u32, max_ms: u32) -> Self {
        Dwell {
            base_s: f64::from(base_ms) / 1000.0,
            max_s: f64::from(max_ms) / 1000.0,
        }
    }
}

/// 停車駅 1 つ(並びは路線の駅の順)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StopFlow {
    /// 魅力度を掛けた発生量(人/秒)。
    pub spawn: f64,
    /// 駅の定員(人)。
    pub capacity: f64,
    /// 運賃(円)。
    pub fare: f64,
}

/// 路線 1 本の入力(01 付録 A.1)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineFlow<'a> {
    /// 最初の停車駅から最後の停車駅までの線路の長さ(マス)。
    pub run_tiles: f64,
    /// 速度(マス/秒)。
    pub speed: f64,
    /// 列車の数。
    pub trains: f64,
    /// 列車の定員。
    pub cap: f64,
    /// 乗り降りの速さ(人/秒)。
    pub boarding: f64,
    /// 停車駅。
    pub stops: &'a [StopFlow],
}

/// 平均乗車駅数 `h = clamp(つながっている駅の数 / 3, 1, 4)`。
pub fn ride_stations(connected: usize) -> f64 {
    min(max(connected as f64 / 3.0, 1.0), 4.0)
}

/// 路線 1 本の 1 秒あたりの収入(A.1 を 3 回反復)。演算は 11 §7.9 と同じく左から 2 項ずつ。
pub fn line_income(l: &LineFlow<'_>, h: f64, dwell: Dwell) -> f64 {
    let count = l.stops.len();
    if count < 2 {
        return 0.0;
    }
    let s_cnt = count as f64;
    let t_run = sat(l.run_tiles / l.speed);
    let t_one_of = |x: f64| {
        let d = min(sat(dwell.base_s + sat(x / l.boarding)), dwell.max_s);
        sat(t_run + sat(s_cnt * d))
    };
    let mut x = 0.0;
    let mut supply = 0.0;
    for _ in 0..3 {
        let headway = sat(sat(2.0 * t_one_of(x)) / l.trains);
        supply = 0.0;
        for s in l.stops {
            supply = sat(supply + min(s.spawn, sat(s.capacity / headway)));
        }
        x = min(l.cap, sat(sat(sat(supply * h) * headway) / s_cnt));
    }
    let t_one = t_one_of(x);
    let arrivals = min(
        sat(supply * h),
        sat(sat(sat(l.trains * l.cap) * (count - 1) as f64) / t_one),
    );
    let fare_sum = l.stops.iter().fold(0.0, |acc, s| sat(acc + s.fare));
    sat(sat(arrivals * fare_sum) / s_cnt)
}

/// 「ぜんぶ」1 段の見積もり: (コスト, いま買えるか)。候補でなければ `None`(11 §7.8 と同じ判定)。
pub fn quote(view: &GameView<'_>, p: ParamKey) -> Option<(Money, bool)> {
    let cmd = Command::LevelUp {
        param: p,
        scope: all_scope(p),
        mode: BuyMode::One,
    };
    match view.validate(&cmd) {
        Ok(preview) => Some((preview.cost, true)),
        Err(CommandError::InsufficientFunds { required, .. }) => Some((required, false)),
        Err(_) => None,
    }
}

/// 「ぜんぶ」1 段で 1 上がる対象(11 §7.3 の `collect_targets` と上限)。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    /// 駅(`StationId` の添字)。
    pub stations: Vec<bool>,
    /// 路線(`LineId` の添字)。
    pub lines: Vec<bool>,
    /// 1 段のコスト(対象のコストの合計)。
    pub step: f64,
}

/// `p` を「ぜんぶ」1 段上げるときの駅と路線の対象(R2 は空)。
pub fn targets(view: &GameView<'_>, p: ParamKey) -> Targets {
    let bal = view.balance();
    let limit = view.level_limit(p);
    let mut t = Targets {
        stations: vec![false; view.stations().len()],
        lines: vec![false; view.lines().len()],
        step: 0.0,
    };
    let mut add = |cur: tm_core::Level, born| {
        t.step = sat(t.step + money_f64(bal.upgrade_cost(p, cur, born)));
    };
    match p.owner() {
        ParamOwner::Station => {
            let by_capacity = p == ParamKey::S1Spawn
                && view.is_unlocked(tm_core::Feature::Param(ParamKey::S3StationCap));
            let margin = bal.spec().transit.spawn_level_margin;
            for s in view.stations().filter(|s| s.connected) {
                let cur = s.levels.get(p).expect("駅のパラメータ");
                let lim = if by_capacity {
                    limit.min(tm_core::Level(s.levels.capacity.0.saturating_add(margin)))
                } else {
                    limit
                };
                if cur < lim {
                    add(cur, s.born_level);
                    t.stations[s.id.index()] = true;
                }
            }
        }
        ParamOwner::Line => {
            for l in view.lines() {
                let cur = l.levels.get(p).expect("路線のパラメータ");
                let mut lim = limit;
                if p == ParamKey::T5Cars {
                    let fit = (0..=limit.0)
                        .rev()
                        .find(|&k| bal.cars(tm_core::Level(k)) <= l.min_platform)
                        .unwrap_or(0);
                    lim = lim.min(tm_core::Level(fit));
                }
                if cur < lim {
                    add(cur, l.born_level);
                    t.lines[l.id.index()] = true;
                }
            }
        }
        ParamOwner::Company => {}
    }
    t
}

/// 実際のゲームの収入の推定(01 付録 A.6。線路は実際の停車駅の間の長さ)。`bump` の対象は 1 段上げて読む。
pub fn estimate_view(view: &GameView<'_>, bump: Option<(ParamKey, &Targets)>) -> f64 {
    let bal = view.balance();
    let tr = &bal.spec().transit;
    let dwell = Dwell::from_ms(tr.dwell_base_ms, tr.dwell_max_ms);
    let h = ride_stations(view.stations().filter(|s| s.connected).count());
    let st_lv = |s: StationId| -> StationLevels {
        let mut lv = view.station(s).expect("出現済みの駅").levels;
        if let Some((p, t)) = bump
            && t.stations.get(s.index()).copied().unwrap_or(false)
            && let Some(v) = lv.get_mut(p)
        {
            *v = v.next();
        }
        lv
    };
    let ln_lv = |l: LineId| -> LineLevels {
        let mut lv = view.line(l).expect("路線").levels;
        if let Some((p, t)) = bump
            && t.lines.get(l.index()).copied().unwrap_or(false)
            && let Some(v) = lv.get_mut(p)
        {
            *v = v.next();
        }
        lv
    };
    let stops_at =
        |l: LineId, s: StationId| bal.platform(st_lv(s).platform) >= bal.cars(ln_lv(l).cars);
    let mut stops = Vec::new();
    let mut income = 0.0;
    for line in view.lines() {
        let l = line.id;
        let lv = ln_lv(l);
        let idx: Vec<usize> = (0..line.stations.len())
            .filter(|&i| stops_at(l, line.stations[i]))
            .collect();
        let (Some(&first), Some(&last)) = (idx.first(), idx.last()) else {
            continue;
        };
        if idx.len() < 2 {
            continue;
        }
        let tiles: u64 = line.segments[first..last]
            .iter()
            .map(|seg| u64::from(seg.len.0))
            .sum();
        let own = bal.attractiveness(lv.attractiveness);
        stops.clear();
        for &i in &idx {
            let s = line.stations[i];
            let sl = st_lv(s);
            let a = view
                .station(s)
                .expect("出現済みの駅")
                .lines
                .iter()
                .filter(|&&o| stops_at(o, s))
                .map(|&o| bal.attractiveness(ln_lv(o).attractiveness))
                .fold(own, core::cmp::Ord::max);
            stops.push(StopFlow {
                spawn: spawn_per_sec(bal.spawn_rate(sl.spawn), a).0 as f64 / 1000.0,
                capacity: bal.station_capacity(sl.capacity).0 as f64,
                fare: money_f64(bal.fare(sl.fare)),
            });
        }
        let cap = train_capacity(
            bal.car_capacity(lv.car_capacity),
            bal.cars(lv.cars),
            bal.crowding(lv.crowding),
        );
        let lf = LineFlow {
            run_tiles: tiles as f64,
            speed: f64::from(bal.speed(lv.speed).0) / 1000.0,
            trains: f64::from(bal.max_trains(lv.max_trains)),
            cap: cap.0 as f64,
            boarding: bal.boarding_rate(lv.boarding).0 as f64,
            stops: &stops,
        };
        income = sat(income + line_income(&lf, h, dwell));
    }
    income
}

#[cfg(test)]
mod tests;
