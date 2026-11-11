//! キャッシュ層の区間の前処理と、層のテクスチャ [14 §3.5]。

use sdl2::render::Texture;
use tm_platform_api::{DrawCmd, Rect};

/// 層の数(`LayerId(0..=3)`)。
pub(crate) const LAYER_COUNT: usize = 4;
/// 1 層の幅と高さの上限。
pub(crate) const LAYER_MAX_DIM: u32 = 1024;
/// 全層のテクスチャの合計の上限(バイト)。
pub(crate) const LAYER_BUDGET_BYTES: u64 = 4 * 1024 * 1024;

/// このフレームの層の区間。`begin` と `end` は Begin と End の添字。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) begin: usize,
    pub(crate) end: usize,
    pub(crate) revision: u64,
    pub(crate) rect: Rect,
}

/// `DrawCachedLayer` での描き方。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerUse {
    None,
    Cached(Rect),
    Inline(Section),
}

/// 層のテクスチャと、最後に描いた revision。
pub(crate) struct LayerSlot<'tc> {
    pub(crate) tex: Texture<'tc>,
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) revision: Option<u64>,
}

/// `begin` の Begin の区間の終わり(深さが 0 に戻る End、なければ列の最後)と、入れ子のない閉じた区間か(14 §3.5.5)。
pub(crate) fn interval(cmds: &[DrawCmd], begin: usize) -> (usize, bool) {
    let mut depth = 1u32;
    let mut nested = false;
    for (j, cmd) in cmds.iter().enumerate().skip(begin + 1) {
        match cmd {
            DrawCmd::BeginCachedLayer { .. } => {
                depth += 1;
                nested = true;
            }
            DrawCmd::EndCachedLayer => {
                depth -= 1;
                if depth == 0 {
                    return (j, !nested);
                }
            }
            _ => {}
        }
    }
    (cmds.len() - 1, false)
}

/// 前処理で見つけた不正な区間。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlanIssues {
    /// 同じ層の 2 回目の Begin、範囲外の `LayerId`。
    pub(crate) ignored: bool,
    /// 入れ子の Begin、End のない Begin(デバッグビルドでは `debug_assert!`)。
    pub(crate) broken: bool,
}

/// 層ごとの、このフレームの最初の正しい区間。
pub(crate) fn plan(cmds: &[DrawCmd], issues: &mut PlanIssues) -> [Option<Section>; LAYER_COUNT] {
    let mut sections = [None; LAYER_COUNT];
    let mut i = 0;
    while i < cmds.len() {
        if let DrawCmd::BeginCachedLayer {
            layer,
            revision,
            x,
            y,
            w,
            h,
        } = cmds[i]
        {
            let (end, ok) = interval(cmds, i);
            if !ok {
                issues.broken = true;
            } else {
                match sections.get_mut(usize::from(layer.0)) {
                    Some(slot @ None) => {
                        *slot = Some(Section {
                            begin: i,
                            end,
                            revision,
                            rect: Rect { x, y, w, h },
                        })
                    }
                    _ => issues.ignored = true,
                }
            }
            i = end;
        }
        i += 1;
    }
    sections
}

/// 層の最初のコマンドが不透明な `Clear` か(14 §3.5.5 の約束)。
pub(crate) fn starts_opaque(cmds: &[DrawCmd], section: &Section) -> bool {
    matches!(cmds.get(section.begin + 1), Some(DrawCmd::Clear(c)) if c.a == 255)
}

/// 層を置く位置。`w` か `h` が 0 なら `None`。
pub(crate) fn placed(section: &Section) -> Option<Rect> {
    (section.rect.w > 0 && section.rect.h > 0).then_some(section.rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_platform_api::{Color, LayerId};

    fn begin(layer: u8, revision: u64, w: u32) -> DrawCmd {
        DrawCmd::BeginCachedLayer {
            layer: LayerId(layer),
            revision,
            x: -5,
            y: 44,
            w,
            h: 288,
        }
    }

    const CLEAR: DrawCmd = DrawCmd::Clear(Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    });

    fn plan_ok(cmds: &[DrawCmd]) -> ([Option<Section>; LAYER_COUNT], PlanIssues) {
        let mut issues = PlanIssues::default();
        let p = plan(cmds, &mut issues);
        (p, issues)
    }

    #[test]
    fn plan_finds_sections() {
        let cmds = [
            CLEAR,
            begin(0, 7, 512),
            CLEAR,
            DrawCmd::EndCachedLayer,
            begin(2, 1, 0),
            DrawCmd::EndCachedLayer,
            DrawCmd::DrawCachedLayer { layer: LayerId(0) },
        ];
        let (p, issues) = plan_ok(&cmds);
        assert_eq!(issues, PlanIssues::default());
        assert_eq!(
            p[0],
            Some(Section {
                begin: 1,
                end: 3,
                revision: 7,
                rect: Rect {
                    x: -5,
                    y: 44,
                    w: 512,
                    h: 288
                }
            })
        );
        assert_eq!(p[1], None);
        assert_eq!(placed(&p[2].unwrap()), None);
        assert!(placed(&p[0].unwrap()).is_some());
    }

    #[test]
    fn plan_ignores_duplicate_and_out_of_range() {
        let (p, issues) = plan_ok(&[
            begin(0, 1, 8),
            DrawCmd::EndCachedLayer,
            begin(0, 2, 8),
            DrawCmd::EndCachedLayer,
            begin(4, 1, 8),
            DrawCmd::EndCachedLayer,
        ]);
        assert_eq!(
            issues,
            PlanIssues {
                ignored: true,
                broken: false
            }
        );
        assert_eq!(p[0].map(|s| s.revision), Some(1));
        assert!(p[1..].iter().all(Option::is_none));
    }

    #[test]
    fn nested_interval_runs_to_the_matching_end() {
        let end = DrawCmd::EndCachedLayer;
        let cmds = [
            begin(0, 1, 8),
            CLEAR,
            begin(1, 1, 8),
            CLEAR,
            end,
            CLEAR,
            end,
            begin(2, 1, 8),
            end,
            end,
        ];
        assert_eq!(interval(&cmds, 0), (6, false));
        assert_eq!(interval(&cmds, 2), (4, true));
        assert_eq!(interval(&cmds, 7), (8, true));
        let (p, issues) = plan_ok(&cmds);
        assert_eq!(
            issues,
            PlanIssues {
                ignored: false,
                broken: true
            }
        );
        assert_eq!(p[0], None);
        assert_eq!(p[1], None);
        assert_eq!(p[2].map(|s| (s.begin, s.end)), Some((7, 8)));
    }

    #[test]
    fn unclosed_interval_runs_to_the_last_command() {
        let cmds = [CLEAR, begin(0, 1, 8), CLEAR, begin(1, 1, 8), CLEAR];
        assert_eq!(interval(&cmds, 1), (4, false));
        assert_eq!(interval(&cmds, 3), (4, false));
        let (p, issues) = plan_ok(&cmds);
        assert!(issues.broken);
        assert!(p.iter().all(Option::is_none));
    }
}
