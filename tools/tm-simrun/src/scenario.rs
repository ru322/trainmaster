//! 01 付録 A.4 のチュートリアルの停止(停止中は `step` しない)と建設の時間を、実際の `Game` に対して再現する。

use std::collections::BTreeMap;
use std::sync::Arc;

use tm_core::{
    BalanceTable, CHECKPOINT_INTERVAL_TICKS, Command, CompanyLevel, Feature, Game, GameConfig,
    GameEvent, GameSnapshot, LineId, Money, ParamKey, PaxLedgerView, REPLAY_FORMAT_VERSION,
    RULES_VERSION, ReplayHeader, ReplayLog, StateHasher, StationId, TICKS_PER_SEC, Tick,
};

use crate::flow::{self, money_f64};
use crate::report::{BuildRecord, GameStats, Milestones, Reach, Report};
use crate::strategy::{self, Decision, GameWorld, Player, Route};

/// 1 秒の tick 数(`u64`)。
pub const TPS: u64 = TICKS_PER_SEC as u64;
/// 路線の作成にかかる時間(秒。01 付録 A.3)。
pub const CREATE_S: u32 = 15;
/// 延伸にかかる時間(秒。01 付録 A.3)。
pub const EXTEND_S: u32 = 8;
/// 建設の値段が足りないとき、強化をやめてためる上限(収入の何秒ぶんか。11 SIM-D-27)。
pub const SAVE_S: u32 = 300;

/// チュートリアルの停止(01 付録 A.4 (iii))。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pause {
    /// 開始時の停止 T0(秒)。
    pub start_s: u32,
    /// 機能が解放される会社レベル(01 では Lv1〜4)に上がるたびの停止 Tu(秒)。
    pub unlock_s: u32,
}

impl Pause {
    /// 停止なし。
    pub const NONE: Pause = Pause {
        start_s: 0,
        unlock_s: 0,
    };
    /// 標準(合計 150 秒)。
    pub const STANDARD: Pause = Pause {
        start_s: 90,
        unlock_s: 15,
    };
    /// 長め(合計 200 秒)。
    pub const LONG: Pause = Pause {
        start_s: 120,
        unlock_s: 20,
    };
    /// 01 §4.6 の 3 通り。
    pub const ALL: [Pause; 3] = [Pause::NONE, Pause::STANDARD, Pause::LONG];

    /// CSV と CLI の名前(`none`、`standard`、`long`。それ以外は `T0+Tu`)。
    pub fn name(&self) -> String {
        match *self {
            Pause::NONE => "none".into(),
            Pause::STANDARD => "standard".into(),
            Pause::LONG => "long".into(),
            Pause { start_s, unlock_s } => format!("{start_s}+{unlock_s}"),
        }
    }

    /// `name` の逆。
    pub fn parse(s: &str) -> Option<Pause> {
        if let Some(p) = Pause::ALL.into_iter().find(|p| p.name() == s) {
            return Some(p);
        }
        let (a, b) = s.split_once('+')?;
        Some(Pause {
            start_s: a.parse().ok()?,
            unlock_s: b.parse().ok()?,
        })
    }
}

/// 放置の窓(壁時計の秒。`start <= t < end` の間は行動しない。01 付録 A.4)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Idle {
    /// 始め。
    pub start_s: u32,
    /// 終わり(`u32::MAX` は終わらない)。
    pub end_s: u32,
}

impl Idle {
    /// `start_s` から後はずっと行動しない。
    pub const fn after(start_s: u32) -> Self {
        Idle {
            start_s,
            end_s: u32::MAX,
        }
    }

    /// CLI の `開始-終了[,開始-終了…]`(分。終了を省くと終わらない)。
    pub fn parse_minutes(s: &str) -> Option<Vec<Idle>> {
        let sec = |t: &str| t.trim().parse::<u32>().ok()?.checked_mul(60);
        s.split(',')
            .map(|w| {
                let (a, b) = w.split_once('-')?;
                let end_s = if b.trim().is_empty() {
                    u32::MAX
                } else {
                    sec(b)?
                };
                let start_s = sec(a)?;
                (start_s < end_s).then_some(Idle { start_s, end_s })
            })
            .collect()
    }

