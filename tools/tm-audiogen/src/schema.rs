//! RON の入力の型(14 §5.2)。

use std::collections::BTreeMap;

use serde::Deserialize;

/// 入力の形式の版。
pub const AUDIO_FORMAT_VERSION: u32 = 1;

/// 1 つの音のファイル(`assets/audio/<名前>.ron`)。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundDef {
    /// 形式の版。[`AUDIO_FORMAT_VERSION`] であること。
    pub format: u32,
    /// 行の長さの決め方。
    pub timing: Timing,
    /// 全体の音量(百分率。0..=400)。
    pub master: u16,
    /// 読み込む共有ファイル(入力のディレクトリからの相対パス)。
    pub imports: Vec<String>,
    /// このファイルだけの音色。
    pub instruments: BTreeMap<String, Instrument>,
    /// チャンネル(1..=4)。
    pub channels: Vec<Channel>,
    /// このファイルだけのパターン。
    pub patterns: BTreeMap<String, Pattern>,
    /// パターンを並べる順番。
    pub order: Vec<OrderRow>,
    /// ループで戻る order の番号(Bgm だけ)。
    #[serde(default)]
    pub loop_to: Option<u16>,
    /// Se/Jingle で最後の行の後に合成を続けるミリ秒。
    #[serde(default)]
    pub tail_ms: u16,
    /// true なら切り詰めを警告にとどめる。
    #[serde(default)]
    pub allow_clip: bool,
}

/// 1 行の長さ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Timing {
    /// 1 行 = 60 / (bpm × rows_per_beat) 秒。
    Bpm {
        /// テンポ(30..=300)。
        bpm: u16,
        /// 1 拍の行数(1..=8)。
        rows_per_beat: u8,
    },
    /// 1 行のミリ秒(5..=1000)。
    RowMs(u16),
}

/// チャンネルの設定。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    /// 名前(エラーの表示に使う)。
    pub name: String,
    /// 音量(0..=15)。
    pub volume: u8,
    /// false はノイズのチャンネル(移調せず、継ぎ目の検査もしない)。
    pub tonal: bool,
}

/// 共有ファイル(`_lib/*.ron`)。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibDef {
    /// 形式の版。
    pub format: u32,
    /// 共有する音色。
    pub instruments: BTreeMap<String, Instrument>,
    /// 共有するパターン。
    pub patterns: BTreeMap<String, Pattern>,
}

fn default_true() -> bool {
    true
}

/// 音色。
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instrument {
    /// 波形。
    pub wave: Wave,
    /// エンベロープ。
    pub env: Adsr,
    /// 音量(0..=100)。
    pub volume: u8,
    /// true ならエンベロープを 16 段に丸める。
    #[serde(default = "default_true")]
    pub quantize: bool,
    /// 半音のずれの列。
    #[serde(default)]
    pub arp: Option<Seq<i8>>,
    /// デューティ比の列(Pulse だけ)。
    #[serde(default)]
    pub duty_seq: Option<Seq<Duty>>,
    /// ビブラート。
    #[serde(default)]
    pub vibrato: Option<Vibrato>,
    /// 鳴り始めのピッチの変化。
    #[serde(default)]
    pub slide: Option<Slide>,
}

/// 波形。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Wave {
    /// 矩形波。
    Pulse {
        /// デューティ比。
        duty: Duty,
    },
    /// 32 段の三角波。
    Triangle,
    /// のこぎり波。
    Saw,
    /// サイン波。
    Sine,
    /// 線形帰還シフトレジスタのノイズ。
    Noise {
        /// 周期の種類。
        mode: NoiseMode,
    },
}

/// デューティ比。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Duty {
    /// 12.5%。
    D12,
    /// 25%。
    D25,
    /// 50%。
    D50,
    /// 75%。
    D75,
}

/// ノイズの周期。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum NoiseMode {
    /// 長周期(32767)。
    Long,
    /// 短周期(93)。
    Short,
}

/// エンベロープ(時間はミリ秒、sustain は 0..=100)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adsr {
    /// アタック。
    pub attack_ms: u16,
    /// ディケイ。
    pub decay_ms: u16,
    /// サステインの高さ。
    pub sustain: u8,
    /// リリース。
    pub release_ms: u16,
}

/// `step_ms` ごとに進む値の列。
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seq<T> {
    /// 値の列。
    pub steps: Vec<T>,
    /// 1 つの値の長さ(ミリ秒)。
    pub step_ms: u16,
    /// false なら最後の値で止まる。
    pub looped: bool,
}

/// ビブラート。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vibrato {
    /// 深さ(セント)。
    pub depth_cents: u16,
    /// 速さ(ミリヘルツ)。
    pub rate_millihz: u32,
    /// かかり始めるまでの時間。
    pub delay_ms: u16,
}

/// 鳴り始めのピッチの変化。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slide {
    /// 1 秒あたりのセント。
    pub cents_per_sec: i32,
    /// 変化を続ける時間。
    pub ms: u16,
}

/// 1 チャンネル分の行の列(行の書き方は 14 §5.3)。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pattern {
    /// 譜面の行。
    pub rows: Vec<String>,
}

/// order の 1 行。チャンネルの順にパターン名を並べる(`"-"` は休み)。
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderRow {
    /// パターン名。
    pub patterns: Vec<String>,
    /// 移調(半音。tonal のチャンネルだけ)。
    pub transpose: i8,
}
