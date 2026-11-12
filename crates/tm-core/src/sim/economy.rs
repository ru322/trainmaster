//! 購入の計画と適用、収入の記帳、収入の窓、所持金の単位とゴール(11 §7.1〜7.7)。

use core::cmp::Ordering;

use tm_numeric::ja_unit_index;

use super::movement;
use super::topology::{self, TrainAnchor};
use crate::balance::BalanceTable;
use crate::command::{CommandError, CommandPreview};
use crate::event::GameEvent;
use crate::ids::{CompanyLevel, Level, LineId, StationId, TrainId};
use crate::param::{BuyMode, Feature, LimitKind, ParamKey, ParamOwner, UpgradeScope};
use crate::recommend::{self, IdSet, Quote, Recommendation};
use crate::state::{GameState, INCOME_WINDOW_SECS, Line, Station};
use crate::units::Money;

/// 強化の対象 1 つ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Station(StationId),
    Line(LineId),
    Company,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TargetState {
    pub target: Target,
    /// 計画の前のレベル。
    pub from: Level,
    /// 計画の後のレベル。
    pub cur: Level,
    pub limit: Level,
    /// `limit` を決めた制約(SIM-D-14)。
    pub kind: LimitKind,
    /// 駅・路線が生まれた会社レベル(値段の下限。M8、M10)。
    pub born: CompanyLevel,
}

const TARGET_CAP: usize = 255;

/// 対象の固定長の入れ物(メモリを確保しない。BV-08 で対象は 255 以下)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TargetBuf {
    len: u8,
    items: [TargetState; TARGET_CAP],
}

impl TargetBuf {
    pub fn new() -> Self {
        let empty = TargetState {
            target: Target::Company,
            from: Level::ZERO,
            cur: Level::ZERO,
            limit: Level::ZERO,
            kind: LimitKind::Intrinsic,
            born: CompanyLevel::ZERO,
        };
        TargetBuf {
            len: 0,
            items: [empty; TARGET_CAP],
        }
    }

    fn push(&mut self, t: TargetState) {
        let i = usize::from(self.len);
        assert!(i < TARGET_CAP, "強化の対象が 255 を超えた");
        self.items[i] = t;
        self.len += 1;
    }

    pub fn as_slice(&self) -> &[TargetState] {
        &self.items[..usize::from(self.len)]
    }

    fn as_mut_slice(&mut self) -> &mut [TargetState] {
        &mut self.items[..usize::from(self.len)]
    }
}

/// 購入の計画(10 §5.3 の検査の手順 1〜4 の結果)。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PurchasePlan {
    pub param: ParamKey,
    pub scope: UpgradeScope,
    pub mode: BuyMode,
    /// 買える段の数(1 以上)。
    pub steps: u32,
    /// 上がるレベルの延べ数。
    pub raised: u32,
    /// 段のコストを段の順に足した額。
    pub cost: Money,
    /// 段ごとに引いた後の所持金(SIM-D-13)。
    pub money_after: Money,
    /// ID 昇順。
    pub targets: TargetBuf,
}

impl PurchasePlan {
    /// `CommandPreview` の値(11 §4.7)。
    pub fn preview(&self) -> CommandPreview {
        CommandPreview {
            cost: self.cost,
            steps: self.steps,
            raised: self.raised,
            track_tiles: 0,
            new_track_tiles: 0,
            new_pass_through: 0,
        }
    }
}

/// `LevelUp` の検査と計画(10 §5.3 の順、TD-09・TD-11)。状態を変えず、メモリを確保しない。
pub(crate) fn plan_purchase(
    st: &GameState,
    bal: &BalanceTable,
    p: ParamKey,
    scope: UpgradeScope,
    mode: BuyMode,
) -> Result<PurchasePlan, CommandError> {
    plan(st, bal, p, scope, mode, true)
}

/// お金の検査を外した 1 段の計画(おすすめで買えない候補の対象を求める。11 §7.8)。
pub(crate) fn plan_ignoring_money(
    st: &GameState,
    bal: &BalanceTable,
    p: ParamKey,
    scope: UpgradeScope,
) -> Result<PurchasePlan, CommandError> {
    plan(st, bal, p, scope, BuyMode::One, false)
}

