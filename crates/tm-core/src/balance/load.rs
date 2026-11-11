use super::{BalanceError, BalanceSpec, BalanceTable};

/// 埋め込みの `data/balance.ron`(01 の初期値)。
pub const BUILTIN_BALANCE_RON: &str = include_str!("../../data/balance.ron");

impl BalanceTable {
    /// RON の文字列から読んで検証する。
    pub fn from_ron_str(src: &str) -> Result<Self, BalanceError> {
        let spec: BalanceSpec = ron::from_str(src).map_err(|e| BalanceError::Parse {
            message: e.to_string(),
        })?;
        BalanceTable::from_spec(spec)
    }

    /// 埋め込みの表。検証に通らなければ panic する(CI のテストで保証)。
    pub fn builtin() -> Self {
        BalanceTable::from_ron_str(BUILTIN_BALANCE_RON)
            .unwrap_or_else(|e| panic!("埋め込みの balance.ron が不正: {e}"))
    }
}
