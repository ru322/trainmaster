//! 1 回の実行の結果と CSV(1 実行 1 行)。時刻は壁時計(チュートリアルの停止を含む)の tick。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use tm_core::{PaxLedgerView, TICKS_PER_SEC};

use crate::scenario::Pause;
use crate::strategy::{Player, Route};

/// 到達の時刻(壁時計の tick)。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Milestones {
    /// 会社レベル `k` に上がった時刻(添字 = レベル。0 は使わない)。
    pub level: Vec<Option<u64>>,
    /// すべての `Feature` が解放された時刻。
    pub all_features: Option<u64>,
    /// 12 のパラメータをすべて 1 回以上強化した時刻。
    pub all_params: Option<u64>,
    /// 収入が 1 億円/秒に届いた時刻。
    pub income_1e8: Option<u64>,
    /// 収入が 1 兆円/秒に届いた時刻。
    pub income_1e12: Option<u64>,
    /// 累計収入がゴールに届いた時刻。
    pub goal: Option<u64>,
}

impl Milestones {
    /// 会社レベル `max_level` までの空の記録。
    pub fn new(max_level: u8) -> Self {
        Milestones {
            level: vec![None; usize::from(max_level) + 1],
            ..Milestones::default()
        }
    }

    /// 会社レベル `k` の時刻。
    pub fn level_at(&self, k: usize) -> Option<u64> {
        self.level.get(k).copied().flatten()
    }

    /// 最初の到達だけを記録する。
    pub fn mark(slot: &mut Option<u64>, wall: u64) {
        slot.get_or_insert(wall);
    }
}

/// 区切り(`--sessions`)ごとの到達。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reach {
    /// 壁時計の tick。
    pub wall: u64,
    /// 会社レベル。
    pub level: u8,
    /// 出現済みの駅の数。
    pub stations: u32,
    /// 届いたお金の単位(`max_money_unit`)。
    pub unit: u8,
}

/// 建設(路線の作成・延伸)1 回。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildRecord {
    /// 完成した壁時計の tick。
    pub wall: u64,
    /// 完成の直前の会社レベル(値段を決めたレベル)。
    pub level: u8,
    /// 新しく線路を敷いたマスの数。
    pub new_tiles: u32,
    /// 線路の値段。
    pub cost: f64,
    /// 完成した時点の収入(1 秒あたり。直近 10 秒の平均)。
    pub income: f64,
}

/// 実際のゲームだけで取れる数値(VS-03、VS-04、RT-V-04、11 §11.5 の 3)。
#[derive(Clone, Debug, PartialEq)]
pub struct GameStats {
    /// `RoutesRecomputed` の回数。
    pub routes_recomputed: u32,
    /// 再計算のノード数 → 回数。
    pub node_counts: BTreeMap<u32, u32>,
    /// 10 秒(ゲーム時間)の窓の中の再計算の回数の最大。
    pub recompute_max_per_10s: u32,
    /// 最後の人数の台帳。
    pub ledger: PaxLedgerView,
    /// 駅の待ち人数 ÷ 上限(定員 × `waiting_cap_factor`)の最大(毎秒)。
    pub waiting_peak_ratio: f64,
    /// 列車 2 本以上の路線の、端の駅への到着間隔の変動係数の平均(団子運転の目安)。
    pub terminal_cv: Option<f64>,
    /// 最後の実際の収入(直近の窓の平均)÷ 推定器の値。
    pub income_ratio: Option<f64>,
    /// `CommandRejected` の回数。
    pub rejected: u32,
    /// おすすめと比べた行動の数。
    pub rec_checked: u32,
    /// おすすめと違った行動の数。
    pub rec_mismatch: u32,
    /// 最初の食い違い(壁時計の tick、戦略、おすすめ)。
    pub first_mismatch: Option<String>,
}

/// 1 回の実行の結果。
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// シード(流量モデルは `None`)。
    pub seed: Option<u64>,
    /// プレイヤー。
    pub player: Player,
    /// チュートリアルの停止。
    pub pause: Pause,
    /// つなぎ方。
    pub route: Route,
    /// 終わったときの壁時計の tick。
    pub end_wall: u64,
    /// 終わったときの会社レベル。
    pub end_level: u8,
    /// 到達の時刻。
    pub milestones: Milestones,
    /// 終わったときの線路のあるマスの数。
    pub track_tiles: u32,
    /// 線路の支出の合計。
    pub track_spent: f64,
    /// 建設のためにためて待った時間(秒。ためると決めた行動の数 × 行動間隔)。
    pub saved_s: u64,
    /// 建設の記録。
    pub builds: Vec<BuildRecord>,
    /// 区切りごとの到達。
    pub reach: Vec<Reach>,
    /// 実際のゲームだけの数値。
    pub game: Option<GameStats>,
}

const TICKS_PER_MIN: u64 = 60 * TICKS_PER_SEC as u64;

