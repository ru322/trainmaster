//! 文言の `TextId` とテンプレートの展開。生成物。生成元は `tm-fontgen`(`assets/text/ja.toml`)。

/// 文言の ID。`ja.toml` のキーの辞書順に 0 から振る。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextId(pub u16);

#[allow(missing_docs)]
mod generated {
    use super::TextId;

    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated/texts.rs"));

    pub(super) fn texts() -> &'static [&'static str] {
        &TEXTS
    }

    pub(super) fn keys() -> &'static [&'static str] {
        &KEYS
    }

    pub(super) fn placeholders() -> &'static [&'static [&'static str]] {
        &PLACEHOLDERS
    }
}

pub use generated::{TEXT_COUNT, txt};

/// プレースホルダに差し込む値。
#[derive(Clone, Copy, Debug)]
pub struct TextArg<'a> {
    /// プレースホルダの名前。
    pub name: &'static str,
    /// 差し込む文字列。
    pub value: &'a str,
}

/// テンプレートのままの文言(プレースホルダを含む)。
pub fn text(id: TextId) -> &'static str {
    generated::texts()[usize::from(id.0)]
}

/// `ja.toml` のキー(`settings.continue` など)。デバッグ用。
pub fn key(id: TextId) -> &'static str {
    generated::keys()[usize::from(id.0)]
}

/// 文言のプレースホルダの名前。出てくる順で、重複しない。
pub fn placeholders(id: TextId) -> &'static [&'static str] {
    generated::placeholders()[usize::from(id.0)]
}

/// プレースホルダを埋めて `out` に書く。`out` は先に空にする。
///
/// 足りない引数と余分な引数は、デバッグビルドでは panic、リリースでは「?」と無視。
/// `{btn:…}` は置き換えずに残す。`{{`、`}}` は括弧 1 つ。
pub fn fill_into(id: TextId, args: &[TextArg<'_>], out: &mut String) {
    expand(
        key(id),
        text(id),
        placeholders(id),
        args,
        cfg!(debug_assertions),
        out,
    );
}

fn expand(
    label: &str,
    template: &str,
    known: &[&str],
    args: &[TextArg<'_>],
    strict: bool,
    out: &mut String,
) {
    out.clear();
    for arg in args {
        assert!(
            !strict || known.contains(&arg.name),
            "{label}: 余分な引数 `{}`",
            arg.name
        );
    }
    let mut rest = template;
    while let Some(pos) = rest.find(['{', '}']) {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        let Some(end) = tail.find('}').filter(|_| tail.starts_with('{')) else {
            out.push_str(&tail[..1]);
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..end];
        if name.starts_with("btn:") {
            out.push_str(&tail[..=end]);
        } else if let Some(arg) = args.iter().find(|a| a.name == name) {
            out.push_str(arg.value);
        } else {
            assert!(!strict, "{label}: 足りない引数 `{name}`");
            out.push('?');
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(template: &str, args: &[TextArg<'_>], strict: bool) -> String {
        let mut out = String::new();
        expand("t", template, &["n", "btn:max"], args, strict, &mut out);
        out
    }

    const N3: [TextArg<'static>; 1] = [TextArg {
        name: "n",
        value: "3",
    }];
    const ZZZ: [TextArg<'static>; 1] = [TextArg {
        name: "zzz",
        value: "1",
    }];

    #[test]
    fn doubled_braces_are_literal() {
        assert_eq!(run("{{n}} と }}", &[], true), "{n} と }");
    }

    #[test]
    fn repeated_placeholders_are_all_filled() {
        assert_eq!(run("{n}と{n}と{btn:max}", &N3, true), "3と3と{btn:max}");
    }

    #[test]
    fn lenient_mode_writes_question_mark_and_ignores_extras() {
        assert_eq!(run("{n}", &ZZZ, false), "?");
        assert_eq!(run("{n}", &[], false), "?");
    }

    #[test]
    #[should_panic(expected = "足りない引数")]
    fn strict_mode_panics_on_missing_arg() {
        run("{n}", &[], true);
    }

    #[test]
    #[should_panic(expected = "余分な引数")]
    fn strict_mode_panics_on_extra_arg() {
        run("{n}", &ZZZ, true);
    }
}
