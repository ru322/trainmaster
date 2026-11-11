//! `Platform` トレイトと計測値 [10 §8.2、14 §2.1]。

use crate::assets::AssetBundle;
use crate::audio::AudioCmd;
use crate::draw::DrawList;
use crate::input::PhysicalInput;
use crate::time::MonoTime;

/// プラットフォームのエラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// SDL などの初期化に失敗した。
    Init(String),
    /// 回復できない描画のエラー。
    Render(String),
    /// アセットの検査かテクスチャの作成に失敗した、または `load_assets` が 2 回目だった。
    Assets(String),
}

/// 直近の計測値。02 §9.4 のデバッグ表示に使う。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlatformStats {
    /// 直前のフレームの間隔(present から present、µs)。
    pub frame_interval_us: u32,
    /// 直前の present にかかった時間(待ちを含む、µs)。
    pub present_us: u32,
    /// 直前のフレームで描いた四角形の数。
    pub draw_quads: u32,
    /// 起動からキャッシュ層を描き直した回数。
    pub layer_redraws: u32,
    /// 音の計測値。
    pub audio: AudioStats,
}

/// 音の計測値。回数は起動からの累計。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioStats {
    /// デバイスが開けて音を鳴らしている。
    pub enabled: bool,
    /// 命令キュー(128)があふれて捨てた数。
    pub dropped_cmds: u32,
    /// ジングルの待ち行列(32)があふれて捨てた数。
    pub dropped_jingles: u32,
    /// 種類の違う ID などで無視した数。
    pub invalid_cmds: u32,
    /// 効果音の枠(8)が埋まっていて古い音を止めた数。
    pub stolen_se: u32,
    /// コールバックの間隔が予定の 1.5 倍を超えた回数。
    pub late_callbacks: u32,
    /// 直近 1 秒のコールバックの間隔の最大値(µs)。
    pub max_callback_gap_us: u32,
}

/// 描画・入力・音・時計の境界。契約は 14 §2.1。
pub trait Platform {
    /// 単調増加するマイクロ秒。何度呼んでもよい。壁時計は使わない。
    fn now(&self) -> MonoTime;

    /// たまった入力を `out` の末尾へ追加する(クリアしない)。ブロックしない。
    /// 入力元が消えるときは、`KeyUp` / `PadButtonUp` を `PadDisconnected` より先に返す。
    fn poll_input(&mut self, out: &mut Vec<PhysicalInput>);

    /// 起動時に 1 回だけ呼ぶ。2 回目以降は `Err(Assets)`。`AssetBundle::validate` の失敗も `Err(Assets)`。
    fn load_assets(&mut self, assets: &AssetBundle) -> Result<(), PlatformError>;

    /// 描いて画面に出し、次のフレームの時刻まで待って戻る。`Err` は回復できない描画のエラーだけ。
    fn present(&mut self, frame: &DrawList) -> Result<(), PlatformError>;

    /// 命令を順にミキサーへ渡す。失敗せず、長くブロックしない。
    fn audio(&mut self, cmds: &[AudioCmd]);

    /// 論理解像度。常に `(LOGICAL_WIDTH, LOGICAL_HEIGHT)`。
    fn logical_size(&self) -> (u32, u32);

    /// 直近の計測値。既定は全部 0。
    fn stats(&self) -> PlatformStats {
        PlatformStats::default()
    }
}