fn plan(
    st: &GameState,
    bal: &BalanceTable,
    p: ParamKey,
    scope: UpgradeScope,
    mode: BuyMode,
    pay: bool,
) -> Result<PurchasePlan, CommandError> {
    check_scope(p, scope)?;
    check_ids(st, scope)?;
    require_unlocked(st, bal, Feature::Param(p))?;
    if matches!(scope, UpgradeScope::AllStations | UpgradeScope::AllLines) || mode == BuyMode::Max {
        require_unlocked(st, bal, Feature::BulkBuy)?;
    }
    let mut tg = TargetBuf::new();
    collect_targets(st, bal, p, scope, &mut tg)?;
    let ts = tg.as_slice();
    if ts.iter().all(|t| t.cur >= t.limit) {
        return Err(CommandError::AtLevelLimit {
            param: p,
            limit: ts.iter().map(|t| t.limit).min().expect("対象は 1 つ以上"),
            kind: pick_kind(ts),
        });
    }
    let (mut money, mut steps, mut raised, mut total) = (st.money, 0u32, 0u32, Money::ZERO);
    loop {
        let mut step_cost = Money::ZERO;
        let mut n = 0u32;
        for t in tg.as_slice().iter().filter(|t| t.cur < t.limit) {
            step_cost = step_cost.saturating_add(bal.upgrade_cost(p, t.cur, t.born));
            n += 1;
        }
        if n == 0 {
            break;
        }
        if pay {
            if money.total_cmp(&step_cost) == Ordering::Less {
                if steps == 0 {
                    return Err(CommandError::InsufficientFunds {
                        required: step_cost,
                        available: money,
                    });
                }
                break;
            }
            money = money.checked_sub(step_cost).expect("所持金 ≥ 1 段のコスト");
        }
        for t in tg.as_mut_slice().iter_mut().filter(|t| t.cur < t.limit) {
            t.cur = t.cur.next();
        }
        steps += 1;
        raised += n;
        total = total.saturating_add(step_cost);
        if mode == BuyMode::One {
            break;
        }
    }
    Ok(PurchasePlan {
        param: p,
        scope,
        mode,
        steps,
        raised,
        cost: total,
        money_after: money,
        targets: tg,
    })
}

fn check_scope(p: ParamKey, scope: UpgradeScope) -> Result<(), CommandError> {
    let ok = matches!(
        (p.owner(), scope),
        (
            ParamOwner::Station,
            UpgradeScope::Station(_) | UpgradeScope::AllStations
        ) | (
            ParamOwner::Line,
            UpgradeScope::Line(_) | UpgradeScope::AllLines
        ) | (ParamOwner::Company, UpgradeScope::Company)
    );
    if ok {
        Ok(())
    } else {
        Err(CommandError::ScopeMismatch { param: p, scope })
    }
}

fn check_ids(st: &GameState, scope: UpgradeScope) -> Result<(), CommandError> {
    match scope {
        UpgradeScope::Station(id) if id.index() >= st.stations.len() => {
            Err(CommandError::UnknownStation(id))
        }
        UpgradeScope::Line(id) if id.index() >= st.lines.len() => {
            Err(CommandError::UnknownLine(id))
        }
        _ => Ok(()),
    }
}

/// `f` が解放済みでなければ `Locked`(10 §5.3 の手順 2)。
pub(crate) fn require_unlocked(
    st: &GameState,
    bal: &BalanceTable,
    f: Feature,
) -> Result<(), CommandError> {
    if st.progress.unlocked.contains(f) {
        Ok(())
    } else {
        Err(CommandError::Locked {
            feature: f,
            required: bal.unlock_level(f),
        })
    }
}

