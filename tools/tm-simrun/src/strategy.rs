//! 01 付録 A.3 の自動プレイヤーと、実際のゲームを戦略から見る `GameWorld`。

use core::cmp::Ordering;

use tm_core::{
    BuyMode, Command, CommandError, GameView, LineEnd, LineId, ParamKey, ParamOwner, StationId,
    UpgradeScope,
};

use crate::flow::{self, money_f64};

/// 01 付録 A.3 のプレイヤー。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Player {
    /// (a) すばやい子: いちばん安い強化を買う。
    A,
    /// (b) ふつうの子: おすすめを買う。
    B,
    /// (c) ゆっくりな子: おすすめを買う。
    C,
}

/// 新しい駅のつなぎ方(11 SIM-D-27)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Route {
    /// つながっている駅のうち ID 最小の駅へ(今までの戦略)。
    Far,
    /// いちばん近い駅か路線の端へ(チェビシェフ距離。同じなら ID の小さい方)。
    Near,
}

impl Route {
    /// 両方。
    pub const ALL: [Route; 2] = [Route::Far, Route::Near];

    /// CSV と CLI の名前(`far`、`near`)。
    pub const fn name(self) -> &'static str {
        match self {
            Route::Far => "far",
            Route::Near => "near",
        }
    }

    /// `name` の逆。
    pub fn parse(s: &str) -> Option<Route> {
        Route::ALL.into_iter().find(|r| r.name() == s)
    }
}

impl Player {
    /// すべてのプレイヤー。
    pub const ALL: [Player; 3] = [Player::A, Player::B, Player::C];

    /// 行動間隔(秒。01 付録 A.4)。
    pub const fn interval_s(self) -> u32 {
        match self {
            Player::A => 2,
            Player::B => 4,
            Player::C => 8,
        }
    }

    /// CSV と CLI の名前(`a`、`b`、`c`)。
    pub const fn name(self) -> &'static str {
        match self {
            Player::A => "a",
            Player::B => "b",
            Player::C => "c",
        }
    }

    /// `name` の逆。
    pub fn parse(s: &str) -> Option<Player> {
        Player::ALL.into_iter().find(|p| p.name() == s)
    }

    /// 強化をおすすめ(01 D-25)と同じ手順で選ぶか((b)(c))。
    pub const fn follows_recommendation(self) -> bool {
        !matches!(self, Player::A)
    }
}

/// 戦略が見る盤面。実際のゲーム(`GameWorld`)と流量モデル(`flow::FlowSim`)が実装する。
pub trait World {
    /// 所持金。
    fn money(&self) -> f64;
    /// 出現済みで、路線にも建設予定にも入っていない駅のうち ID 最小。
    fn first_unconnected(&self) -> Option<StationId>;
    /// 路線に入っている駅のうち ID 最小。
    fn first_connected(&self) -> Option<StationId>;
    /// 路線に入っている駅(ID 順)。
    fn connected_stations(&self) -> Vec<StationId>;
    /// 路線ごとの両端の駅(`LineId` の順)。
    fn line_end_stations(&self) -> Vec<[StationId; 2]>;
    /// 2 つの駅のチェビシェフ距離。
    fn distance(&self, a: StationId, b: StationId) -> u32;
    /// 路線の空き枠(建設予定の新路線を引いた数)。
    fn free_slots(&self) -> u32;
    /// 路線ごとの駅の数(`LineId` の順)。
    fn line_sizes(&self) -> Vec<usize>;
    /// 「ぜんぶ」1 段のコスト。解放済みで上げられる対象がなければ `None`。
    fn step_cost(&self, p: ParamKey) -> Option<f64>;
    /// 一度でも強化したか。
    fn upgraded_once(&self, p: ParamKey) -> bool;
    /// 1 秒あたりの収入。`bump` は「ぜんぶ」1 段上げた状態で見積もる。
    fn income(&self, bump: Option<ParamKey>) -> f64;
    /// 建設(`CreateLine`・`Extend`)の線路の値段。建設でないか、お金のほかの理由で作れなければ `None`。
    fn build_price(&self, d: Decision) -> Option<f64>;
    /// 「ためる」規則で比べる 1 秒あたりの収入(実際のゲームは直近 10 秒の平均)。
    fn earning(&self) -> f64;
}

