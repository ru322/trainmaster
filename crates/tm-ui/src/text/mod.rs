//! テキストのレイアウト、禁則、プレースホルダ、固定文のキャッシュ [13 §4.4、15 §4.10]。

pub(crate) mod numbers;

use std::collections::BTreeMap;

use tm_assets::font::{self, FontId, Glyph};
use tm_assets::sprite;
use tm_assets::text::{self, TextArg, TextId};
use tm_input::InputDevice;
use tm_platform_api::SpriteId;

/// 行頭に来てはいけない字(行頭禁則。13 §4.4)。
const NO_LINE_START: &str = "、。」』)!?ー〜…ぁぃぅぇぉっゃゅょァィゥェォッャュョ";
/// 行末に来てはいけない字(行末禁則。13 §4.4)。
const NO_LINE_END: &str = "「『(";
/// ボタンの絵の大きさ(送り幅も同じ)[15 §4.10]。
const BUTTON: u16 = 16;
/// 固定文のキャッシュの上限 [13 §4.4]。
const TEXT_CACHE_MAX: usize = 512;

/// 並べた 1 字(またはボタンの絵)。座標はブロックの左上から。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GlyphPos {
    pub sprite: SpriteId,
    pub x: i16,
    pub y: i16,
    pub w: u8,
    pub h: u8,
    /// true ならボタンの絵(文字の色で tint しない)。
    pub plain: bool,
}

/// レイアウトの結果。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TextBlock {
    pub glyphs: Vec<GlyphPos>,
    /// いちばん長い行の幅。
    pub width: u16,
    /// 行の数。
    pub lines: u8,
    /// 行送り。
    pub line_height: u8,
    /// 幅を超えて自動で折り返した行があれば true(`\n` だけの改行では false)。
    pub wrapped: bool,
}

impl TextBlock {
    /// 高さ(行の数 × 行送り)。
    pub(crate) fn height(&self) -> i32 {
        i32::from(self.lines) * i32::from(self.line_height)
    }

    /// 寸法をすべて整数 `k` 倍にする(最近傍で描く拡大。15 §4.10)。レイアウトの後に 1 回だけ呼ぶ。
    pub(crate) fn scale(&mut self, k: u8) {
        let k16 = i16::from(k);
        for g in &mut self.glyphs {
            g.x = g.x.saturating_mul(k16);
            g.y = g.y.saturating_mul(k16);
            g.w = g.w.saturating_mul(k);
            g.h = g.h.saturating_mul(k);
        }
        self.width = self.width.saturating_mul(u16::from(k));
        self.line_height = self.line_height.saturating_mul(k);
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Glyph(Glyph),
    Button(SpriteId),
    Space,
    Newline,
}

/// 並べる前の 1 項目。`glue` は前の項目との間で切らない印。
#[derive(Clone, Copy)]
struct Item {
    kind: Kind,
    adv: u16,
    glue: bool,
}

impl Item {
    fn new(kind: Kind) -> Self {
        let adv = match kind {
            Kind::Glyph(g) => u16::from(g.advance),
            Kind::Button(_) => BUTTON,
            Kind::Space | Kind::Newline => 0,
        };
        Self {
            kind,
            adv,
            glue: false,
        }
    }

    fn ch(self) -> Option<char> {
        match self.kind {
            Kind::Glyph(g) => Some(g.ch),
            _ => None,
        }
    }

    fn hangs(self) -> bool {
        self.ch().is_some_and(|c| NO_LINE_START.contains(c))
    }

