//! 入力のファイルの列挙、`_lib/` の読み込みと結合、名前の検査。

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path};

use serde::de::DeserializeOwned;

use crate::schema::{AUDIO_FORMAT_VERSION, Instrument, LibDef, Pattern, SoundDef};
use crate::validate::{Code, Diagnostic};
use crate::{Fail, IoError, SoundKind};

/// 音のファイル 1 つ。
#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub name: String,
    pub kind: SoundKind,
    pub text: String,
}

/// imports を結合した定義。
#[derive(Clone, Debug)]
pub(crate) struct Merged {
    pub def: SoundDef,
    pub instruments: BTreeMap<String, Instrument>,
    pub patterns: BTreeMap<String, Pattern>,
}

/// E1: `^(se|bgm|jingle)_[a-z0-9_]+$` を手で検査し、接頭辞から種類を決める。
pub(crate) fn kind_of(name: &str) -> Option<SoundKind> {
    let (kind, rest) = [
        ("se_", SoundKind::Se),
        ("bgm_", SoundKind::Bgm),
        ("jingle_", SoundKind::Jingle),
    ]
    .into_iter()
    .find_map(|(prefix, kind)| name.strip_prefix(prefix).map(|r| (kind, r)))?;
    let ok = !rest.is_empty()
        && rest
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_');
    ok.then_some(kind)
}

/// `_` で始まるものと `.ron` 以外を除き、名前の順に読む。名前の誤りは E1 として返す。
pub(crate) fn list_sounds(in_dir: &Path) -> Result<(Vec<Source>, Vec<Diagnostic>), IoError> {
    let io_err = |source| IoError {
        path: in_dir.to_path_buf(),
        source,
    };
    let mut names = Vec::new();
    for entry in fs::read_dir(in_dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            names.push(Err(file_name.to_string_lossy().into_owned()));
            continue;
        };
        if file_name.starts_with('_') || !entry.file_type().map_err(io_err)?.is_file() {
            continue;
        }
        if let Some(stem) = file_name.strip_suffix(".ron") {
            names.push(Ok(stem.to_string()));
        }
    }
    names.sort();
    let mut sources = Vec::new();
    let mut diags = Vec::new();
    for name in names {
        let name = match name {
            Ok(n) => n,
            Err(n) => {
                diags.push(Diagnostic::new(Code::E1, &n, "ファイル名が UTF-8 でない"));
                continue;
            }
        };
        let Some(kind) = kind_of(&name) else {
            diags.push(Diagnostic::new(
                Code::E1,
                &format!("{name}.ron"),
                "名前が ^(se|bgm|jingle)_[a-z0-9_]+$ に合わない",
            ));
            continue;
        };
        let path = in_dir.join(format!("{name}.ron"));
        let text = fs::read_to_string(&path).map_err(|source| IoError { path, source })?;
        sources.push(Source { name, kind, text });
    }
    Ok((sources, diags))
}

fn parse<T: DeserializeOwned>(label: &str, text: &str) -> Result<T, Fail> {
    ron::from_str(text)
        .map_err(|e| Fail::Check(Diagnostic::new(Code::E4, label, &format!("RON: {e}"))))
}

fn check_import_path(p: &str) -> bool {
    let path = Path::new(p);
    let mut comps = path.components();
    let first_ok = matches!(comps.next(), Some(Component::Normal(c)) if c.to_str().is_some_and(|s| s.starts_with('_')));
    first_ok && comps.all(|c| matches!(c, Component::Normal(_))) && p.ends_with(".ron")
}

/// 音のファイルを読み、imports を `base` からの相対パスで読んで結合する。
pub(crate) fn load_sound(base: &Path, label: &str, text: &str) -> Result<Merged, Fail> {
    let def: SoundDef = parse(label, text)?;
    if def.format != AUDIO_FORMAT_VERSION {
        return Err(Fail::Check(Diagnostic::new(
            Code::E4,
            label,
            &format!("format が {}(1 であること)", def.format),
        )));
    }
    let e5 = |msg: String| Fail::Check(Diagnostic::new(Code::E5, label, &msg));
    let mut instruments = BTreeMap::new();
    let mut patterns = BTreeMap::new();
    for import in &def.imports {
        if !check_import_path(import) {
            return Err(e5(format!(
                "imports のパスの誤り(`_` で始まるディレクトリの .ron): {import:?}"
            )));
        }
        let path = base.join(import);
        let lib_text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(e5(format!("imports のファイルがない: {import}")));
            }
            Err(source) => return Err(Fail::Io(IoError { path, source })),
        };
        let lib: LibDef = parse(import, &lib_text)?;
        if lib.format != AUDIO_FORMAT_VERSION {
            return Err(Fail::Check(Diagnostic::new(
                Code::E4,
                import,
                &format!("format が {}(1 であること)", lib.format),
            )));
        }
        merge(&mut instruments, lib.instruments, "音色", import).map_err(e5)?;
        merge(&mut patterns, lib.patterns, "パターン", import).map_err(e5)?;
    }
    merge(&mut instruments, def.instruments.clone(), "音色", label).map_err(e5)?;
    merge(&mut patterns, def.patterns.clone(), "パターン", label).map_err(e5)?;
    Ok(Merged {
        def,
        instruments,
        patterns,
    })
}

fn merge<T>(
    into: &mut BTreeMap<String, T>,
    from: BTreeMap<String, T>,
    what: &str,
    origin: &str,
) -> Result<(), String> {
    for (k, v) in from {
        if into.contains_key(&k) {
            return Err(format!("{what}の名前 {k:?} が重なる({origin})"));
        }
        into.insert(k, v);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e1_name_rule() {
        assert_eq!(kind_of("se_coin"), Some(SoundKind::Se));
        assert_eq!(kind_of("bgm_game_1"), Some(SoundKind::Bgm));
        assert_eq!(kind_of("jingle_goal"), Some(SoundKind::Jingle));
        assert_eq!(kind_of("se__"), Some(SoundKind::Se));
        for bad in [
            "se_",
            "se",
            "sfx_coin",
            "Se_coin",
            "se_Coin",
            "se-coin",
            "se_co-in",
            "bgm",
            "jingle_",
            "se_coin.x",
            "_se_coin",
            "se_ coin",
        ] {
            assert_eq!(kind_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn import_paths() {
        assert!(check_import_path("_lib/instruments.ron"));
        assert!(check_import_path("_lib/sub/a.ron"));
        assert!(!check_import_path("lib/a.ron"));
        assert!(!check_import_path("_lib/../a.ron"));
        assert!(!check_import_path("/_lib/a.ron"));
        assert!(!check_import_path("_lib/a.txt"));
    }
}