    fn contains(&self, wall: u64) -> bool {
        wall >= u64::from(self.start_s) * TPS
            && (self.end_s == u32::MAX || wall < u64::from(self.end_s) * TPS)
    }
}

/// 自動プレイの条件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// プレイヤー。
    pub player: Player,
    /// チュートリアルの停止。
    pub pause: Pause,
    /// 新しい駅のつなぎ方。
    pub route: Route,
    /// 建設のためにためる上限(収入の何秒ぶんか。11 SIM-D-27)。
    pub save_s: u32,
    /// 放置の窓。最初の路線と延伸、予約した建設は行う。
    pub idle: Vec<Idle>,
    /// この壁時計の秒ごとに到達を記録し、写しから作り直して続ける(`--sessions`)。
    pub session_s: Option<u32>,
    /// ゴールに届いたら止める。
    pub stop_at_goal: bool,
}

impl Scenario {
    /// `far`、ためる上限 `SAVE_S`、放置なし、区切りなし。
    pub const fn new(player: Player, pause: Pause) -> Self {
        Scenario {
            player,
            pause,
            route: Route::Far,
            save_s: SAVE_S,
            idle: Vec::new(),
            session_s: None,
            stop_at_goal: false,
        }
    }

    /// つなぎ方を変えたもの。
    pub fn with_route(mut self, route: Route) -> Self {
        self.route = route;
        self
    }

    /// 壁時計 `wall` に行動しないか。
    pub fn is_idle(&self, wall: u64) -> bool {
        self.idle.iter().any(|w| w.contains(wall))
    }
}

/// 戦略が積んだコマンド 1 つ(テストと集計用)。
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// 決めた壁時計の tick。
    pub decided_wall: u64,
    /// 決めたときのゲームの tick(`now`)。
    pub decided_tick: Tick,
    /// 積んだ壁時計の tick(建設は完成の tick)。
    pub wall: u64,
    /// 適用される tick(`next_tick`)。
    pub tick: Tick,
    /// コマンド。
    pub cmd: Command,
}

#[derive(Clone, Debug)]
struct Build {
    cmd: Command,
    decided_wall: u64,
    decided_tick: Tick,
    due: Tick,
    opening: bool,
}

#[derive(Clone, Debug)]
struct TrackStats {
    spent: Money,
    saved_s: u64,
    builds: Vec<BuildRecord>,
    level_before: u8,
}

#[derive(Clone, Debug, Default)]
struct Terminal {
    last: Option<u64>,
    n: u32,
    sum: f64,
    sumsq: f64,
}

/// 実際の `Game` を 1 壁時計 tick ずつ進める自動プレイ(01 付録 A.2〜A.4 の順)。
pub struct Runner {
    game: Game,
    scenario: Scenario,
    resumed: bool,
    wall: u64,
    pause_left: u64,
    pause_through: CompanyLevel,
    build: Option<Build>,
    events: Vec<GameEvent>,
    actions: Vec<Action>,
    milestones: Milestones,
    bought: [bool; 12],
    recomputes: Vec<u64>,
    node_counts: BTreeMap<u32, u32>,
    recompute_max_per_10s: u32,
    waiting_peak_ratio: f64,
    terminals: BTreeMap<(u32, u32), Terminal>,
    rejected: u32,
    rec_checked: u32,
    rec_mismatch: u32,
    first_mismatch: Option<String>,
    log: Option<Recording>,
    track: TrackStats,
    reach: Vec<Reach>,
    restore_due: bool,
    restores: u32,
    ledger_offset: PaxLedgerView,
}

const NO_OFFSET: PaxLedgerView = PaxLedgerView {
    spawned: 0,
    capacity_loss: 0,
    delivered: 0,
    waiting_cap_loss: 0,
    waiting: 0,
    onboard: 0,
};

/// リプレイログと 600 tick ごとのイベント列のハッシュ(tm-core のシナリオのゴールデンと同じ形)。
#[derive(Clone, Debug)]
pub struct Recording {
    /// コマンドとチェックポイント。
    pub log: ReplayLog,
    /// (tick, その区間のイベント列のハッシュ)。
    pub event_hashes: Vec<(Tick, u64)>,
    hasher: StateHasher,
}