const GAME_COLUMNS: &str = "routes_recomputed,node_count_min,node_count_max,\
recompute_max_per_10s,delivered,waiting_cap_loss,cap_loss_ratio,waiting_peak_ratio,terminal_cv,\
income_ratio,rejected,rec_checked,rec_mismatch";

/// CSV の見出し(会社レベルの列は `lv1`〜`lv{max_level}`)。
pub fn csv_header(max_level: u8) -> String {
    let mut h = String::from("seed,player,pause,route");
    for k in 1..=max_level {
        let _ = write!(h, ",lv{k}");
    }
    h.push_str(",all_features,all_params,income_1e8,income_1e12,goal,end_min,end_level,");
    h.push_str("track_tiles,track_spent,builds,saved_min,");
    h.push_str(GAME_COLUMNS);
    h.push_str(",reach");
    h
}

/// `--builds` の見出し(建設 1 回 1 行。`cost_s` = 値段 ÷ 収入)。
pub const BUILDS_HEADER: &str = "seed,player,route,wall_min,level,new_tiles,cost,income,cost_s";

/// 壁時計の tick を分(小数 2 桁)にする。
pub fn minutes(wall: u64) -> String {
    let hundredths = (wall * 100 + TICKS_PER_MIN / 2) / TICKS_PER_MIN;
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

fn opt_min(v: Option<u64>) -> String {
    v.map(minutes).unwrap_or_default()
}

fn opt_f(v: Option<f64>, prec: usize) -> String {
    v.map(|x| format!("{x:.prec$}")).unwrap_or_default()
}

impl Report {
    /// `wall` 以内に全機能の解放とゴールに届いたか(11 SIM-D-20 (i))。
    pub fn features_and_goal_within(&self, wall: u64) -> bool {
        let m = &self.milestones;
        m.all_features.is_some_and(|t| t <= wall) && m.goal.is_some_and(|t| t <= wall)
    }

    /// CSV の 1 行(`csv_header` の順。届かなかった時刻は空)。
    pub fn csv_row(&self) -> String {
        let m = &self.milestones;
        let mut row = String::new();
        let seed = self
            .seed
            .map(|s| s.to_string())
            .unwrap_or_else(|| "flow".into());
        let _ = write!(
            row,
            "{seed},{},{},{}",
            self.player.name(),
            self.pause.name(),
            self.route.name()
        );
        let levels = (1..m.level.len()).map(|k| m.level_at(k));
        for t in levels.chain([
            m.all_features,
            m.all_params,
            m.income_1e8,
            m.income_1e12,
            m.goal,
        ]) {
            let _ = write!(row, ",{}", opt_min(t));
        }
        let _ = write!(
            row,
            ",{},{},{},{:.3e},{},{}",
            minutes(self.end_wall),
            self.end_level,
            self.track_tiles,
            self.track_spent,
            self.builds.len(),
            minutes(self.saved_s * TICKS_PER_SEC as u64)
        );
        match &self.game {
            None => row.push_str(&",".repeat(13)),
            Some(g) => {
                let nmin = g.node_counts.keys().next().map(|n| n.to_string());
                let nmax = g.node_counts.keys().next_back().map(|n| n.to_string());
                let loss_ratio = (g.ledger.delivered > 0)
                    .then(|| g.ledger.waiting_cap_loss as f64 / g.ledger.delivered as f64);
                let _ = write!(
                    row,
                    ",{},{},{},{},{},{},{},{:.4},{},{},{},{},{}",
                    g.routes_recomputed,
                    nmin.unwrap_or_default(),
                    nmax.unwrap_or_default(),
                    g.recompute_max_per_10s,
                    g.ledger.delivered,
                    g.ledger.waiting_cap_loss,
                    opt_f(loss_ratio, 6),
                    g.waiting_peak_ratio,
                    opt_f(g.terminal_cv, 3),
                    opt_f(g.income_ratio, 3),
                    g.rejected,
                    g.rec_checked,
                    g.rec_mismatch,
                );
            }
        }
        row.push(',');
        row.push_str(&self.reach_text());
        row
    }

    /// `reach` の列: `分=会社Lv/駅/単位` を `;` で区切る。
    pub fn reach_text(&self) -> String {
        let cells: Vec<String> = self
            .reach
            .iter()
            .map(|r| {
                format!(
                    "{}={}/{}/{}",
                    r.wall / TICKS_PER_MIN,
                    r.level,
                    r.stations,
                    r.unit
                )
            })
            .collect();
        cells.join(";")
    }

    /// `--builds` の行(`BUILDS_HEADER` の順)。
    pub fn build_rows(&self) -> Vec<String> {
        let seed = self.seed.map(|s| s.to_string()).unwrap_or_default();
        self.builds
            .iter()
            .map(|b| {
                let cost_s = (b.income > 0.0).then(|| b.cost / b.income);
                format!(
                    "{seed},{},{},{},{},{},{:.3e},{:.3e},{}",
                    self.player.name(),
                    self.route.name(),
                    minutes(b.wall),
                    b.level,
                    b.new_tiles,
                    b.cost,
                    b.income,
                    opt_f(cost_s, 2)
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
