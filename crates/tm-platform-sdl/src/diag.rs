//! 描画の問題のログの間引き(14 §3.4、§3.5.5、§3.5.6)。

use crate::render::DrawResult;

const QUAD_LIMIT: u32 = 2000;
const SECOND_US: u64 = 1_000_000;

/// 一度だけ出すログと、デバッグビルドで 1 秒に 1 回まで出すログを決める。
#[derive(Default)]
pub(crate) struct DrawLog {
    bad_sprite: bool,
    bad_layer: bool,
    not_opaque: bool,
    quads_at: Option<u64>,
    recreates_window: Option<(u64, u32)>,
}

impl DrawLog {
    /// このフレームで出す警告。`recreates` は起動からの層の作り直しの回数。
    pub(crate) fn check(
        &mut self,
        r: &DrawResult,
        recreates: u32,
        now: u64,
        debug: bool,
    ) -> Vec<String> {
        let mut out = Vec::new();
        if r.bad_sprite && !self.bad_sprite {
            self.bad_sprite = true;
            out.push("範囲外の SpriteId を描かなかった".to_owned());
        }
        if r.bad_layer && !self.bad_layer {
            self.bad_layer = true;
            out.push("対応のないキャッシュ層のコマンドを無視した".to_owned());
        }
        if !debug {
            return out;
        }
        if r.layer_not_opaque && !self.not_opaque {
            self.not_opaque = true;
            out.push("キャッシュ層の最初のコマンドが不透明な Clear でない(14 §3.5.5)".to_owned());
        }
        if r.quads > QUAD_LIMIT && self.quads_at.is_none_or(|t| now - t >= SECOND_US) {
            self.quads_at = Some(now);
            out.push(format!("四角形が {} 個(上限の目安 {QUAD_LIMIT})", r.quads));
        }
        match self.recreates_window {
            None => self.recreates_window = Some((now, recreates)),
            Some((start, base)) if now - start >= SECOND_US => {
                if recreates - base >= 2 {
                    out.push(format!(
                        "layer_recreates: {recreates}(この 1 秒に {} 回。層の大きさが変わり続けている)",
                        recreates - base
                    ));
                }
                self.recreates_window = Some((now, recreates));
            }
            Some(_) => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(quads: u32) -> DrawResult {
        DrawResult {
            quads,
            error: None,
            bad_sprite: false,
            bad_layer: false,
            broken_layer: false,
            layer_not_opaque: false,
        }
    }

    fn count(logs: &[String], needle: &str) -> usize {
        logs.iter().filter(|l| l.contains(needle)).count()
    }

    #[test]
    fn quad_warning_is_over_2000_and_once_a_second() {
        let mut d = DrawLog::default();
        assert!(d.check(&result(2000), 0, 0, true).is_empty());
        assert_eq!(count(&d.check(&result(2001), 0, 10, true), "四角形"), 1);
        assert!(
            d.check(&result(2001), 0, 10 + SECOND_US - 1, true)
                .is_empty()
        );
        assert_eq!(
            count(&d.check(&result(2001), 0, 10 + SECOND_US, true), "四角形"),
            1
        );
        assert!(
            DrawLog::default()
                .check(&result(9999), 0, 0, false)
                .is_empty()
        );
    }

    #[test]
    fn recreates_are_logged_only_when_the_size_keeps_changing() {
        let mut d = DrawLog::default();
        assert!(d.check(&result(0), 0, 0, true).is_empty());
        assert!(d.check(&result(0), 1, SECOND_US / 2, true).is_empty());
        assert!(
            d.check(&result(0), 1, SECOND_US, true).is_empty(),
            "1 回だけ"
        );
        assert!(d.check(&result(0), 3, SECOND_US + 10, true).is_empty());
        let logs = d.check(&result(0), 3, 2 * SECOND_US, true);
        assert_eq!(count(&logs, "この 1 秒に 2 回"), 1, "{logs:?}");
        assert!(d.check(&result(0), 3, 3 * SECOND_US, true).is_empty());
        let mut r = DrawLog::default();
        r.check(&result(0), 0, 0, false);
        assert!(r.check(&result(0), 50, 2 * SECOND_US, false).is_empty());
    }

    #[test]
    fn one_time_warnings() {
        let mut d = DrawLog::default();
        let mut r = result(0);
        r.bad_sprite = true;
        r.bad_layer = true;
        r.layer_not_opaque = true;
        let logs = d.check(&r, 0, 0, false);
        assert_eq!(logs.len(), 2, "{logs:?}");
        let logs = d.check(&r, 0, 1, true);
        assert_eq!(count(&logs, "不透明"), 1, "{logs:?}");
        assert!(d.check(&r, 0, 2, true).is_empty());
    }
}