/// 1 回の行動(01 付録 A.3)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// 新しい路線 `[new, with]` を作る(15 秒)。
    CreateLine {
        /// つなぐ駅(未接続)。
        new: StationId,
        /// つながっている駅。
        with: StationId,
    },
    /// R2 を MAX で買う。
    BuyLineSlots,
    /// 路線の端へ延伸する(8 秒)。
    Extend {
        /// 延ばす路線。
        line: LineId,
        /// 足す駅。
        station: StationId,
    },
    /// 強化を MAX で買う。買えなければ待つ。
    Upgrade {
        /// 選んだパラメータ。
        param: ParamKey,
        /// いま 1 段以上買えるか。
        affordable: bool,
    },
    /// 候補がない。
    Wait,
    /// 建設の値段のためにためる(強化しない。11 SIM-D-27)。
    Save,
}

impl Decision {
    /// 路線の作成か延伸か。
    pub const fn is_build(self) -> bool {
        matches!(self, Decision::CreateLine { .. } | Decision::Extend { .. })
    }
}

/// 延伸する路線: 駅数 5 未満(なければ全路線)のうち駅数が最少・ID 最小。
fn extend_target(sizes: &[usize]) -> Option<LineId> {
    let pick = |short: bool| {
        sizes
            .iter()
            .enumerate()
            .filter(|&(_, &n)| !short || n < 5)
            .min_by_key(|&(i, &n)| (n, i))
            .map(|(i, _)| LineId::from_index(i))
    };
    pick(true).or_else(|| pick(false))
}

/// `near` の延伸先: 端の駅が `new` にいちばん近い路線(同じなら路線 ID の小さい方。11 SIM-D-27)。
fn nearest_line(w: &impl World, new: StationId) -> Option<LineId> {
    w.line_end_stations()
        .iter()
        .enumerate()
        .min_by_key(|&(i, ends)| (ends.iter().map(|&e| w.distance(e, new)).min(), i))
        .map(|(i, _)| LineId::from_index(i))
}

fn connect_to(route: Route, w: &impl World, new: StationId) -> Option<StationId> {
    match route {
        Route::Far => w.first_connected(),
        Route::Near => w
            .connected_stations()
            .into_iter()
            .min_by_key(|&s| (w.distance(s, new), s)),
    }
}

/// 01 付録 A.3 の手順で次の行動を選ぶ(つなぎ方は `far`、ためない)。
pub fn decide(player: Player, w: &impl World) -> Decision {
    decide_route(player, Route::Far, w)
}

/// 01 付録 A.3 の手順で次の行動を選ぶ(ためない)。
pub fn decide_route(player: Player, route: Route, w: &impl World) -> Decision {
    let money = w.money();
    if let Some(new) = w.first_unconnected() {
        if w.free_slots() > 0
            && let Some(with) = connect_to(route, w, new)
        {
            return Decision::CreateLine { new, with };
        }
        if w.step_cost(ParamKey::R2LineSlots)
            .is_some_and(|s| s <= money)
        {
            return Decision::BuyLineSlots;
        }
        let line = match route {
            Route::Far => extend_target(&w.line_sizes()),
            Route::Near => nearest_line(w, new),
        };
        if let Some(line) = line {
            return Decision::Extend { line, station: new };
        }
    }
    choose_upgrade(player, w)
}

