//! 追加のゲームパッドのマッピング(SDL_GameControllerDB の形式)[14 §3.6.2]。
//! 機種が決まるまで空にして SDL の内蔵のマッピングに任せ、会場での追加は `SDL_GAMECONTROLLERCONFIG` で行う(PL-V-07、03 BV8)。

/// 初期化で `add_mapping` する文字列。機種が決まったら SDL_GameControllerDB の Linux の行をそのまま足す。
pub(crate) const PAD_MAPPINGS: &[&str] = &[];

/// ゲームが使うボタン(14 §3.6.2)。マッピングにすべてなければならない。
#[cfg(test)]
pub(crate) const REQUIRED: [&str; 12] = [
    "a",
    "b",
    "x",
    "y",
    "back",
    "start",
    "leftshoulder",
    "rightshoulder",
    "dpup",
    "dpdown",
    "dpleft",
    "dpright",
];

/// 1 行のマッピングの形の検査: 32 桁の 16 進の GUID、名前、`platform:Linux`、`REQUIRED` のボタン。
#[cfg(test)]
pub(crate) fn check(mapping: &str) -> Result<(), String> {
    let mut fields = mapping.trim_end_matches(',').split(',');
    let guid = fields.next().unwrap_or_default();
    if guid.len() != 32 || !guid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("GUID: {guid}"));
    }
    if fields.next().is_none_or(str::is_empty) {
        return Err("名前がない".to_owned());
    }
    let keys: Vec<(&str, &str)> = fields
        .map(|f| f.split_once(':').ok_or_else(|| f.to_owned()))
        .collect::<Result<_, _>>()?;
    if !keys.contains(&("platform", "Linux")) {
        return Err("platform:Linux がない".to_owned());
    }
    match REQUIRED.iter().find(|r| !keys.iter().any(|(k, _)| k == *r)) {
        Some(r) => Err(format!("{r} がない")),
        None => Ok(()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 検査の確認用の架空の行(実在の機種ではない)。
    pub(crate) const SAMPLE: &str = "03000000ffff00000100000000000000,Test Pad,a:b0,b:b1,x:b2,y:b3,back:b6,start:b7,leftshoulder:b4,rightshoulder:b5,dpup:h0.1,dpdown:h0.4,dpleft:h0.8,dpright:h0.2,leftx:a0,lefty:a1,platform:Linux,";

    #[test]
    fn bundled_mappings_are_well_formed() {
        for m in PAD_MAPPINGS {
            assert_eq!(check(m), Ok(()), "{m}");
        }
    }

    #[test]
    fn check_rejects_bad_lines() {
        assert_eq!(check(SAMPLE), Ok(()));
        assert!(check(&SAMPLE.replace("03000000ffff", "zz000000ffff")).is_err());
        assert!(check(&SAMPLE.replace("Test Pad", "")).is_err());
        assert!(check(&SAMPLE.replace("platform:Linux", "platform:Windows")).is_err());
        assert!(check(&SAMPLE.replace("start:b7,", "")).is_err());
        assert!(check(&SAMPLE.replace("dpleft:h0.8,", "dpleft,")).is_err());
    }
}
