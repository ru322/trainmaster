//! 統合テストとベンチマークで共有する表・シナリオ・イベントのハッシュ。

use std::sync::Arc;

use tm_core::{
    BalanceTable, BuyMode, CHECKPOINT_INTERVAL_TICKS, Command, Feature, Game, GameConfig,
    GameEvent, GameView, LineEnd, LineId, ParamKey, ParamOwner, REPLAY_FORMAT_VERSION,
    RULES_VERSION, ReplayHeader, ReplayLog, StateHasher, StationId, TICKS_PER_SEC, Tick,
    UpgradeScope,
};

pub fn builtin() -> Arc<BalanceTable> {
    Arc::new(BalanceTable::builtin())
}

/// 組み込みの表を `edit` で変えた表。
pub fn custom(edit: impl FnOnce(&mut tm_core::BalanceSpec)) -> Arc<BalanceTable> {
    let mut spec = BalanceTable::builtin().spec().clone();
    edit(&mut spec);
    Arc::new(BalanceTable::from_spec(spec).expect("検証を通る表"))
}

/// 所持金だけを多くした表(終盤に近い状態まで届かせる。11 §11.2)。
pub fn rich() -> Arc<BalanceTable> {
    custom(|s| s.economy.initial_money_yen = 1_000_000_000_000_000)
}

/// コストと線路の値段をすべて 1 円・倍率 1 にし、所持金を最大にした表(01 の上限の規模にすぐ届かせる)。
pub fn cheap() -> Arc<BalanceTable> {
    custom(|s| {
        for p in &mut s.params {
            p.base_yen = 1;
            p.cost_ratio = 1;
            p.cost_steps.clear();
        }
        s.economy.initial_money_yen = u64::MAX;
        s.economy.track.c1_yen = 1;
        s.economy.track.level_ratio = 1;
        s.economy.track.tile_ratio = (1, 1);
    })
}

pub fn new_game(seed: u64, balance: &Arc<BalanceTable>) -> Game {
    Game::new(GameConfig {
        seed,
        balance: Arc::clone(balance),
    })
}

pub fn header(seed: u64, balance: &BalanceTable) -> ReplayHeader {
    ReplayHeader {
        format: REPLAY_FORMAT_VERSION,
        rules_version: RULES_VERSION,
        game_version: "tm-core tests".to_string(),
        rustc_version: "unknown".to_string(),
        ticks_per_sec: TICKS_PER_SEC,
        seed,
        config_hash: balance.config_hash(),
    }
}

/// イベント列のハッシュに 1 つ足す(`Debug` の文字列を FNV-1a 64 に書く)。
pub fn hash_event(h: &mut StateHasher, ev: &GameEvent) {
    h.write_bytes(format!("{ev:?}").as_bytes());
    h.write_u8(0);
}

/// 600 tick ごとの (tick, その区間のイベント列のハッシュ)。
pub type EventHashes = Vec<(Tick, u64)>;