impl Recording {
    /// `scenario_*.ron` の本文(tm-core のゴールデンと同じ `depth_limit(2)` の RON と末尾の改行)。
    pub fn ron_text(&self) -> String {
        let pretty = ron::ser::PrettyConfig::new().depth_limit(2);
        ron::ser::to_string_pretty(&self.log, pretty).expect("ReplayLog は RON に書ける") + "\n"
    }

    /// `scenario_*.events` の本文(1 行に「tick 16 進のハッシュ」)。
    pub fn events_text(&self) -> String {
        self.event_hashes
            .iter()
            .map(|(t, v)| format!("{} {v:016x}\n", t.0))
            .collect()
    }
}

/// イベント列のハッシュに 1 つ足す(`Debug` の文字列と区切りの 0。tm-core のテストと同じ)。
pub fn hash_event(h: &mut StateHasher, ev: &GameEvent) {
    h.write_bytes(format!("{ev:?}").as_bytes());
    h.write_u8(0);
}

/// tm-simrun が書くリプレイログのヘッダ。
pub fn replay_header(seed: u64, balance: &BalanceTable) -> ReplayHeader {
    ReplayHeader {
        format: REPLAY_FORMAT_VERSION,
        rules_version: RULES_VERSION,
        game_version: format!("tm-simrun {}", env!("CARGO_PKG_VERSION")),
        rustc_version: "unknown".to_string(),
        ticks_per_sec: TICKS_PER_SEC,
        seed,
        config_hash: balance.config_hash(),
    }
}

fn last_unlock_level(bal: &BalanceTable) -> CompanyLevel {
    Feature::all_in_order()
        .map(|f| bal.unlock_level(f))
        .max()
        .unwrap_or(CompanyLevel::ZERO)
}

impl Runner {
    /// シード `seed` の新しいゲーム。最初の路線 `[駅0, 駅1]` の建設から始める(01 付録 A.4)。
    pub fn new(seed: u64, balance: Arc<BalanceTable>, scenario: Scenario) -> Self {
        let game = Game::new(GameConfig { seed, balance });
        let first = vec![StationId(0), StationId(1)];
        let bends = vec![game.view().default_bend(first[0], first[1])];
        let build = Build {
            cmd: Command::CreateLine {
                stations: first,
                bends,
            },
            decided_wall: 0,
            decided_tick: Tick(0),
            due: Tick(u64::from(CREATE_S) * TPS),
            opening: true,
        };
        let pause_left = u64::from(scenario.pause.start_s) * TPS;
        Self::start(game, scenario, false, pause_left, Some(build))
    }

    /// 写しから作り直したゲームの続き(`resume`)。壁時計はゲームの tick から始め、開始時の停止と最初の路線はない。
    /// 再開より前の到達の時刻は記録しない。
    pub fn resume(game: Game, scenario: Scenario) -> Self {
        let mut r = Self::start(game, scenario, true, 0, None);
        r.wall = r.game.now().0;
        let v = r.game.view();
        for p in ParamKey::ALL {
            r.bought[p.index()] = v.is_unlocked(Feature::Param(p)) && !v.is_new(p);
        }
        r
    }

    fn start(
        game: Game,
        scenario: Scenario,
        resumed: bool,
        pause_left: u64,
        build: Option<Build>,
    ) -> Self {
        let bal = &game.config().balance;
        let max_level = bal.spec().company.max_level;
        let pause_through = last_unlock_level(bal);
        Runner {
            game,
            scenario,
            resumed,
            wall: 0,
            pause_left,
            pause_through,
            build,
            events: Vec::new(),
            actions: Vec::new(),
            milestones: Milestones::new(max_level),
            bought: [false; 12],
            recomputes: Vec::new(),
            node_counts: BTreeMap::new(),
            recompute_max_per_10s: 0,
            waiting_peak_ratio: 0.0,
            terminals: BTreeMap::new(),
            rejected: 0,
            rec_checked: 0,
            rec_mismatch: 0,
            first_mismatch: None,
            log: None,
            track: TrackStats {
                spent: Money::ZERO,
                saved_s: 0,
                builds: Vec::new(),
                level_before: 0,
            },
            reach: Vec::new(),
            restore_due: false,
            restores: 0,
            ledger_offset: NO_OFFSET,
        }
    }