fn collect_targets(
    st: &GameState,
    bal: &BalanceTable,
    p: ParamKey,
    scope: UpgradeScope,
    tg: &mut TargetBuf,
) -> Result<(), CommandError> {
    let c = st.company.level;
    match scope {
        UpgradeScope::Station(id) => {
            let s = &st.stations[id.index()];
            if !s.is_connected() {
                return Err(CommandError::StationNotConnected(id));
            }
            tg.push(station_target(bal, p, s, c));
        }
        UpgradeScope::AllStations => {
            for s in st.stations.iter().filter(|s| s.is_connected()) {
                tg.push(station_target(bal, p, s, c));
            }
        }
        UpgradeScope::Line(id) => {
            tg.push(line_target(bal, p, &st.lines[id.index()], &st.stations, c))
        }
        UpgradeScope::AllLines => {
            for l in &st.lines {
                tg.push(line_target(bal, p, l, &st.stations, c));
            }
        }
        UpgradeScope::Company => {
            let limit = bal.limit(p, c);
            tg.push(target_state(
                Target::Company,
                st.company.line_slots,
                limit,
                limit_kind(bal, p, limit, None),
                CompanyLevel::ZERO,
            ));
        }
    }
    if tg.as_slice().is_empty() {
        return Err(CommandError::NoTarget { param: p });
    }
    Ok(())
}

fn target_state(
    target: Target,
    level: Level,
    limit: Level,
    kind: LimitKind,
    born: CompanyLevel,
) -> TargetState {
    TargetState {
        target,
        from: level,
        cur: level,
        limit,
        kind,
        born,
    }
}

fn station_target(bal: &BalanceTable, p: ParamKey, s: &Station, c: CompanyLevel) -> TargetState {
    let level = s.levels.get(p).expect("駅のパラメータ");
    let base = bal.limit(p, c);
    let by_capacity = capacity_limit(bal, p, s, c).filter(|&l| l < base);
    let limit = by_capacity.unwrap_or(base);
    let kind = if by_capacity.is_some() {
        LimitKind::Capacity
    } else {
        limit_kind(bal, p, limit, None)
    };
    target_state(
        Target::Station(s.id),
        level,
        limit,
        kind,
        bal.station_born_level(s.id),
    )
}

/// S3 の解放後の、駅の S1 のレベルの上限 = S3 のレベル + `spawn_level_margin`(M9)。S1 以外は `None`。
fn capacity_limit(bal: &BalanceTable, p: ParamKey, s: &Station, c: CompanyLevel) -> Option<Level> {
    let unlocked = bal.unlock_level(Feature::Param(ParamKey::S3StationCap)) <= c;
    (p == ParamKey::S1Spawn && unlocked).then(|| {
        Level(
            s.levels
                .capacity
                .0
                .saturating_add(bal.spec().transit.spawn_level_margin),
        )
    })
}

fn line_target(
    bal: &BalanceTable,
    p: ParamKey,
    line: &Line,
    stations: &[Station],
    c: CompanyLevel,
) -> TargetState {
    let level = line.levels.get(p).expect("路線のパラメータ");
    let base = bal.limit(p, c);
    let top = bal.limit(p, CompanyLevel(bal.spec().company.max_level));
    let by_platform = (p == ParamKey::T5Cars).then(|| platform_limit(bal, line, stations, top));
    let limit = by_platform.map_or(base, |lp| lp.min(base));
    target_state(
        Target::Line(line.id),
        level,
        limit,
        limit_kind(bal, p, limit, by_platform),
        line.born_level,
    )
}

/// `cars(L) ≤ 路線の全駅のホーム長の最小` を満たす最大の `L`(表の範囲 `upto` 以下)[01 D-17]。
fn platform_limit(bal: &BalanceTable, line: &Line, stations: &[Station], upto: Level) -> Level {
    let min_platform = line
        .stations
        .iter()
        .map(|s| bal.platform(stations[s.index()].levels.platform))
        .min()
        .expect("路線に駅がある");
    let mut l = Level::ZERO;
    while l < upto && bal.cars(l.next()) <= min_platform {
        l = l.next();
    }
    l
}