    fn is_content(self) -> bool {
        matches!(self.kind, Kind::Glyph(_) | Kind::Button(_))
    }
}

/// 英数字の連続、ボタンの絵とその次の 1 字、行末禁則の字とその次の間に `glue` を付ける。
fn link(items: &mut [Item]) {
    for i in 1..items.len() {
        let (prev, cur) = (items[i - 1], items[i]);
        if !cur.is_content() {
            continue;
        }
        let alnum = |it: Item| it.ch().is_some_and(|c| c.is_ascii_alphanumeric());
        if (alnum(prev) && alnum(cur))
            || (matches!(prev.kind, Kind::Button(_)) && cur.ch().is_some())
            || prev.ch().is_some_and(|c| NO_LINE_END.contains(c))
        {
            items[i].glue = true;
        }
    }
}

/// 各行の最初の項目の位置を `starts` に入れる(先頭の 0 を含む)。自動で折り返したら true。
/// 幅を超えたら行の最後の空白で、なければ最後の切ってよい字の間で切る(13 §4.4)。
fn break_lines(items: &[Item], max_w: u16, starts: &mut Vec<usize>) -> bool {
    starts.clear();
    starts.push(0);
    let mut wrapped = false;
    let (mut x, mut pending) = (0u16, 0u16);
    let mut first: Option<usize> = None;
    let mut space: Option<usize> = None;
    let mut i = 0;
    while i < items.len() {
        let it = items[i];
        match it.kind {
            Kind::Newline => {
                i += 1;
                starts.push(i);
                (x, pending, first, space) = (0, 0, None, None);
            }
            Kind::Space => {
                if first.is_some() {
                    pending = pending.saturating_add(it.adv);
                    space = Some(i);
                }
                i += 1;
            }
            Kind::Glyph(_) | Kind::Button(_) => {
                let end = x.saturating_add(pending).saturating_add(it.adv);
                if let Some(f) = first.filter(|_| end > max_w && (space.is_some() || !it.hangs())) {
                    let next = space.map_or_else(
                        || {
                            (f + 1..=i)
                                .rev()
                                .find(|&b| !items[b].glue && !items[b].hangs())
                                // 切れる所がなければ glue より幅を守る
                                .unwrap_or(i)
                        },
                        |s| s + 1,
                    );
                    starts.push(next);
                    wrapped = true;
                    (x, pending, first, space) = (0, 0, None, None);
                    i = next;
                    continue;
                }
                (x, pending) = (end, 0);
                first.get_or_insert(i);
                i += 1;
            }
        }
    }
    wrapped
}

/// `{btn:<名前>}` の絵。機器に合わせる(13 §4.4、UI-D-19)。
pub(crate) fn button_sprite(btn: &str, dev: InputDevice) -> Option<SpriteId> {
    use sprite::*;
    let (key, pad) = match btn {
        "confirm" => (BTN_KEY_Z, BTN_PAD_A),
        "cancel" => (BTN_KEY_X, BTN_PAD_B),
        "max" => (BTN_KEY_C, BTN_PAD_Y),
        "tab" => (BTN_KEY_E, BTN_PAD_RB),
        "build" => (BTN_KEY_B, BTN_PAD_X),
        "bend" => (BTN_KEY_TAB, BTN_PAD_R3),
        "recommend" => (BTN_KEY_R, BTN_PAD_BACK),
        "menu" => (BTN_KEY_ESC, BTN_PAD_START),
        "move" => (BTN_KEY_ARROWS, BTN_PAD_DPAD),
        "lr" => (BTN_KEY_LR, BTN_PAD_LR),
        _ => return None,
    };
    Some(match dev {
        InputDevice::Keyboard => key,
        InputDevice::Gamepad => pad,
    })
}

fn glyph_or_fallback(size: FontId, ch: char) -> Glyph {
    font::glyph(size, ch).unwrap_or_else(|| {
        debug_assert!(false, "フォントにない字 {ch:?}");
        font::fallback_glyph(size)
    })
}

fn push_char(items: &mut Vec<Item>, size: FontId, ch: char, space: u16) {
    items.push(match ch {
        '\n' => Item::new(Kind::Newline),
        ' ' => Item {
            adv: space,
            ..Item::new(Kind::Space)
        },
        _ => Item::new(Kind::Glyph(glyph_or_fallback(size, ch))),
    });
}

fn expand(src: &str, size: FontId, args: &[TextArg<'_>], dev: InputDevice, items: &mut Vec<Item>) {
    let space = font::glyph(size, ' ').map_or(u16::from(font::face(size).full_advance) / 2, |g| {
        u16::from(g.advance)
    });
    let mut rest = src;
    while let Some(pos) = rest.find(['{', '}']) {
        rest[..pos]
            .chars()
            .for_each(|c| push_char(items, size, c, space));
        let tail = &rest[pos..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            push_char(items, size, tail.as_bytes()[0] as char, space);
            rest = &tail[2..];
            continue;
        }
        let Some(end) = tail.find('}').filter(|_| tail.starts_with('{')) else {
            push_char(items, size, tail.as_bytes()[0] as char, space);
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..end];
        if let Some(btn) = name.strip_prefix("btn:") {
            match button_sprite(btn, dev) {
                Some(s) => items.push(Item::new(Kind::Button(s))),
                None => {
                    debug_assert!(false, "ボタンの名前 {btn:?} がない");
                    push_char(items, size, '?', space);
                }
            }
        } else if let Some(arg) = args.iter().find(|a| a.name == name) {
            let from = items.len() + 1;
            arg.value
                .chars()
                .for_each(|c| push_char(items, size, c, space));
            if let Some(value) = items.get_mut(from..) {
                value.iter_mut().for_each(|it| it.glue = true);
            }
        } else {
            debug_assert!(false, "プレースホルダ {name:?} の値がない");
            push_char(items, size, '?', space);
        }
        rest = &tail[end + 1..];
    }
    rest.chars().for_each(|c| push_char(items, size, c, space));
}

fn place(items: &[Item], starts: &[usize], size: FontId, out: &mut TextBlock) {
    let face = font::face(size);
    let (mut line, mut next) = (0i32, 1usize);
    let (mut x, mut pending, mut content) = (0u16, 0u16, false);
    for (i, &it) in items.iter().enumerate() {
        while starts.get(next) == Some(&i) {
            (line, next) = (line + 1, next + 1);
            (x, pending, content) = (0, 0, false);
        }
        let top = line * i32::from(face.line_height);
        match it.kind {
            Kind::Space => {
                if content {
                    pending = pending.saturating_add(it.adv);
                }
                continue;
            }
            Kind::Newline => continue,
            Kind::Glyph(g) => {
                x = x.saturating_add(pending);
                if let Some(b) = g.bitmap {
                    out.glyphs.push(GlyphPos {
                        sprite: b.sprite,
                        x: (i32::from(x) + i32::from(b.x_off)) as i16,
                        y: (top + i32::from(face.ascent) + i32::from(b.y_off)) as i16,
                        w: b.w,
                        h: b.h,
                        plain: false,
                    });
                }
            }
            Kind::Button(s) => {
                x = x.saturating_add(pending);
                let pad = (i32::from(face.line_height) - i32::from(BUTTON)) / 2;
                out.glyphs.push(GlyphPos {
                    sprite: s,
                    x: x as i16,
                    y: (top + pad) as i16,
                    w: BUTTON as u8,
                    h: BUTTON as u8,
                    plain: true,
                });
            }
        }
        x = x.saturating_add(it.adv);
        (pending, content) = (0, true);
        out.width = out.width.max(x);
    }
}

/// 文字列を幅 `max_w` に収めて並べる。`out` は空にしてから書く。
///
/// 改行は `\n` と、幅を超えたときの空白か字の間(禁則と切らない並びを守る。13 §4.4)。行頭の空白は描かない。
pub(crate) fn layout(
    src: &str,
    size: FontId,
    max_w: u16,
    args: &[TextArg<'_>],
    dev: InputDevice,
    out: &mut TextBlock,
) {
    out.glyphs.clear();
    out.width = 0;
    out.line_height = font::face(size).line_height;
    let mut items = Vec::with_capacity(src.len());
    expand(src, size, args, dev, &mut items);
    link(&mut items);
    let mut starts = Vec::new();
    out.wrapped = break_lines(&items, max_w, &mut starts);
    place(&items, &starts, size, out);
    out.lines = if items.iter().any(|it| !matches!(it.kind, Kind::Space)) {
        u8::try_from(starts.len()).unwrap_or(u8::MAX)
    } else {
        0
    };
}

/// 固定文(`{btn:…}` 以外のプレースホルダがない文)のレイアウトのキャッシュ [13 §4.4]。
#[derive(Default)]
pub(crate) struct TextCache {
    index: BTreeMap<(u16, u8, u16, u8), u32>,
    blocks: Vec<TextBlock>,
}

/// `TextCache` の中のブロックへの参照。同じフレームの `update` の中だけで有効。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextRef(u32);

fn size_key(size: FontId) -> u8 {
    match size {
        FontId::Body12 => 0,
        FontId::Head16 => 1,
    }
}

fn device_key(dev: InputDevice) -> u8 {
    match dev {
        InputDevice::Keyboard => 0,
        InputDevice::Gamepad => 1,
    }
}

impl TextCache {
    /// フレームの最初に呼ぶ。上限を超えていたら全部捨てる(フレームの途中では捨てない)。
    pub(crate) fn begin_frame(&mut self) {
        if self.blocks.len() >= TEXT_CACHE_MAX {
            self.index.clear();
            self.blocks.clear();
        }
    }

    /// 固定文のレイアウト。なければ作る。
    pub(crate) fn get(
        &mut self,
        id: TextId,
        size: FontId,
        max_w: u16,
        dev: InputDevice,
    ) -> TextRef {
        debug_assert!(
            text::placeholders(id).iter().all(|p| p.starts_with("btn:")),
            "{} は固定文ではない",
            text::key(id)
        );
        let key = (id.0, size_key(size), max_w, device_key(dev));
        if let Some(&i) = self.index.get(&key) {
            return TextRef(i);
        }
        let mut block = TextBlock::default();
        layout(text::text(id), size, max_w, &[], dev, &mut block);
        let i = self.blocks.len() as u32;
        self.blocks.push(block);
        self.index.insert(key, i);
        TextRef(i)
    }

    /// 参照のブロック。
    pub(crate) fn block(&self, r: TextRef) -> &TextBlock {
        &self.blocks[r.0 as usize]
    }

    /// 入っている数。
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.blocks.len()
    }
}

/// プレースホルダのある文 1 つ分。値が変わったときだけレイアウトし直す。
#[derive(Default)]
pub(crate) struct DynText {
    id: Option<TextId>,
    key: String,
    params: (u8, u16, u8),
    scratch: String,
    pub block: TextBlock,
    #[cfg(test)]
    relayouts: u32,
}

impl DynText {
    /// 文と値を決める。前回と同じなら何もしない。変わったら true。
    pub(crate) fn set(
        &mut self,
        id: TextId,
        args: &[TextArg<'_>],
        size: FontId,
        max_w: u16,
        dev: InputDevice,
    ) -> bool {
        self.scratch.clear();
        for a in args {
            self.scratch.push_str(a.name);
            self.scratch.push('\u{1}');
            self.scratch.push_str(a.value);
            self.scratch.push('\u{1}');
        }
        let params = (size_key(size), max_w, device_key(dev));
        if self.id == Some(id) && self.params == params && self.key == self.scratch {
            return false;
        }
        self.id = Some(id);
        self.params = params;
        std::mem::swap(&mut self.key, &mut self.scratch);
        layout(text::text(id), size, max_w, args, dev, &mut self.block);
        #[cfg(test)]
        {
            self.relayouts += 1;
        }
        true
    }

    /// いまの文。
    pub(crate) fn id(&self) -> Option<TextId> {
        self.id
    }

    /// レイアウトし直した回数(テスト用)。
    #[cfg(test)]
    pub(crate) fn relayouts(&self) -> u32 {
        self.relayouts
    }
}

#[cfg(test)]
mod break_tests;
#[cfg(test)]
mod tests;