    /// リプレイログとイベント列のハッシュを記録する(最初の `tick` の前に呼ぶ)。`resume` なら写しを `start` に入れる(v2)。
    pub fn record(&mut self) {
        let bal = &self.game.config().balance;
        let mut log = ReplayLog::new(replay_header(self.game.config().seed, bal));
        if self.resumed {
            log.start = Some(self.game.snapshot());
        }
        self.log = Some(Recording {
            log,
            event_hashes: Vec::new(),
            hasher: StateHasher::new(),
        });
    }

    /// 条件。
    pub fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    /// 壁時計の `wall_s` 秒から後は行動しない(放置の窓を足す)。
    pub fn idle_from(&mut self, wall_s: u32) {
        self.scenario.idle.push(Idle::after(wall_s));
    }

    /// 建設中(作成・延伸の時間の間)か。
    pub fn is_building(&self) -> bool {
        self.build.is_some()
    }

    /// 写しから作り直した回数(`session_s`)。
    pub fn restores(&self) -> u32 {
        self.restores
    }

    /// 記録したログ。
    pub fn recording(&self) -> Option<&Recording> {
        self.log.as_ref()
    }

    /// ゲーム。
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// 壁時計の tick(停止中も進む)。
    pub fn wall(&self) -> u64 {
        self.wall
    }

    /// 停止の残り(tick)。
    pub fn pause_left(&self) -> u64 {
        self.pause_left
    }

    /// 戦略が積んだコマンド。
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// 到達の時刻。
    pub fn milestones(&self) -> &Milestones {
        &self.milestones
    }

    /// 最後の tick のイベント(停止中の tick は空)。
    pub fn events(&self) -> &[GameEvent] {
        &self.events
    }

    fn enqueue(&mut self, decided_wall: u64, decided_tick: Tick, cmd: Command) {
        if self.game.validate(&cmd).is_err() {
            self.rejected += 1;
            return;
        }
        if let Some(r) = &mut self.log {
            r.log.record(self.game.next_tick(), &cmd);
        }
        self.actions.push(Action {
            decided_wall,
            decided_tick,
            wall: self.wall,
            tick: self.game.next_tick(),
            cmd: cmd.clone(),
        });
        self.game.enqueue(cmd);
    }

    /// 壁時計を 1 tick 進める(停止中ならゲームは進めない)。
    pub fn tick(&mut self) {
        self.tick_observing(|_| {});
    }

    /// `tick` と同じ。ゲームを進めた tick では、行動を選ぶ前(キューが空のとき)に `f` を呼ぶ。
    pub fn tick_observing(&mut self, f: impl FnOnce(&Game)) {
        self.wall += 1;
        self.events.clear();
        let boundary = self
            .scenario
            .session_s
            .is_some_and(|s| s > 0 && self.wall.is_multiple_of(u64::from(s) * TPS));
        self.restore_due |= boundary;
        if self.pause_left > 0 {
            self.pause_left -= 1;
        } else {
            self.advance(f);
        }
        if boundary {
            self.mark_reach();
        }
    }

    fn advance(&mut self, f: impl FnOnce(&Game)) {
        let mut applied = None;
        if let Some(b) = &self.build
            && self.game.next_tick() >= b.due
        {
            let b = self.build.take().expect("建設中");
            self.track.level_before = self.game.view().company().level.0;
            self.enqueue(b.decided_wall, b.decided_tick, b.cmd.clone());
            applied = Some(b);
        }
        self.game.step(&mut self.events);
        self.observe();
        if applied.is_some_and(|b| b.opening && matches!(b.cmd, Command::CreateLine { .. })) {
            self.open_extension();
        }
        if self.restore_due {
            self.restore_due = false;
            self.save_and_restore();
        }
        f(&self.game);
        let iv = u64::from(self.scenario.player.interval_s()) * TPS;
        if self.build.is_none() && !self.scenario.is_idle(self.wall) && self.wall.is_multiple_of(iv)
        {
            self.act();
        }
    }

