//! 譜面の 1 行の解析(14 §5.3)。

use crate::schema::Duty;
use crate::tables::PITCH_MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoteCmd {
    /// C0 からのセント。
    Pitch(i32),
    /// ノイズの速さの番号(`N-0`〜`N-F`)。
    Noise(u8),
    Hold,
    Release,
    Cut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InstRef {
    Name(String),
    Prev,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fx {
    Arp(u8, u8),
    Up(u8),
    Down(u8),
    Glide(u8),
    Vibrato(u8, u8),
    Duty(Duty),
    Delay(u8),
    Kill(u8),
    Retrig(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub note: NoteCmd,
    pub inst: InstRef,
    pub vol: Option<u8>,
    pub fx: Vec<Fx>,
}

/// 書式の誤り(E6)と値の範囲外(E7)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RowError {
    Syntax(String),
    Range(String),
}

impl RowError {
    pub(crate) fn message(&self) -> &str {
        match self {
            RowError::Syntax(m) | RowError::Range(m) => m,
        }
    }
}

fn syn<T>(msg: String) -> Result<T, RowError> {
    Err(RowError::Syntax(msg))
}

fn range<T>(msg: String) -> Result<T, RowError> {
    Err(RowError::Range(msg))
}

pub(crate) fn parse_row(line: &str) -> Result<Row, RowError> {
    let items: Vec<&str> = line.split_whitespace().collect();
    if !(3..=5).contains(&items.len()) {
        return syn(format!("項目の数が {} 個(3〜5 個): {line:?}", items.len()));
    }
    let note = parse_note(items[0])?;
    let inst = parse_inst(items[1])?;
    let vol = parse_vol(items[2])?;
    let mut fx = Vec::new();
    for item in &items[3..] {
        if let Some(f) = parse_fx(item)? {
            if fx
                .iter()
                .any(|g| std::mem::discriminant(g) == std::mem::discriminant(&f))
            {
                return syn(format!("同じ効果が 2 つある: {item:?}"));
            }
            fx.push(f);
        }
    }
    let is_new = matches!(note, NoteCmd::Pitch(_) | NoteCmd::Noise(_));
    if !is_new && inst != InstRef::Prev {
        return syn("音色は新しい音の行にだけ書ける(ほかの行は `..`)".to_string());
    }
    for f in &fx {
        match f {
            Fx::Glide(_) if !matches!(note, NoteCmd::Pitch(_)) => {
                return syn("G の行には音名が要る".to_string());
            }
            Fx::Delay(_) if !is_new => return syn("S の行には音が要る".to_string()),
            Fx::Retrig(0) => return range("R00 は使えない(01〜FF)".to_string()),
            _ => {}
        }
    }
    Ok(Row {
        note,
        inst,
        vol,
        fx,
    })
}

fn parse_note(s: &str) -> Result<NoteCmd, RowError> {
    match s {
        "..." => return Ok(NoteCmd::Hold),
        "===" => return Ok(NoteCmd::Release),
        "^^^" => return Ok(NoteCmd::Cut),
        _ => {}
    }
    let b = s.as_bytes();
    if b.len() != 3 {
        return syn(format!("音の書き方の誤り: {s:?}"));
    }
    if b[0] == b'N' && b[1] == b'-' {
        return hex1(b[2])
            .map(NoteCmd::Noise)
            .ok_or_else(|| RowError::Syntax(format!("ノイズの番号の誤り(N-0〜N-F): {s:?}")));
    }
    let semi: i32 = match b[0] {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return syn(format!("知らない音名: {s:?}")),
    };
    let sharp = match b[1] {
        b'-' => 0,
        b'#' => 1,
        _ => return syn(format!("音名の 2 文字目は `-` か `#`: {s:?}")),
    };
    if !(b'0'..=b'8').contains(&b[2]) {
        return range(format!("音域外(オクターブは 0〜8): {s:?}"));
    }
    let cents = 1200 * i32::from(b[2] - b'0') + 100 * (semi + sharp);
    if cents > PITCH_MAX {
        return range(format!("音域外(C0〜B8): {s:?}"));
    }
    Ok(NoteCmd::Pitch(cents))
}

fn parse_inst(s: &str) -> Result<InstRef, RowError> {
    if s == ".." {
        return Ok(InstRef::Prev);
    }
    if is_inst_name(s) {
        Ok(InstRef::Name(s.to_string()))
    } else {
        syn(format!("音色の名前の誤り([a-z0-9_]{{1,12}}): {s:?}"))
    }
}

pub(crate) fn is_inst_name(s: &str) -> bool {
    (1..=12).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

fn parse_vol(s: &str) -> Result<Option<u8>, RowError> {
    if s == "..." {
        return Ok(None);
    }
    let b = s.as_bytes();
    if b.len() == 3 && b[0] == b'v' && b[1].is_ascii_digit() && b[2].is_ascii_digit() {
        let v = (b[1] - b'0') * 10 + (b[2] - b'0');
        if v <= 15 {
            return Ok(Some(v));
        }
        return range(format!("音量が範囲外(v00〜v15): {s:?}"));
    }
    syn(format!("音量の誤り(v00〜v15): {s:?}"))
}

fn parse_fx(s: &str) -> Result<Option<Fx>, RowError> {
    if s == "..." {
        return Ok(None);
    }
    let b = s.as_bytes();
    let err = || RowError::Syntax(format!("効果の誤り: {s:?}"));
    if b.len() != 3 {
        return Err(err());
    }
    if b[0] == b'W' {
        let duty = match (b[1], b[2]) {
            (b'0', b'_') => Duty::D12,
            (b'1', b'_') => Duty::D25,
            (b'2', b'_') => Duty::D50,
            (b'3', b'_') => Duty::D75,
            _ => return Err(err()),
        };
        return Ok(Some(Fx::Duty(duty)));
    }
    let (Some(x), Some(y)) = (hex1(b[1]), hex1(b[2])) else {
        return Err(err());
    };
    let xx = x * 16 + y;
    let fx = match b[0] {
        b'A' => Fx::Arp(x, y),
        b'U' => Fx::Up(xx),
        b'D' => Fx::Down(xx),
        b'G' => Fx::Glide(xx),
        b'V' => Fx::Vibrato(x, y),
        b'S' => Fx::Delay(xx),
        b'K' => Fx::Kill(xx),
        b'R' => Fx::Retrig(xx),
        _ => return Err(err()),
    };
    Ok(Some(fx))
}

fn hex1(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a08_valid_rows() {
        let r = parse_row("E-6 coin v15 ...").unwrap();
        assert_eq!(r.note, NoteCmd::Pitch(7600));
        assert_eq!(r.inst, InstRef::Name("coin".into()));
        assert_eq!(r.vol, Some(15));
        assert!(r.fx.is_empty());

        let r = parse_row("C#4 .. ... A47 V3A").unwrap();
        assert_eq!(r.note, NoteCmd::Pitch(4900));
        assert_eq!(r.inst, InstRef::Prev);
        assert_eq!(r.vol, None);
        assert_eq!(r.fx, vec![Fx::Arp(4, 7), Fx::Vibrato(3, 10)]);

        let r = parse_row("=== .. ... ...").unwrap();
        assert_eq!(r.note, NoteCmd::Release);
        assert_eq!(parse_row("^^^ .. v00").unwrap().note, NoteCmd::Cut);
        assert_eq!(parse_row("... .. v07 U10").unwrap().fx, vec![Fx::Up(16)]);
        assert_eq!(parse_row("N-F hat v03").unwrap().note, NoteCmd::Noise(15));
        assert_eq!(parse_row("C-0 a ...").unwrap().note, NoteCmd::Pitch(0));
        assert_eq!(
            parse_row("B-8 a ...").unwrap().note,
            NoteCmd::Pitch(PITCH_MAX)
        );
        assert_eq!(
            parse_row("G-5 a v15 G20 W3_").unwrap().fx,
            vec![Fx::Glide(32), Fx::Duty(Duty::D75)]
        );
        assert_eq!(
            parse_row("C-5 a v15 S02 K0F").unwrap().fx,
            vec![Fx::Delay(2), Fx::Kill(15)]
        );
        assert_eq!(
            parse_row("... .. ... RFF D01").unwrap().fx,
            vec![Fx::Retrig(255), Fx::Down(1)]
        );
    }

    #[test]
    fn a08_invalid_rows() {
        let bad = [
            ("C-4 a", "項目の数"),
            ("C-4 a v15 ... ... ...", "項目の数"),
            ("H-4 a v15", "知らない音名"),
            ("C+4 a v15", "2 文字目"),
            ("C-9 a v15", "音域外"),
            ("B#8 a v15", "音域外"),
            ("N-G a v15", "ノイズ"),
            ("C-4 Lead v15", "音色の名前"),
            ("C-4 abcdefghijklm v15", "音色の名前"),
            ("C-4 a v16", "音量"),
            ("C-4 a 15", "音量"),
            ("C-4 a v15 X00", "効果"),
            ("C-4 a v15 a00", "効果"),
            ("C-4 a v15 W4_", "効果"),
            ("C-4 a v15 W20", "効果"),
            ("C-4 a v15 U1", "効果"),
            ("C-4 a v15 U10 U20", "同じ効果"),
            ("... a v15", "新しい音の行"),
            ("... .. v15 G10", "G の行"),
            ("=== .. ... S01", "S の行"),
            ("C-4 a v15 R00", "R00"),
        ];
        for (line, want) in bad {
            let e = parse_row(line).unwrap_err();
            let range = matches!(e, RowError::Range(_));
            assert!(e.message().contains(want), "{line:?}: {e:?}");
            let want_range = matches!(want, "音域外" | "R00") || line.contains("v16");
            assert_eq!(range, want_range, "{line:?}");
        }
    }
}
