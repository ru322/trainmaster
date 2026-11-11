//! おすすめ(01 D-25)と収入の推定器(01 付録 A.1・A.6。11 §7.8〜7.9)。状態を変えない。

use core::cmp::Ordering;

use crate::balance::BalanceTable;
use crate::ids::{LineId, StationId};
use crate::param::{LineLevels, ParamKey, ParamOwner, StationLevels, UpgradeScope};
use crate::state::{GameState, Line, Station};
use crate::units::{Money, Permille, spawn_per_sec, train_capacity};

/// おすすめの強化 [01 D-25]。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recommendation {
    /// パラメータ。
    pub param: ParamKey,
    /// `AllStations` / `AllLines` / `Company`(「ぜんぶ」で評価する)。
    pub scope: UpgradeScope,
    /// false = いまは買えない(ためる目標)。
    pub affordable: bool,
    /// 選んだ理由。
    pub reason: RecommendReason,
}

/// おすすめの理由。01 D-25 の (1)〜(4)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecommendReason {
    /// (1) つなげない駅があり、路線の空き枠がない。
    NeedLineSlot,
    /// (2) 解放後に一度も強化していない。
    New,
    /// (3) 増える収入 ÷ コストが最大。
    BestValue,
    /// (4) 収入が増えるものがない。いちばん安いもの。
    Cheapest,
}

/// 駅または路線の ID の集合(`StationId`・`LineId` < 256。BV-08、BV-09)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct IdSet([u64; 4]);

impl IdSet {
    pub fn insert(&mut self, id: u32) {
        assert!(id < 256, "IdSet に入らない ID {id}");
        self.0[(id / 64) as usize] |= 1 << (id % 64);
    }

    pub fn contains(&self, id: u32) -> bool {
        id < 256 && self.0[(id / 64) as usize] & (1 << (id % 64)) != 0
    }
}

/// 「ぜんぶ」1 段の見積もり(`sim::economy::quote` が作る)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Quote {
    pub param: ParamKey,
    pub scope: UpgradeScope,
    /// 1 段のコスト。買えないときは `InsufficientFunds` の `required`。
    pub step: Money,
    pub affordable: bool,
    /// この段で 1 上がる対象。
    pub raised: IdSet,
}

/// 推定器が読むレベルと路線の形(11 §1.3)。`bump` の対象はレベルを 1 上げて読む。
#[derive(Clone, Copy, Debug)]
pub(crate) struct EconSnapshot<'a> {
    pub stations: &'a [Station],
    pub lines: &'a [Line],
    pub connected_count: u32,
    pub bump: Option<(ParamKey, &'a IdSet)>,
}

impl<'a> EconSnapshot<'a> {
    pub fn of(st: &'a GameState) -> Self {
        let connected = st.stations.iter().filter(|s| s.is_connected()).count();
        EconSnapshot {
            stations: &st.stations,
            lines: &st.lines,
            connected_count: u32::try_from(connected).expect("駅の数は u32"),
            bump: None,
        }
    }

    /// `q` の対象を 1 段上げた写し。
    pub fn bumped(self, q: &'a Quote) -> Self {
        EconSnapshot {
            bump: Some((q.param, &q.raised)),
            ..self
        }
    }

    pub fn station_levels(&self, s: StationId) -> StationLevels {
        let mut lv = self.stations[s.index()].levels;
        if let Some((p, set)) = self.bump
            && p.owner() == ParamOwner::Station
            && set.contains(s.0)
        {
            let v = lv.get_mut(p).expect("駅のパラメータ");
            *v = v.next();
        }
        lv
    }

    pub fn line_levels(&self, l: LineId) -> LineLevels {
        let mut lv = self.lines[l.index()].levels;
        if let Some((p, set)) = self.bump
            && p.owner() == ParamOwner::Line
            && set.contains(l.0)
        {
            let v = lv.get_mut(p).expect("路線のパラメータ");
            *v = v.next();
        }
        lv
    }