    /// 写し → RON → 作り直し。台帳(test-support)は作り直しで損失が 0 に戻るので、差を足して続ける。
    fn save_and_restore(&mut self) {
        let text = ron::to_string(&self.game.snapshot()).expect("写しは RON に書ける");
        let snap: GameSnapshot = ron::from_str(&text).expect("書いた写しを読める");
        let bal = Arc::clone(&self.game.config().balance);
        let game = Game::restore(bal, &snap)
            .unwrap_or_else(|e| panic!("wall {}: 写しから作り直せない: {e:?}", self.wall));
        let (a, b) = (self.game.pax_ledger(), game.pax_ledger());
        let o = &mut self.ledger_offset;
        o.spawned = o.spawned.wrapping_add(a.spawned.wrapping_sub(b.spawned));
        o.capacity_loss = o
            .capacity_loss
            .wrapping_add(a.capacity_loss.wrapping_sub(b.capacity_loss));
        o.delivered = o
            .delivered
            .wrapping_add(a.delivered.wrapping_sub(b.delivered));
        o.waiting_cap_loss = o
            .waiting_cap_loss
            .wrapping_add(a.waiting_cap_loss.wrapping_sub(b.waiting_cap_loss));
        self.game = game;
        self.restores += 1;
    }

    /// 人数の台帳(写しから作り直した分を足した値)。
    pub fn pax_ledger(&self) -> PaxLedgerView {
        let l = self.game.pax_ledger();
        let o = &self.ledger_offset;
        PaxLedgerView {
            spawned: l.spawned.wrapping_add(o.spawned),
            capacity_loss: l.capacity_loss.wrapping_add(o.capacity_loss),
            delivered: l.delivered.wrapping_add(o.delivered),
            waiting_cap_loss: l.waiting_cap_loss.wrapping_add(o.waiting_cap_loss),
            ..l
        }
    }

    fn mark_reach(&mut self) {
        let v = self.game.view();
        let c = v.company();
        self.reach.push(Reach {
            wall: self.wall,
            level: c.level.0,
            stations: c.station_count,
            unit: v.max_money_unit(),
        });
    }

    fn open_extension(&mut self) {
        let v = self.game.view();
        let (Some(_), Some(third)) = (v.line(LineId(0)), v.station(StationId(2))) else {
            return;
        };
        let third = third.id;
        let cmd = strategy::extend_nearer(&v, LineId(0), third);
        self.build = Some(Build {
            cmd,
            decided_wall: self.wall,
            decided_tick: self.game.now(),
            due: Tick(self.game.now().0 + u64::from(EXTEND_S) * TPS),
            opening: true,
        });
    }

    fn act(&mut self) {
        let player = self.scenario.player;
        let view = self.game.view();
        let world = GameWorld::new(view);
        let sc = &self.scenario;
        let d = strategy::decide_with(player, sc.route, sc.save_s, &world);
        self.track.saved_s += saved_secs(d, player);
        if player.follows_recommendation() {
            let chosen = match d {
                Decision::BuyLineSlots => Some(ParamKey::R2LineSlots),
                Decision::Upgrade { param, .. } => Some(param),
                _ => None,
            };
            if let Some(p) = chosen {
                self.rec_checked += 1;
                let rec = view.recommendation().map(|r| r.param);
                if rec != Some(p) {
                    self.rec_mismatch += 1;
                    self.first_mismatch
                        .get_or_insert_with(|| format!("wall {}: {p:?} / {rec:?}", self.wall));
                }
            }
        }
        let Some(cmd) = world.command(d) else {
            return;
        };
        let secs = match d {
            Decision::CreateLine { .. } => CREATE_S,
            Decision::Extend { .. } => EXTEND_S,
            _ => {
                self.enqueue(self.wall, self.game.now(), cmd);
                return;
            }
        };
        self.build = Some(Build {
            cmd,
            decided_wall: self.wall,
            decided_tick: self.game.now(),
            due: Tick(self.game.now().0 + u64::from(secs) * TPS),
            opening: false,
        });
    }

