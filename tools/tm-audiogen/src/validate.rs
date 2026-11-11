//! 検査 E1〜E10 と警告 W1〜W4(14 §5.7)。

use std::fmt;

use crate::load::Merged;
use crate::schema::{Instrument, Timing, Wave};
use crate::tables::SAMPLE_RATE;
use crate::tracker::is_inst_name;
use crate::{Sound, SoundKind};

/// WAV の合計の予算(24MiB)。
pub const BUDGET_BYTES: u64 = 25_165_824;

/// 1 つの音の長さの上限(180 秒)。
pub const MAX_SAMPLES: u64 = 180 * SAMPLE_RATE as u64;

/// 01 §12.2・§12.3 の 23 個の ID。
pub const EXPECTED_IDS: [&str; 23] = [
    "bgm_game_1",
    "bgm_game_2",
    "bgm_game_3",
    "bgm_title",
    "jingle_all_unlocked",
    "jingle_company_up",
    "jingle_goal",
    "se_buy",
    "se_buy_max",
    "se_cancel",
    "se_cannot_buy",
    "se_coin",
    "se_confirm",
    "se_cursor",
    "se_error",
    "se_line_extend",
    "se_line_new",
    "se_menu_close",
    "se_menu_open",
    "se_popup",
    "se_reset",
    "se_station_new",
    "se_unit_up",
];

/// 01 §12.2 の効果音の長さの目安(ミリ秒)。W1 に使う。
const SE_TARGET_MS: [(&str, u32); 16] = [
    ("se_buy", 120),
    ("se_buy_max", 200),
    ("se_cancel", 60),
    ("se_cannot_buy", 150),
    ("se_coin", 80),
    ("se_confirm", 60),
    ("se_cursor", 30),
    ("se_error", 250),
    ("se_line_extend", 300),
    ("se_line_new", 600),
    ("se_menu_close", 120),
    ("se_menu_open", 120),
    ("se_popup", 100),
    ("se_reset", 500),
    ("se_station_new", 400),
    ("se_unit_up", 500),
];

/// 検査の番号。E はエラー、W は警告。
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Code {
    E1,
    E2,
    E3,
    E4,
    E5,
    E6,
    E7,
    E8,
    E9,
    E10,
    W1,
    W2,
    W3,
    W4,
}

impl Code {
    /// エラー(`build` と `check` を 1 で終わらせる)なら true。
    pub fn is_error(self) -> bool {
        !matches!(self, Code::W1 | Code::W2 | Code::W3 | Code::W4)
    }
}

/// 検査の結果 1 件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// 検査の番号。
    pub code: Code,
    /// 対象のファイル(入力のディレクトリからの相対)。
    pub file: String,
    /// 内容。
    pub message: String,
}

