//! `resume` のための写しの読み込み: セーブファイル(10 §9.4。1 行目がヘッダ)か、`GameSnapshot` だけの RON。

use serde::Deserialize;
use tm_core::{GameSnapshot, StateHasher};

#[derive(Deserialize)]
#[serde(rename = "SaveHeader")]
struct SaveHeader {
    magic: String,
    body_fnv: u64,
}

#[derive(Deserialize)]
#[serde(rename = "SaveBody")]
struct SaveBody {
    game: GameSnapshot,
}

/// 読み込んだ写し。
#[derive(Debug)]
pub struct Loaded {
    /// ゲームの写し。
    pub snapshot: GameSnapshot,
    /// 読めたが確かめられなかったこと(`body_fnv` の不一致など)。
    pub warnings: Vec<String>,
}

/// セーブファイルか `GameSnapshot` の RON を読む。チュートリアルの進みは読み飛ばす。
pub fn read_snapshot(text: &str) -> Result<Loaded, String> {
    if let Ok(snapshot) = ron::from_str::<GameSnapshot>(text) {
        return Ok(Loaded {
            snapshot,
            warnings: Vec::new(),
        });
    }
    let (first, body) = text
        .split_once('\n')
        .ok_or("写しでもセーブファイルでもない")?;
    let header: SaveHeader =
        ron::from_str(first).map_err(|e| format!("セーブファイルのヘッダ: {e}"))?;
    if header.magic != "TMSAVE" {
        return Err(format!("magic {:?}", header.magic));
    }
    let mut warnings = Vec::new();
    let mut h = StateHasher::new();
    h.write_bytes(body.as_bytes());
    if h.finish() != header.body_fnv {
        warnings.push(format!(
            "body_fnv {:016x} が本体の {:016x} と違う",
            header.body_fnv,
            h.finish()
        ));
    }
    let body: SaveBody = ron::from_str(body).map_err(|e| format!("セーブファイルの本体: {e}"))?;
    Ok(Loaded {
        snapshot: body.game,
        warnings,
    })
}

#[cfg(test)]
mod tests;