    fn observe(&mut self) {
        let wall = self.wall;
        let now = self.game.now();
        for i in 0..self.events.len() {
            let ev = self.events[i];
            if let Some(r) = &mut self.log {
                hash_event(&mut r.hasher, &ev);
            }
            match ev {
                GameEvent::CompanyLevelUp {
                    new_level,
                    all_features_unlocked,
                } => {
                    if let Some(slot) = self.milestones.level.get_mut(usize::from(new_level.0)) {
                        Milestones::mark(slot, wall);
                    }
                    if all_features_unlocked {
                        Milestones::mark(&mut self.milestones.all_features, wall);
                    }
                    if new_level <= self.pause_through {
                        self.pause_left += u64::from(self.scenario.pause.unlock_s) * TPS;
                    }
                }
                GameEvent::Purchased { param, .. } if !self.bought[param.index()] => {
                    self.bought[param.index()] = true;
                    if self.bought.iter().all(|&b| b) {
                        Milestones::mark(&mut self.milestones.all_params, wall);
                    }
                }
                GameEvent::GoalReached { .. } => Milestones::mark(&mut self.milestones.goal, wall),
                GameEvent::RoutesRecomputed { node_count } => {
                    *self.node_counts.entry(node_count).or_default() += 1;
                    self.recomputes.push(now.0);
                    let from = now.0.saturating_sub(10 * TPS - 1);
                    let n = self
                        .recomputes
                        .iter()
                        .rev()
                        .take_while(|&&t| t >= from)
                        .count();
                    self.recompute_max_per_10s = self.recompute_max_per_10s.max(n as u32);
                    let keep = self.recomputes.len().saturating_sub(64);
                    self.recomputes.drain(..keep);
                }
                GameEvent::TrainAdded { line, .. } => {
                    self.terminals.retain(|&(l, _), _| l != line.0);
                }
                GameEvent::LineExtended {
                    line,
                    cost,
                    new_tiles,
                    ..
                } => {
                    self.terminals.retain(|&(l, _), _| l != line.0);
                    self.built(cost, new_tiles);
                }
                GameEvent::TrainArrived { line, station, .. } => self.arrival(line, station),
                GameEvent::CommandRejected { .. } => self.rejected += 1,
                GameEvent::LineCreated {
                    cost, new_tiles, ..
                } => self.built(cost, new_tiles),
                _ => {}
            }
        }
        if let Some(r) = &mut self.log
            && now.0.is_multiple_of(CHECKPOINT_INTERVAL_TICKS)
        {
            r.log.checkpoint(&self.game);
            r.event_hashes.push((now, r.hasher.finish()));
            r.hasher = StateHasher::new();
        }
        if now.is_second_boundary() {
            self.per_second();
        }
    }

    fn built(&mut self, cost: Money, new_tiles: u32) {
        let v = self.game.view();
        self.track.spent = self.track.spent.saturating_add(cost);
        self.track.builds.push(BuildRecord {
            wall: self.wall,
            level: self.track.level_before,
            new_tiles,
            cost: money_f64(cost),
            income: money_f64(v.income_per_sec()),
        });
    }

    fn arrival(&mut self, line: LineId, station: StationId) {
        let v = self.game.view();
        let l = v.line(line).expect("路線");
        if l.trains.len() < 2 || !v.line_ends(line).iter().any(|&(_, s)| s == station) {
            return;
        }
        let now = self.game.now().0;
        let t = self.terminals.entry((line.0, station.0)).or_default();
        if let Some(prev) = t.last {
            let d = (now - prev) as f64;
            t.n += 1;
            t.sum += d;
            t.sumsq += d * d;
        }
        t.last = Some(now);
    }

    fn per_second(&mut self) {
        let wall = self.wall;
        let v = self.game.view();
        let income = money_f64(v.income_per_sec());
        if income >= 1e8 {
            Milestones::mark(&mut self.milestones.income_1e8, wall);
        }
        if income >= 1e12 {
            Milestones::mark(&mut self.milestones.income_1e12, wall);
        }
        let factor = f64::from(v.balance().spec().transit.waiting_cap_factor);
        for s in v.stations() {
            let limit = s.capacity.0 as f64 * factor;
            if limit > 0.0 {
                self.waiting_peak_ratio = self.waiting_peak_ratio.max(s.waiting.0 as f64 / limit);
            }
        }
    }

    /// `minutes` 分(壁時計)まで進めて結果を返す(`stop_at_goal` ならゴールで止める)。
    pub fn run_for(self, minutes: u32) -> Report {
        self.run_until(u64::from(minutes) * 60 * TPS)
    }