    fn stops_at(&self, bal: &BalanceTable, l: LineId, s: StationId) -> bool {
        bal.platform(self.station_levels(s).platform) >= bal.cars(self.line_levels(l).cars)
    }

    /// `A(s)`: `s` に停車する路線の魅力度の最大(A.1)。`s` は `l` の停車駅。
    fn attractiveness_at(&self, bal: &BalanceTable, l: LineId, s: StationId) -> Permille {
        let own = bal.attractiveness(self.line_levels(l).attractiveness);
        self.stations[s.index()]
            .serving_lines
            .iter()
            .filter(|&&o| self.stops_at(bal, o, s))
            .map(|&o| bal.attractiveness(self.line_levels(o).attractiveness))
            .fold(own, Permille::max)
    }
}

/// 推定器の線路の長さの取り方。ゲーム内は `Actual`(01 A.6)、A.1 との照合は `Fixed(6)`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SegTiles {
    Actual,
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "A.1 との照合テスト(11 §11.5)だけが使う")
    )]
    Fixed(u32),
}

fn n(v: u64) -> Money {
    Money::from_u64(v)
}

fn min(a: Money, b: Money) -> Money {
    if b.total_cmp(&a) == Ordering::Less {
        b
    } else {
        a
    }
}

fn max(a: Money, b: Money) -> Money {
    if b.total_cmp(&a) == Ordering::Greater {
        b
    } else {
        a
    }
}

fn div(a: Money, b: Money) -> Money {
    a.checked_div(b).expect("割る数は 0 でない(BV-04、BV-15)")
}

/// 1 秒あたりの収入の推定(01 付録 A.1 を 3 回反復。演算は左から 2 項ずつ。11 §7.9、SIM-D-16)。
pub(crate) fn estimate_income(sn: &EconSnapshot, bal: &BalanceTable, seg: SegTiles) -> Money {
    let tr = &bal.spec().transit;
    let base_s = n(u64::from(tr.dwell_base_ms)).div_small(1000);
    let max_s = n(u64::from(tr.dwell_max_ms)).div_small(1000);
    let h = min(
        max(n(u64::from(sn.connected_count)).div_small(3), n(1)),
        n(4),
    );
    let mut income = Money::ZERO;
    for line in sn.lines {
        let l = line.id;
        let lv = sn.line_levels(l);
        let is_stop = |s: &&StationId| sn.stops_at(bal, l, **s);
        let mut count = 0u64;
        let (mut first, mut last) = (usize::MAX, 0usize);
        for (i, s) in line.stations.iter().enumerate() {
            if is_stop(&s) {
                count += 1;
                first = first.min(i);
                last = i;
            }
        }
        if count < 2 {
            continue;
        }
        let s_cnt = n(count);
        let v = n(u64::from(bal.speed(lv.speed).0)).div_small(1000);
        let trains = n(u64::from(bal.max_trains(lv.max_trains)));
        let cap = n(train_capacity(
            bal.car_capacity(lv.car_capacity),
            bal.cars(lv.cars),
            bal.crowding(lv.crowding),
        )
        .0);
        let b = n(bal.boarding_rate(lv.boarding).0);
        let tiles = match seg {
            SegTiles::Actual => u64::from(line.offsets[last] - line.offsets[first]),
            SegTiles::Fixed(m) => (last - first) as u64 * u64::from(m),
        };
        let t_run = div(n(tiles), v);
        let t_one_of = |x: Money| {
            t_run.saturating_add(s_cnt.saturating_mul(min(base_s.saturating_add(div(x, b)), max_s)))
        };
        let mut x = Money::ZERO;
        let mut supply = Money::ZERO;
        for _ in 0..3 {
            let t_one = t_one_of(x);
            let headway = div(n(2).saturating_mul(t_one), trains);
            supply = Money::ZERO;
            for &s in line.stations.iter().filter(is_stop) {
                let sl = sn.station_levels(s);
                let a = sn.attractiveness_at(bal, l, s);
                let spawn = n(spawn_per_sec(bal.spawn_rate(sl.spawn), a).0).div_small(1000);
                let room = div(n(bal.station_capacity(sl.capacity).0), headway);
                supply = supply.saturating_add(min(spawn, room));
            }
            x = min(
                cap,
                div(supply.saturating_mul(h).saturating_mul(headway), s_cnt),
            );
        }
        let t_one = t_one_of(x);
        let arrivals = min(
            supply.saturating_mul(h),
            div(
                trains.saturating_mul(cap).saturating_mul(n(count - 1)),
                t_one,
            ),
        );
        let fare_sum = line
            .stations
            .iter()
            .filter(is_stop)
            .fold(Money::ZERO, |acc, &s| {
                acc.saturating_add(bal.fare(sn.station_levels(s).fare))
            });
        income = income.saturating_add(div(arrivals.saturating_mul(fare_sum), s_cnt));
    }
    income
}