/// SIM-D-14: 固有上限なら `Intrinsic`、ホーム長なら `Platform`、それ以外は `CompanyCap`。
fn limit_kind(
    bal: &BalanceTable,
    p: ParamKey,
    limit: Level,
    by_platform: Option<Level>,
) -> LimitKind {
    if bal.param(p).intrinsic_max == Some(limit.0) {
        LimitKind::Intrinsic
    } else if by_platform == Some(limit) {
        LimitKind::Platform
    } else {
        LimitKind::CompanyCap
    }
}

/// 全部の対象の種類: Platform > Capacity > CompanyCap > Intrinsic(10 TD-12)。
fn pick_kind(ts: &[TargetState]) -> LimitKind {
    let has = |k| ts.iter().any(|t| t.kind == k);
    if has(LimitKind::Platform) {
        LimitKind::Platform
    } else if has(LimitKind::Capacity) {
        LimitKind::Capacity
    } else if has(LimitKind::CompanyCap) {
        LimitKind::CompanyCap
    } else {
        LimitKind::Intrinsic
    }
}

/// 計画を適用する(11 §7.4)。イベントは `Purchased` → `TrainAdded`(TrainId 昇順)→
/// `PassThroughResolved`((LineId, 添字) 昇順)の順に出す。会社レベルの判定は呼び出し側。
pub(crate) fn apply_purchase(
    st: &mut GameState,
    bal: &BalanceTable,
    plan: &PurchasePlan,
    anchors: &mut Vec<TrainAnchor>,
    coords: &mut Vec<(u64, TrainId)>,
    stops_before: &mut Vec<bool>,
    events: &mut Vec<GameEvent>,
) {
    st.money = plan.money_after;
    for t in plan.targets.as_slice() {
        set_level(st, plan.param, t.target, t.cur);
    }
    st.progress.upgraded_once.insert(plan.param);
    st.stats.purchases += 1;
    events.push(GameEvent::Purchased {
        param: plan.param,
        scope: plan.scope,
        mode: plan.mode,
        steps: plan.steps,
        raised: plan.raised,
        cost: plan.cost,
    });
    match plan.param {
        ParamKey::R1MaxTrains => {
            for step in 1..=plan.steps {
                for t in plan.targets.as_slice() {
                    let Target::Line(l) = t.target else {
                        unreachable!("R1 の対象は路線")
                    };
                    if step <= u32::from(t.cur.0 - t.from.0) {
                        let train = movement::place_added_train(st, l, coords);
                        events.push(GameEvent::TrainAdded { train, line: l });
                    }
                }
            }
        }
        ParamKey::S4Platform => {
            let mut changed_any = false;
            for li in 0..st.lines.len() {
                let raised = st.lines[li].stations.iter().any(|&s| {
                    plan.targets
                        .as_slice()
                        .iter()
                        .any(|t| t.target == Target::Station(s) && t.cur > t.from)
                });
                if !raised {
                    continue;
                }
                topology::snapshot_anchors(&st.lines[li], &st.trains, anchors);
                stops_before.clear();
                stops_before.extend_from_slice(&st.lines[li].stops);
                topology::rebuild_line_derived(&mut st.lines[li], &st.stations, bal);
                let line = &st.lines[li];
                if *stops_before == line.stops {
                    continue;
                }
                topology::relocate_trains(line, anchors, &mut st.trains);
                for (idx, (&was, &now)) in stops_before.iter().zip(&line.stops).enumerate() {
                    debug_assert!(!was || now, "停車駅が通過駅になった");
                    if !was && now {
                        events.push(GameEvent::PassThroughResolved {
                            line: line.id,
                            station: line.stations[idx],
                        });
                    }
                }
                st.routing.mark_dirty();
                changed_any = true;
            }
            if changed_any {
                st.map_revision += 1;
            }
        }
        ParamKey::T1Speed | ParamKey::R3Attractiveness => st.routing.mark_dirty(),
        _ => {}
    }
}