impl Diagnostic {
    pub(crate) fn new(code: Code, file: &str, message: &str) -> Diagnostic {
        Diagnostic {
            code,
            file: file.to_string(),
            message: message.to_string(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = if self.code.is_error() {
            "error"
        } else {
            "warning"
        };
        write!(
            f,
            "{level}[{:?}] {}: {}",
            self.code, self.file, self.message
        )
    }
}

/// 合成の前の検査(E2、E5 の名前の書式、E6 の構成、E7 の値の範囲)。
pub(crate) fn check_def(label: &str, kind: SoundKind, m: &Merged) -> Result<(), Diagnostic> {
    let d = &m.def;
    let err = |code, msg: String| Err(Diagnostic::new(code, label, &msg));
    match (kind, d.loop_to) {
        (SoundKind::Bgm, None) => return err(Code::E2, "Bgm に loop_to がない".into()),
        (SoundKind::Se | SoundKind::Jingle, Some(_)) => {
            return err(Code::E2, "Se と Jingle には loop_to を書かない".into());
        }
        _ => {}
    }
    if !(1..=4).contains(&d.channels.len()) {
        return err(
            Code::E6,
            format!("チャンネルの数が {}(1..=4)", d.channels.len()),
        );
    }
    if d.order.is_empty() {
        return err(Code::E6, "order が空".into());
    }
    for (i, row) in d.order.iter().enumerate() {
        if row.patterns.len() != d.channels.len() {
            return err(
                Code::E6,
                format!(
                    "order {i} の列の数 {} がチャンネルの数 {} と違う",
                    row.patterns.len(),
                    d.channels.len()
                ),
            );
        }
    }
    match d.timing {
        Timing::Bpm { bpm, rows_per_beat } => {
            if !(30..=300).contains(&bpm) || !(1..=8).contains(&rows_per_beat) {
                return err(
                    Code::E7,
                    format!("bpm {bpm}(30..=300)か rows_per_beat {rows_per_beat}(1..=8)が範囲外"),
                );
            }
        }
        Timing::RowMs(ms) => {
            if !(5..=1000).contains(&ms) {
                return err(Code::E7, format!("RowMs {ms} が範囲外(5..=1000)"));
            }
        }
    }
    if d.master > 400 {
        return err(Code::E7, format!("master {} が範囲外(0..=400)", d.master));
    }
    for c in &d.channels {
        if c.volume > 15 {
            return err(
                Code::E7,
                format!(
                    "チャンネル {} の volume {} が範囲外(0..=15)",
                    c.name, c.volume
                ),
            );
        }
    }
    if let Some(l) = d.loop_to
        && usize::from(l) >= d.order.len()
    {
        return err(
            Code::E7,
            format!("loop_to {l} が order の範囲外(0..{})", d.order.len()),
        );
    }
    for (name, inst) in &m.instruments {
        if !is_inst_name(name) {
            return err(
                Code::E5,
                format!("音色の名前 {name:?} が [a-z0-9_]{{1,12}} に合わない"),
            );
        }
        check_inst(inst)
            .map_err(|msg| Diagnostic::new(Code::E7, label, &format!("音色 {name}: {msg}")))?;
    }
    if m.patterns.contains_key("-") {
        return err(Code::E5, "パターンの名前に \"-\" は使えない".into());
    }
    Ok(())
}

fn check_inst(i: &Instrument) -> Result<(), String> {
    if i.volume > 100 {
        return Err(format!("volume {} が範囲外(0..=100)", i.volume));
    }
    if i.env.sustain > 100 {
        return Err(format!("sustain {} が範囲外(0..=100)", i.env.sustain));
    }
    let seq_ok = |len: usize, step_ms: u16| len > 0 && step_ms > 0;
    if let Some(s) = &i.arp
        && !seq_ok(s.steps.len(), s.step_ms)
    {
        return Err("arp の steps が空か step_ms が 0".into());
    }
    if let Some(s) = &i.duty_seq {
        if !seq_ok(s.steps.len(), s.step_ms) {
            return Err("duty_seq の steps が空か step_ms が 0".into());
        }
        if !matches!(i.wave, Wave::Pulse { .. }) {
            return Err("duty_seq は Pulse の音色にだけ書ける".into());
        }
    }
    Ok(())
}

/// E10 の前半: 合成する前に分かる長さが上限を超えるか。
pub(crate) fn check_length(label: &str, samples: u64) -> Result<(), Diagnostic> {
    if samples > MAX_SAMPLES {
        return Err(Diagnostic::new(
            Code::E10,
            label,
            &format!(
                "長さが {} ms(180 秒を超える)",
                samples * 1000 / u64::from(SAMPLE_RATE)
            ),
        ));
    }
    Ok(())
}

/// 合成の後の検査(E9、E10、W1、W3)。
pub(crate) fn check_sound(label: &str, s: &Sound, allow_clip: bool) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if s.samples.is_empty() {
        out.push(Diagnostic::new(Code::E10, label, "長さが 0 サンプル"));
    }
    if s.clipped > 0 {
        let msg = format!("{} サンプルを切り詰めた", s.clipped);
        out.push(if allow_clip {
            Diagnostic::new(Code::W3, label, &format!("{msg}(allow_clip)"))
        } else {
            Diagnostic::new(Code::E9, label, &msg)
        });
    }
    if s.kind == SoundKind::Se
        && let Some((_, target)) = SE_TARGET_MS.iter().find(|(n, _)| *n == s.name)
    {
        let ms = s.duration_ms();
        let target = u64::from(*target);
        if ms * 2 < target || ms * 2 > target * 3 {
            out.push(Diagnostic::new(
                Code::W1,
                label,
                &format!("長さ {ms} ms が目安 {target} ms の ±50% を外れる"),
            ));
        }
    }
    let pct = peak_limit_pct(&s.name, s.kind);
    if u64::from(s.peak) * 100 > 32_768 * u64::from(pct) {
        out.push(Diagnostic::new(
            Code::W3,
            label,
            &format!("最大の振幅 {} が目安 {pct}% を超える", s.peak),
        ));
    }
    out
}

/// 14 §6.1 の最大の振幅の目安(全体に対する百分率。直流除去の後)。
fn peak_limit_pct(name: &str, kind: SoundKind) -> u32 {
    match (name, kind) {
        ("se_coin", _) => 35,
        (_, SoundKind::Jingle) => 90,
        _ => 50,
    }
}

/// 全体の検査(E3、W4、W2)。
pub(crate) fn check_set(names: &[&str], total_bytes: u64) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let pct = total_bytes * 1000 / BUDGET_BYTES;
    let usage = format!(
        "WAV の合計 {total_bytes} バイト / 予算 {BUDGET_BYTES}({}.{}%)",
        pct / 10,
        pct % 10
    );
    if total_bytes > BUDGET_BYTES {
        out.push(Diagnostic::new(Code::E3, "*", &format!("{usage} を超える")));
    } else if total_bytes * 5 > BUDGET_BYTES * 4 {
        out.push(Diagnostic::new(
            Code::W4,
            "*",
            &format!("{usage} が 80% を超える"),
        ));
    }
    for id in EXPECTED_IDS {
        if !names.contains(&id) {
            out.push(Diagnostic::new(
                Code::W2,
                &format!("{id}.ron"),
                "01 の ID のファイルがない",
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a09_e3_budget_and_w4() {
        let all: Vec<&str> = EXPECTED_IDS.to_vec();
        let d = check_set(&all, BUDGET_BYTES);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, Code::W4);
        let d = check_set(&all, BUDGET_BYTES + 1);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, Code::E3);
        assert!(d[0].message.contains("100.0%"), "{}", d[0].message);
        let d = check_set(&all, BUDGET_BYTES * 4 / 5 + 1);
        assert_eq!(d[0].code, Code::W4);
        assert!(check_set(&all, BUDGET_BYTES * 4 / 5).is_empty());
    }

    #[test]
    fn w2_missing_ids() {
        let d = check_set(&["se_coin"], 0);
        assert_eq!(d.len(), 22);
        assert!(d.iter().all(|x| x.code == Code::W2 && !x.code.is_error()));
    }

    #[test]
    fn expected_ids_are_sorted_and_valid() {
        assert!(
            EXPECTED_IDS
                .windows(2)
                .all(|w| w[0].as_bytes() < w[1].as_bytes())
        );
        assert!(
            EXPECTED_IDS
                .iter()
                .all(|n| crate::load::kind_of(n).is_some())
        );
        assert!(SE_TARGET_MS.iter().all(|(n, _)| EXPECTED_IDS.contains(n)));
    }

    #[test]
    fn a09_e10_length_limit() {
        assert!(check_length("x", MAX_SAMPLES).is_ok());
        assert_eq!(
            check_length("x", MAX_SAMPLES + 1).unwrap_err().code,
            Code::E10
        );
    }
}