/// 01 D-25 の (1)〜(4) でおすすめを選ぶ(11 §7.8)。`quotes` は `ParamKey` の順の「ぜんぶ」1 段の見積もり。
pub(crate) fn recommend(
    st: &GameState,
    bal: &BalanceTable,
    quotes: &[Option<Quote>; 12],
) -> Option<Recommendation> {
    let cands = || quotes.iter().flatten();
    cands().next()?;
    let r2 = &quotes[ParamKey::R2LineSlots.index()];

    let unconnected = st.stations.iter().any(|s| !s.is_connected());
    let no_free_slot = st.lines.len() as u64 >= u64::from(bal.line_slots(st.company.line_slots));
    if unconnected
        && no_free_slot
        && let Some(q) = r2.as_ref().filter(|q| q.affordable)
    {
        return Some(rec(q, RecommendReason::NeedLineSlot));
    }

    let new = cands().filter(|q| {
        q.affordable
            && q.param != ParamKey::R2LineSlots
            && !st.progress.upgraded_once.contains(q.param)
    });
    if let Some(q) = cheapest(new) {
        return Some(rec(q, RecommendReason::New));
    }

    let any_affordable = cands().any(|q| q.affordable);
    let pool = || cands().filter(move |q| q.affordable || !any_affordable);
    let now = EconSnapshot::of(st);
    let base = estimate_income(&now, bal, SegTiles::Actual);
    let mut best: Option<(Money, &Quote)> = None;
    for q in pool() {
        let eval = if q.param == ParamKey::R2LineSlots {
            Money::ZERO
        } else {
            let bumped = estimate_income(&now.bumped(q), bal, SegTiles::Actual);
            bumped
                .checked_sub(base)
                .map_or(Money::ZERO, |d| div(d, q.step))
        };
        let better = match best {
            None => true,
            Some((be, bq)) => match eval.total_cmp(&be) {
                Ordering::Greater => true,
                Ordering::Less => false,
                Ordering::Equal => q.step.total_cmp(&bq.step) == Ordering::Less,
            },
        };
        if better {
            best = Some((eval, q));
        }
    }
    let (eval, q) = best.expect("候補が 1 つ以上ある");
    if !eval.is_zero() {
        return Some(rec(q, RecommendReason::BestValue));
    }
    cheapest(pool()).map(|q| rec(q, RecommendReason::Cheapest))
}

/// コスト最小(同点は先に来たもの = `ParamKey` の順)。
fn cheapest<'q>(it: impl Iterator<Item = &'q Quote>) -> Option<&'q Quote> {
    it.fold(None, |best: Option<&Quote>, q| match best {
        Some(b) if q.step.total_cmp(&b.step) != Ordering::Less => Some(b),
        _ => Some(q),
    })
}

fn rec(q: &Quote, reason: RecommendReason) -> Recommendation {
    Recommendation {
        param: q.param,
        scope: q.scope,
        affordable: q.affordable,
        reason,
    }
}