fn set_level(st: &mut GameState, p: ParamKey, target: Target, level: Level) {
    let slot = match target {
        Target::Station(s) => st.stations[s.index()].levels.get_mut(p),
        Target::Line(l) => st.lines[l.index()].levels.get_mut(p),
        Target::Company => Some(&mut st.company.line_slots),
    };
    *slot.expect("対象の持ち主とパラメータが合う") = level;
}

/// 「ぜんぶ」1 段の見積もり(おすすめの候補。11 §7.8)。未解放・対象なし・上限は `None`。
pub(crate) fn quote(st: &GameState, bal: &BalanceTable, p: ParamKey) -> Option<Quote> {
    let scope = match p.owner() {
        ParamOwner::Station => UpgradeScope::AllStations,
        ParamOwner::Line => UpgradeScope::AllLines,
        ParamOwner::Company => UpgradeScope::Company,
    };
    let (plan, step, affordable) = match plan_purchase(st, bal, p, scope, BuyMode::One) {
        Ok(plan) => {
            let step = plan.cost;
            (plan, step, true)
        }
        Err(CommandError::InsufficientFunds { required, .. }) => (
            plan_ignoring_money(st, bal, p, scope).ok()?,
            required,
            false,
        ),
        Err(_) => return None,
    };
    let mut raised = IdSet::default();
    for t in plan.targets.as_slice().iter().filter(|t| t.cur > t.from) {
        match t.target {
            Target::Station(s) => raised.insert(s.0),
            Target::Line(l) => raised.insert(l.0),
            Target::Company => {}
        }
    }
    Some(Quote {
        param: p,
        scope,
        step,
        affordable,
        raised,
    })
}

/// おすすめ [01 D-25](`GameView::recommendation` が呼ぶ)。
pub(crate) fn recommendation(st: &GameState, bal: &BalanceTable) -> Option<Recommendation> {
    let quotes = ParamKey::ALL.map(|p| quote(st, bal, p));
    recommend::recommend(st, bal, &quotes)
}

/// 収入を所持金・累計収入・この 1 秒の収入に足す(11 §7.1)。
pub(crate) fn book_income(st: &mut GameState, income: Money) {
    st.money = st.money.saturating_add(income);
    st.progress.total_earned = st.progress.total_earned.saturating_add(income);
    st.stats.income_this_sec = st.stats.income_this_sec.saturating_add(income);
}

/// 手順4の後: 新しく届いた単位を k の昇順に、ゴールを 1 回だけ出す(11 §7.6、10 TD-14)。
pub(crate) fn check_money_milestones(
    st: &mut GameState,
    bal: &BalanceTable,
    events: &mut Vec<GameEvent>,
) {
    let k = ja_unit_index(st.money);
    while st.progress.max_money_unit < k {
        st.progress.max_money_unit += 1;
        events.push(GameEvent::MoneyUnitReached {
            unit: st.progress.max_money_unit,
        });
    }
    if !st.progress.goal_reached
        && st.progress.total_earned.total_cmp(&bal.goal()) != Ordering::Less
    {
        st.progress.goal_reached = true;
        events.push(GameEvent::GoalReached {
            total_earned: st.progress.total_earned,
        });
    }
}

/// 毎秒: この 1 秒の収入を窓に書き、次の 1 秒を 0 から始める(11 §7.7)。
pub(crate) fn roll_income_window(st: &mut GameState) {
    let s = &mut st.stats;
    s.income_window[usize::from(s.window_head)] = s.income_this_sec;
    s.window_head =
        u8::try_from((usize::from(s.window_head) + 1) % INCOME_WINDOW_SECS).expect("窓の添字は u8");
    s.income_this_sec = Money::ZERO;
}

/// 直近 `INCOME_WINDOW_SECS` 秒の平均(添字の昇順に足してから割る。01 D-38)。
pub(crate) fn income_per_sec(st: &GameState) -> Money {
    st.stats
        .income_window
        .iter()
        .fold(Money::ZERO, |acc, &m| acc.saturating_add(m))
        .div_small(INCOME_WINDOW_SECS as u64)
}

#[cfg(test)]
pub(crate) mod fixtures;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod recommend_tests;