    /// 壁時計の `end` まで進めて結果を返す(`stop_at_goal` ならゴールで止める)。
    pub fn run_until(mut self, end: u64) -> Report {
        self.advance_until(end);
        self.report()
    }

    /// 壁時計の `end` まで進める(`stop_at_goal` ならゴールで止める)。
    pub fn advance_until(&mut self, end: u64) {
        while self.wall < end && !(self.scenario.stop_at_goal && self.milestones.goal.is_some()) {
            self.tick();
        }
    }

    /// いまの時点の結果。
    pub fn report(&self) -> Report {
        let v = self.game.view();
        let estimate = flow::estimate_view(&v, None);
        let income = money_f64(v.income_per_sec());
        let cvs: Vec<f64> = self
            .terminals
            .values()
            .filter(|t| t.n >= 5)
            .map(|t| {
                let n = f64::from(t.n);
                let mean = t.sum / n;
                let var = (t.sumsq / n - mean * mean).max(0.0);
                var.sqrt() / mean
            })
            .collect();
        let tiles = v
            .track_grid()
            .masks
            .iter()
            .filter(|m| !m.is_empty())
            .count();
        Report {
            seed: Some(self.game.config().seed),
            player: self.scenario.player,
            pause: self.scenario.pause,
            route: self.scenario.route,
            end_wall: self.wall,
            end_level: v.company().level.0,
            milestones: self.milestones.clone(),
            track_tiles: u32::try_from(tiles).expect("マスの数は u32"),
            track_spent: money_f64(self.track.spent),
            saved_s: self.track.saved_s,
            builds: self.track.builds.clone(),
            reach: self.reach.clone(),
            game: Some(GameStats {
                routes_recomputed: self.node_counts.values().sum(),
                node_counts: self.node_counts.clone(),
                recompute_max_per_10s: self.recompute_max_per_10s,
                ledger: self.pax_ledger(),
                waiting_peak_ratio: self.waiting_peak_ratio,
                terminal_cv: (!cvs.is_empty()).then(|| cvs.iter().sum::<f64>() / cvs.len() as f64),
                income_ratio: (estimate > 0.0).then(|| income / estimate),
                rejected: self.rejected,
                rec_checked: self.rec_checked,
                rec_mismatch: self.rec_mismatch,
                first_mismatch: self.first_mismatch.clone(),
            }),
        }
    }
}

/// 行動 1 回でためて待った秒(`Save` だけを行動間隔ぶん数える)。
fn saved_secs(d: Decision, player: Player) -> u64 {
    if d == Decision::Save {
        u64::from(player.interval_s())
    } else {
        0
    }
}

/// シード 1 つ・条件 1 つを `minutes` 分(壁時計)回す。
pub fn run(seed: u64, balance: &Arc<BalanceTable>, scenario: Scenario, minutes: u32) -> Report {
    Runner::new(seed, Arc::clone(balance), scenario).run_for(minutes)
}

/// ゲーム時間 `ticks` まで回してリプレイログを記録する(11 §11.3 のシナリオ (2) のゴールデン用)。
pub fn record_until(
    seed: u64,
    balance: Arc<BalanceTable>,
    scenario: Scenario,
    ticks: u64,
) -> Runner {
    let mut r = Runner::new(seed, balance, scenario);
    r.record();
    while r.game().now().0 < ticks {
        r.tick();
    }
    r
}

/// 複数の組を並列に回し、入力の順に結果を返す。
pub fn run_many(
    jobs: &[(u64, Scenario)],
    balance: &Arc<BalanceTable>,
    minutes: u32,
) -> Vec<Report> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<Report>>> =
        jobs.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|sc| {
        for _ in 0..threads.min(jobs.len()) {
            sc.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some((seed, scenario)) = jobs.get(i) else {
                        break;
                    };
                    let r = run(*seed, balance, scenario.clone(), minutes);
                    *slots[i].lock().expect("結果の枠") = Some(r);
                }
            });
        }
    });
    slots
        .into_iter()
        .map(|s| s.into_inner().expect("結果の枠").expect("全部回した"))
        .collect()
}

#[cfg(test)]
mod tests;