/// `decide_route` に 11 SIM-D-27 の「ためる」を足した行動。建設の値段が所持金より高いとき、
/// 足りない分が収入の `save_s` 秒ぶん以内なら `Save`、超えるなら強化を選ぶ。
pub fn decide_with(player: Player, route: Route, save_s: u32, w: &impl World) -> Decision {
    let d = decide_route(player, route, w);
    if !d.is_build() {
        return d;
    }
    let Some(price) = w.build_price(d) else {
        return d;
    };
    let money = w.money();
    if price <= money {
        d
    } else if price - money <= w.earning() * f64::from(save_s) {
        Decision::Save
    } else {
        choose_upgrade(player, w)
    }
}

fn cheapest(it: impl Iterator<Item = (ParamKey, f64)>) -> Option<(ParamKey, f64)> {
    it.fold(None, |best, (p, s)| match best {
        Some((_, bs)) if s.total_cmp(&bs) != Ordering::Less => best,
        _ => Some((p, s)),
    })
}

/// 01 付録 A.3 (i) の強化の選び方。同点は `ParamKey` の順(01 §3.2 の表の順)。
pub fn choose_upgrade(player: Player, w: &impl World) -> Decision {
    let money = w.money();
    let cands: Vec<(ParamKey, f64)> = ParamKey::ALL
        .into_iter()
        .filter_map(|p| w.step_cost(p).map(|s| (p, s)))
        .collect();
    if cands.is_empty() {
        return Decision::Wait;
    }
    let upgrade = |(param, step): (ParamKey, f64)| Decision::Upgrade {
        param,
        affordable: step <= money,
    };
    let new = cands
        .iter()
        .copied()
        .filter(|&(p, s)| p != ParamKey::R2LineSlots && !w.upgraded_once(p) && s <= money);
    if let Some(c) = cheapest(new) {
        return upgrade(c);
    }
    if !player.follows_recommendation() {
        return upgrade(cheapest(cands.iter().copied()).expect("候補がある"));
    }
    let any_affordable = cands.iter().any(|&(_, s)| s <= money);
    let pool = || {
        cands
            .iter()
            .copied()
            .filter(move |&(_, s)| s <= money || !any_affordable)
    };
    let base = w.income(None);
    let mut best: Option<(f64, (ParamKey, f64))> = None;
    for (p, s) in pool() {
        let eval = if p == ParamKey::R2LineSlots {
            0.0
        } else {
            let bumped = w.income(Some(p));
            if bumped < base {
                0.0
            } else {
                flow::sat(flow::sat(bumped - base) / s)
            }
        };
        let better = match best {
            None => true,
            Some((be, (_, bs))) => match eval.total_cmp(&be) {
                Ordering::Greater => true,
                Ordering::Less => false,
                Ordering::Equal => s.total_cmp(&bs) == Ordering::Less,
            },
        };
        if better {
            best = Some((eval, (p, s)));
        }
    }
    let (eval, c) = best.expect("候補がある");
    if eval != 0.0 {
        return upgrade(c);
    }
    upgrade(cheapest(pool()).expect("候補がある"))
}

/// 「ぜんぶ」の対象(駅は `AllStations`、路線は `AllLines`、R2 は `Company`)。
pub fn all_scope(p: ParamKey) -> UpgradeScope {
    match p.owner() {
        ParamOwner::Station => UpgradeScope::AllStations,
        ParamOwner::Line => UpgradeScope::AllLines,
        ParamOwner::Company => UpgradeScope::Company,
    }
}

/// 実際のゲームを `World` として見る。候補のコストは作ったときに `validate` で求める。
pub struct GameWorld<'a> {
    view: GameView<'a>,
    steps: [Option<f64>; 12],
}

impl<'a> GameWorld<'a> {
    /// `view` の時点の盤面。
    pub fn new(view: GameView<'a>) -> Self {
        let steps = ParamKey::ALL.map(|p| flow::quote(&view, p).map(|(s, _)| money_f64(s)));
        GameWorld { view, steps }
    }

    /// `far` の `decide_with`。
    pub fn decide_saving(&self, player: Player, save_s: u32) -> Decision {
        decide_with(player, Route::Far, save_s, self)
    }