/// `driver` が返すコマンドを積みながら `ticks` まで進め、ログ(600 tick ごとのチェックポイントつき)を作る。
pub fn record(
    seed: u64,
    balance: &Arc<BalanceTable>,
    ticks: u64,
    mut driver: impl FnMut(&GameView<'_>) -> Vec<Command>,
) -> (Game, ReplayLog, EventHashes) {
    let mut g = new_game(seed, balance);
    let mut log = ReplayLog::new(header(seed, balance));
    let mut events = Vec::new();
    let mut eh = StateHasher::new();
    let mut hashes = Vec::new();
    while g.now().0 < ticks {
        for cmd in driver(&g.view()) {
            log.record(g.next_tick(), &cmd);
            g.enqueue(cmd);
        }
        events.clear();
        g.step(&mut events);
        events.iter().for_each(|e| hash_event(&mut eh, e));
        if g.now().0.is_multiple_of(CHECKPOINT_INTERVAL_TICKS) {
            log.checkpoint(&g);
            hashes.push((g.now(), eh.finish()));
            eh = StateHasher::new();
        }
    }
    (g, log, hashes)
}

/// ログを `until` まで再生し、600 tick ごとの (tick, state_hash) とイベント列のハッシュを返す。
pub fn play(
    log: &ReplayLog,
    balance: &Arc<BalanceTable>,
    until: u64,
) -> (Game, Vec<(Tick, u64)>, EventHashes) {
    let mut g = new_game(log.header.seed, balance);
    let mut events = Vec::new();
    let mut eh = StateHasher::new();
    let (mut states, mut hashes) = (Vec::new(), Vec::new());
    let mut entries = log.entries.iter().peekable();
    while g.now().0 < until {
        let t = g.next_tick();
        while let Some(e) = entries.next_if(|e| e.tick == t) {
            g.enqueue(e.cmd.clone());
        }
        events.clear();
        g.step(&mut events);
        events.iter().for_each(|e| hash_event(&mut eh, e));
        if g.now().0.is_multiple_of(CHECKPOINT_INTERVAL_TICKS) {
            states.push((g.now(), g.state_hash()));
            hashes.push((g.now(), eh.finish()));
            eh = StateHasher::new();
        }
    }
    (g, states, hashes)
}

fn station_ids(v: &GameView<'_>) -> Vec<StationId> {
    v.stations().map(|s| s.id).collect()
}

/// 駅の列を通る路線の作成(曲がり方は `default_bend`)。
pub fn create_line(v: &GameView<'_>, stations: Vec<StationId>) -> Command {
    let bends = stations
        .windows(2)
        .map(|w| v.default_bend(w[0], w[1]))
        .collect();
    Command::CreateLine { stations, bends }
}

/// 路線 `l` の近い方の端(同じなら Tail)から駅 `s` へ延ばす(11 §11.5 の推奨)。
pub fn extend_nearer(v: &GameView<'_>, l: LineId, s: StationId) -> Command {
    let [(_, head), (_, tail)] = v.line_ends(l);
    let pos = |id: StationId| v.station(id).expect("出現済み").pos;
    let (end, from) = if pos(head).chebyshev(pos(s)) < pos(tail).chebyshev(pos(s)) {
        (LineEnd::Head, head)
    } else {
        (LineEnd::Tail, tail)
    };
    Command::ExtendLine {
        line: l,
        end,
        station: s,
        bend: v.default_bend(from, s),
    }
}

pub fn all_scope(p: ParamKey) -> UpgradeScope {
    match p.owner() {
        ParamOwner::Station => UpgradeScope::AllStations,
        ParamOwner::Line => UpgradeScope::AllLines,
        ParamOwner::Company => UpgradeScope::Company,
    }
}

fn first_unconnected(v: &GameView<'_>) -> Option<StationId> {
    v.stations().find(|s| !s.connected).map(|s| s.id)
}

/// シナリオ (1): 最初の路線(2 駅)と、10 秒後の延伸だけで放置する(11 §11.3)。
pub fn driver_idle(v: &GameView<'_>) -> Vec<Command> {
    match v.tick().0 {
        0 => vec![create_line(v, vec![StationId(0), StationId(1)])],
        599 => vec![extend_nearer(v, LineId(0), StationId(2))],
        _ => Vec::new(),
    }
}

/// 01 付録 A.3 (b) に近い手順(つなげる駅は延伸、おすすめをいま買えれば 1 段買う)。リプレイ・性質・ベンチ用。
pub fn driver_recommend(v: &GameView<'_>) -> Vec<Command> {
    let mut out = Vec::new();
    if v.lines().len() == 0 {
        out.push(create_line(v, station_ids(v)));
        return out;
    }
    if let Some(s) = first_unconnected(v) {
        let cmd = extend_nearer(v, LineId(0), s);
        if v.validate(&cmd).is_ok() {
            out.push(cmd);
        }
    }
    if let Some(r) = v.recommendation()
        && r.affordable
    {
        let cmd = Command::LevelUp {
            param: r.param,
            scope: r.scope,
            mode: BuyMode::One,
        };
        if v.validate(&cmd).is_ok() {
            out.push(cmd);
        }
    }
    out
}

/// 全駅を通る `k` 本目の路線の駅の並び(路線ごとに並びを変える)。
fn full_order(v: &GameView<'_>, k: usize) -> Vec<StationId> {
    let mut ids = station_ids(v);
    let n = ids.len();
    ids.rotate_left((k * 3) % n);
    if k % 2 == 1 {
        ids.reverse();
    }
    ids
}

/// シナリオ (3): `cheap` の表で全機能を使い、01 の上限(駅 27、路線 6、列車 48)まで作る。
pub fn driver_full_build(v: &GameView<'_>) -> Vec<Command> {
    let mut out = Vec::new();
    let max_stations = v.balance().max_stations() as usize;
    if v.lines().len() == 0 {
        out.push(create_line(v, station_ids(v)));
        return out;
    }
    if let Some(s) = first_unconnected(v) {
        if v.is_unlocked(Feature::LineExtend) {
            out.push(Command::ExtendLine {
                line: LineId(0),
                end: LineEnd::Tail,
                station: s,
                bend: v.default_bend(v.line_ends(LineId(0))[1].1, s),
            });
        }
    } else if v.stations().len() == max_stations {
        let (used, slots) = v.line_slots();
        if used < slots {
            out.push(create_line(v, full_order(v, used as usize)));
        }
    }
    for p in ParamKey::ALL {
        let cmd = Command::LevelUp {
            param: p,
            scope: all_scope(p),
            mode: BuyMode::Max,
        };
        if v.validate(&cmd).is_ok() {
            out.push(cmd);
        }
    }
    out
}

/// `cheap` の表で 01 の上限の規模まで作ったゲーム(ベンチマークの最悪の場面と終盤のテスト)。
pub fn full_build(seed: u64) -> Game {
    let bal = cheap();
    let mut g = new_game(seed, &bal);
    let mut events = Vec::new();
    while !is_full(&g.view()) {
        assert!(g.now().0 < 20_000, "上限の規模に届かない");
        for cmd in driver_full_build(&g.view()) {
            g.enqueue(cmd);
        }
        events.clear();
        g.step(&mut events);
    }
    g
}

/// 駅 27・路線 6(全駅に停車)・列車 48・会社 Lv24。
pub fn is_full(v: &GameView<'_>) -> bool {
    let bal = v.balance();
    v.company().level == v.company().max_level
        && v.stations().len() == bal.max_stations() as usize
        && v.lines().len() == bal.max_lines() as usize
        && v.lines()
            .all(|l| l.stops.iter().all(|&s| s) && l.stations.len() == v.stations().len())
        && v.trains().len() == 48
}
