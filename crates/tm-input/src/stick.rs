//! 左スティックの 4 方向化 [13 §2.5]。

use tm_platform_api::PadAxis;

use crate::Action;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub(crate) fn action(self) -> Action {
        match self {
            Dir::Up => Action::MoveUp,
            Dir::Down => Action::MoveDown,
            Dir::Left => Action::MoveLeft,
            Dir::Right => Action::MoveRight,
        }
    }
}

/// 1 台のパッドの左スティックの状態。
#[derive(Clone, Debug, Default)]
pub(crate) struct StickFilter {
    x: i32,
    y: i32,
    cur: Option<Dir>,
    suppressed: bool,
}

impl StickFilter {
    pub(crate) fn dir(&self) -> Option<Dir> {
        self.cur
    }

    /// 軸の値を取り込み、方向を判定し直す。右スティックとトリガーは無視する。
    pub(crate) fn feed(&mut self, axis: PadAxis, value: i16, threshold: i16, release: i16) {
        match axis {
            PadAxis::LeftX => self.x = i32::from(value),
            PadAxis::LeftY => self.y = i32::from(value),
            _ => return,
        }
        self.update(i32::from(threshold), i32::from(release));
    }

    /// 中立に戻るまで方向を出さない。
    pub(crate) fn suppress(&mut self) {
        if self.cur.take().is_some() {
            self.suppressed = true;
        }
    }

    fn update(&mut self, threshold: i32, release: i32) {
        let (ax, ay) = (self.x.abs(), self.y.abs());
        if self.suppressed {
            if ax.max(ay) < release {
                self.suppressed = false;
            }
            return;
        }
        if let Some(cur) = self.cur {
            let (along, other, other_dir) = match cur {
                Dir::Up => (-self.y, ax, horizontal(self.x)),
                Dir::Down => (self.y, ax, horizontal(self.x)),
                Dir::Left => (-self.x, ay, vertical(self.y)),
                Dir::Right => (self.x, ay, vertical(self.y)),
            };
            if along >= release {
                if other >= threshold && other > along {
                    self.cur = Some(other_dir);
                }
                return;
            }
            self.cur = None;
        }
        if ax.max(ay) >= threshold {
            self.cur = Some(if ax >= ay {
                horizontal(self.x)
            } else {
                vertical(self.y)
            });
        }
    }
}

fn horizontal(x: i32) -> Dir {
    if x < 0 { Dir::Left } else { Dir::Right }
}

// SDL は y が負で上。
fn vertical(y: i32) -> Dir {
    if y < 0 { Dir::Up } else { Dir::Down }
}