    /// 行動をコマンドにする。延伸は近い端(同じなら Tail)、曲がり方は `default_bend`(11 §11.5)。
    pub fn command(&self, d: Decision) -> Option<Command> {
        let v = &self.view;
        match d {
            Decision::CreateLine { new, with } => Some(Command::CreateLine {
                stations: vec![new, with],
                bends: vec![v.default_bend(new, with)],
            }),
            Decision::BuyLineSlots => Some(level_up_max(ParamKey::R2LineSlots)),
            Decision::Extend { line, station } => Some(extend_nearer(v, line, station)),
            Decision::Upgrade {
                param,
                affordable: true,
            } => Some(level_up_max(param)),
            Decision::Upgrade { .. } | Decision::Wait | Decision::Save => None,
        }
    }
}

fn level_up_max(param: ParamKey) -> Command {
    Command::LevelUp {
        param,
        scope: all_scope(param),
        mode: BuyMode::Max,
    }
}

/// 路線 `line` の近い方の端(チェビシェフ距離。同じなら Tail)から `station` へ延ばすコマンド。
pub fn extend_nearer(v: &GameView<'_>, line: LineId, station: StationId) -> Command {
    let [(_, head), (_, tail)] = v.line_ends(line);
    let pos = |id: StationId| v.station(id).expect("出現済みの駅").pos;
    let target = pos(station);
    let (end, from) = if pos(head).chebyshev(target) < pos(tail).chebyshev(target) {
        (LineEnd::Head, head)
    } else {
        (LineEnd::Tail, tail)
    };
    Command::ExtendLine {
        line,
        end,
        station,
        bend: v.default_bend(from, station),
    }
}

impl World for GameWorld<'_> {
    fn money(&self) -> f64 {
        money_f64(self.view.money())
    }

    fn first_unconnected(&self) -> Option<StationId> {
        self.view.stations().find(|s| !s.connected).map(|s| s.id)
    }

    fn first_connected(&self) -> Option<StationId> {
        self.view.stations().find(|s| s.connected).map(|s| s.id)
    }

    fn connected_stations(&self) -> Vec<StationId> {
        self.view
            .stations()
            .filter(|s| s.connected)
            .map(|s| s.id)
            .collect()
    }

    fn line_end_stations(&self) -> Vec<[StationId; 2]> {
        self.view
            .lines()
            .map(|l| self.view.line_ends(l.id).map(|(_, s)| s))
            .collect()
    }

    fn distance(&self, a: StationId, b: StationId) -> u32 {
        let pos = |id: StationId| self.view.station(id).expect("出現済みの駅").pos;
        pos(a).chebyshev(pos(b))
    }

    fn free_slots(&self) -> u32 {
        let (used, slots) = self.view.line_slots();
        slots.saturating_sub(used)
    }

    fn line_sizes(&self) -> Vec<usize> {
        self.view.lines().map(|l| l.stations.len()).collect()
    }

    fn step_cost(&self, p: ParamKey) -> Option<f64> {
        self.steps[p.index()]
    }

    fn upgraded_once(&self, p: ParamKey) -> bool {
        self.view.is_unlocked(tm_core::Feature::Param(p)) && !self.view.is_new(p)
    }

    fn income(&self, bump: Option<ParamKey>) -> f64 {
        let targets = bump.map(|p| flow::targets(&self.view, p));
        flow::estimate_view(&self.view, bump.zip(targets.as_ref()))
    }

    fn build_price(&self, d: Decision) -> Option<f64> {
        if !d.is_build() {
            return None;
        }
        match self.view.validate(&self.command(d)?) {
            Ok(p) => Some(money_f64(p.cost)),
            Err(CommandError::InsufficientFunds { required, .. }) => Some(money_f64(required)),
            Err(_) => None,
        }
    }

    fn earning(&self) -> f64 {
        money_f64(self.view.income_per_sec())
    }
}

#[cfg(test)]
mod tests;
