//! マップの静的な層(キャッシュ)と動的な部分 [13 §3.11、§3.11.1、15 §5.8]。

use tm_assets::sprite::{self, BandVariant};
use tm_core::{
    Dir8, GameView, GridPos, LineId, MapPoint, POS_UNITS_PER_TILE, StationId, TrackSegment,
};
use tm_platform_api::{DrawCmd, LayerId, MonoTime, Rect};

use super::palette::{self, line_color};
use super::{DrawBuilder, Shed, map_sprite_cmd, rect};
use crate::layout::{
    Cam, LAYER_COUNT, LAYER_W, MAP_COLS, MAP_ROWS, MAP_SCALE, MAP_X, MAP_Y, TILE, WORLD_H,
    near_view, tile_center, tile_layer,
};
use crate::text::numbers::NumText;

/// 静的なマップ層の 1 枚目の番号。縦に切った 2 枚目以降は続きの番号。
pub(crate) const MAP_LAYER: LayerId = LayerId(0);
/// 1 つの辺に描く路線の最大数(4 本目以降は描かない)。
const MAX_LANES: usize = 3;
/// 点で見せる人数の上限。これより多いと人数の札を出す [01 §7.1]。
const DOTS_MAX: u64 = 10;

/// 待っている人数の点の絵(駅で待てる人数が 10 人のとき)と、人数の札を出すか。
pub(crate) fn waiting_marks(n: u64) -> (Option<tm_platform_api::SpriteId>, bool) {
    (waiting_dots(n, DOTS_MAX), n > DOTS_MAX)
}

/// 待っている人の点の絵。点の数は駅で待てる人数 `capacity` に対する割合(切り上げ、1〜10 個)で、
/// 待てる人数以上なら 10 個。待てる人数が 10 人なら 1 人 1 個(M8)。
pub(crate) fn waiting_dots(n: u64, capacity: u64) -> Option<tm_platform_api::SpriteId> {
    let k = (u128::from(n) * u128::from(DOTS_MAX)).div_ceil(u128::from(capacity.max(1)));
    (n > 0).then(|| sprite::PAX_DOTS[k.min(u128::from(DOTS_MAX)) as usize])
}

/// 辺 = 隣り合う 2 マスの間の区間。`dir` が E、SE、S、SW になる側のマスで表す。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EdgeKey {
    tile: GridPos,
    dir: Dir8,
}

fn dir_between(from: GridPos, to: GridPos) -> Dir8 {
    Dir8::from_delta(to.x - from.x, to.y - from.y)
}

impl EdgeKey {
    /// `a` と `b`(隣り合うマス)の間の辺。
    pub(crate) fn between(a: GridPos, b: GridPos) -> EdgeKey {
        let d = dir_between(a, b);
        match d {
            Dir8::E | Dir8::SE | Dir8::S | Dir8::SW => EdgeKey { tile: a, dir: d },
            _ => EdgeKey {
                tile: b,
                dir: d.opposite(),
            },
        }
    }
}

/// 辺 → その辺を通る路線(`LineId` の昇順、最大 3)。
#[derive(Debug, Default)]
pub(crate) struct EdgeLanes {
    entries: Vec<(EdgeKey, [LineId; MAX_LANES], u8)>,
}

impl EdgeLanes {
    /// 路線ごとのマスの列から作る。
    pub(crate) fn build(paths: &[(LineId, &[GridPos])]) -> EdgeLanes {
        let mut pairs: Vec<(EdgeKey, LineId)> = paths
            .iter()
            .flat_map(|&(l, p)| p.windows(2).map(move |w| (EdgeKey::between(w[0], w[1]), l)))
            .collect();
        pairs.sort();
        pairs.dedup();
        let mut entries: Vec<(EdgeKey, [LineId; MAX_LANES], u8)> = Vec::new();
        for (k, l) in pairs {
            match entries.last_mut() {
                Some((lk, lines, n)) if *lk == k => {
                    if usize::from(*n) < MAX_LANES {
                        lines[usize::from(*n)] = l;
                        *n += 1;
                    }
                }
                _ => entries.push((k, [l; MAX_LANES], 1)),
            }
        }
        EdgeLanes { entries }
    }

    /// 路線がその辺で使うレーン(0、1、2)。描かない路線は `None`。
    pub(crate) fn lane(&self, a: GridPos, b: GridPos, line: LineId) -> Option<u8> {
        let k = EdgeKey::between(a, b);
        let i = self.entries.binary_search_by(|e| e.0.cmp(&k)).ok()?;
        let (_, lines, n) = &self.entries[i];
        let pos = lines[..usize::from(*n)].iter().position(|&l| l == line)?;
        Some(match (n, pos) {
            (1, _) => 1,
            (2, 0) => 0,
            (2, _) => 2,
            (_, p) => p as u8,
        })
    }
}

/// 帯の半区間 1 つ。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BandPiece {
    pub tile: GridPos,
    pub dir: Dir8,
    pub line: LineId,
    pub variant: BandVariant,
}

fn variant(lane: u8, parallel: bool) -> BandVariant {
    match (lane, parallel) {
        (1, _) => BandVariant::Center,
        (0, true) => BandVariant::Lane0,
        (0, false) => BandVariant::Lane0Conv,
        (_, true) => BandVariant::Lane2,
        (_, false) => BandVariant::Lane2Conv,
    }
}

/// 帯の半区間を、描く順(マスの行優先 → `Dir8` の順 → `LineId` の昇順)に並べる。
pub(crate) fn band_pieces(paths: &[(LineId, &[GridPos])], lanes: &EdgeLanes) -> Vec<BandPiece> {
    let mut out = Vec::new();
    for &(line, path) in paths {
        for (i, &t) in path.iter().enumerate() {
            let prev = i.checked_sub(1).map(|j| path[j]);
            let next = path.get(i + 1).copied();
            let half =
                |other: Option<GridPos>| other.map(|o| (dir_between(t, o), lanes.lane(t, o, line)));
            let (hp, hn) = (half(prev), half(next));
            let parallel = match (hp, hn) {
                (Some((dp, Some(lp))), Some((dn, Some(ln)))) => dp == dn.opposite() && lp == ln,
                _ => false,
            };
            for (dir, lane) in [hp, hn].into_iter().flatten() {
                if let Some(lane) = lane {
                    out.push(BandPiece {
                        tile: t,
                        dir,
                        line,
                        variant: variant(lane, parallel),
                    });
                }
            }
        }
    }
    out.sort_by_key(|p| (p.tile.y, p.tile.x, p.dir, p.line));
    out.dedup();
    out
}

/// 路線の線路を区間の順につないだマスの列(駅のマスは 1 回だけ)。
pub(crate) fn line_path(segments: &[TrackSegment], out: &mut Vec<GridPos>) {
    out.clear();
    for s in segments {
        for &t in &s.tiles {
            if out.last() != Some(&t) {
                out.push(t);
            }
        }
    }
}

/// `map_revision` ごとに作り直すもの: 層の中身、路線のマスの列、マス → 路線。
#[derive(Debug, Default)]
pub(crate) struct MapCache {
    /// 中身を作ったときの `map_revision`。作り直すかどうかの判定に使う。
    pub revision: u64,
    /// 層の版。`BeginCachedLayer` に渡す。リセットをまたいでも同じ値を使わない。
    pub layer_version: u64,
    /// マップ全体の層の中身(層の座標。テストで見る)。
    #[cfg(test)]
    pub layer: Vec<DrawCmd>,
    /// `layer` を幅 `LAYER_W` ごとに縦に切った中身(それぞれの層の座標)。描くのはこちら。
    pub chunks: [Vec<DrawCmd>; LAYER_COUNT],
    /// `LineId` の添字ごとのマスの列。
    pub paths: Vec<Vec<GridPos>>,
    /// 出現の演出の間で、層に入れなかった駅。
    pub hidden: Vec<StationId>,
    line_tiles: Vec<u8>,
    width: i32,
}

impl MapCache {
    /// 今の盤面から作る。`layer_version` は作るたびに増える番号。`hidden` の駅は層に入れない。
    pub(crate) fn build(
        view: GameView<'_>,
        layer_version: u64,
        hidden: Vec<StationId>,
    ) -> MapCache {
        let (w, h) = view.map_size();
        let mut paths: Vec<Vec<GridPos>> = Vec::with_capacity(view.lines().len());
        for l in view.lines() {
            let mut p = Vec::new();
            line_path(l.segments, &mut p);
            paths.push(p);
        }
        let mut line_tiles = vec![0u8; usize::from(w) * usize::from(h)];
        for (i, p) in paths.iter().enumerate() {
            for t in p {
                if let Some(cell) = line_tiles.get_mut(t.y as usize * usize::from(w) + t.x as usize)
                {
                    *cell |= 1u8.checked_shl(i as u32).unwrap_or(0);
                }
            }
        }
        let refs: Vec<(LineId, &[GridPos])> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| (LineId::from_index(i), p.as_slice()))
            .collect();
        let lanes = EdgeLanes::build(&refs);
        let pieces = band_pieces(&refs, &lanes);
        let mut layer = Vec::with_capacity(1024);
        build_layer(view, &paths, &pieces, &hidden, &mut layer);
        let chunks = std::array::from_fn(|k| split_layer(&layer, k));
        MapCache {
            revision: view.map_revision(),
            layer_version,
            #[cfg(test)]
            layer,
            chunks,
            paths,
            hidden,
            line_tiles,
            width: i32::from(w),
        }
    }

    /// そのマスを通る路線のうち `LineId` が最小のもの。
    pub(crate) fn first_line_at(&self, pos: GridPos) -> Option<LineId> {
        let i = usize::try_from(pos.y * self.width + pos.x).ok()?;
        let m = *self.line_tiles.get(i)?;
        (m != 0).then(|| LineId(m.trailing_zeros()))
    }
}

/// 静的な層の中身 [13 §3.11]。不透明の `Clear` → 市松(マップが表示域より小さければ外を塗る)→ 線路の縁 → 線路の中身 → 帯の縁 → 帯の中身 → 駅・つうか・番号の札。
fn build_layer(
    view: GameView<'_>,
    paths: &[Vec<GridPos>],
    pieces: &[BandPiece],
    hidden: &[StationId],
    out: &mut Vec<DrawCmd>,
) {
    out.clear();
    out.push(DrawCmd::Clear(palette::GROUND_A));
    let (w, h) = view.map_size();
    let (w, h) = (i32::from(w).min(MAP_COLS), i32::from(h).min(MAP_ROWS));
    for y in 0..h {
        for x in 0..w {
            if (x + y) % 2 == 1 {
                out.push(DrawCmd::FillRect {
                    rect: rect(x * TILE, y * TILE, TILE as u32, TILE as u32),
                    color: palette::GROUND_B,
                });
            }
        }
    }
    for off in off_map(w, h) {
        out.push(DrawCmd::FillRect {
            rect: off,
            color: palette::OFF_MAP,
        });
    }
    let grid = view.track_grid();
    for table in [&sprite::TRK_RAIL_O, &sprite::TRK_RAIL_F] {
        for (i, m) in grid.masks.iter().enumerate() {
            if m.is_empty() {
                continue;
            }
            let pos = GridPos {
                x: (i % usize::from(grid.width)) as i32,
                y: (i / usize::from(grid.width)) as i32,
            };
            let (lx, ly) = tile_layer(pos);
            for d in Dir8::ALL {
                if m.contains(d) {
                    out.push(map_sprite_cmd(table[d as usize], lx, ly, palette::WHITE));
                }
            }
        }
    }
    for fill in [false, true] {
        for p in pieces {
            let (lx, ly) = tile_layer(p.tile);
            let (table, tint) = if fill {
                (&sprite::TRK_BAND_F, line_color(p.line))
            } else {
                (&sprite::TRK_BAND_O, palette::WHITE)
            };
            out.push(map_sprite_cmd(
                table[p.dir as usize][p.variant as usize],
                lx,
                ly,
                tint,
            ));
        }
    }
    for st in view.stations().filter(|s| !hidden.contains(&s.id)) {
        let (lx, ly) = tile_layer(st.pos);
        let id = if st.connected {
            sprite::STN_BODY
        } else {
            sprite::STN_BODY_OFF
        };
        out.push(map_sprite_cmd(
            id,
            lx + TILE / 2,
            ly + TILE / 2,
            palette::WHITE,
        ));
    }
    for line in view.lines() {
        for (&s, &stop) in line.stations.iter().zip(line.stops) {
            if !stop && let Some(st) = view.station(s) {
                let (lx, ly) = tile_layer(st.pos);
                out.push(map_sprite_cmd(
                    sprite::STN_PASS,
                    lx + TILE / 2,
                    ly + TILE / 2,
                    palette::WHITE,
                ));
            }
        }
    }
    for (id, x, y) in badge_positions(paths) {
        let badge = sprite::LINE_NO[id.index() % sprite::LINE_NO.len()];
        out.push(map_sprite_cmd(badge, x, y, line_color(id)));
    }
}

/// マップ全体の層の中身から、`k` 枚目(x が `k·LAYER_W` から幅 `LAYER_W`)にかかるものを、その層の座標に移して取り出す。
fn split_layer(layer: &[DrawCmd], k: usize) -> Vec<DrawCmd> {
    let x0 = k as i32 * LAYER_W;
    let area = rect(x0, 0, LAYER_W as u32, WORLD_H as u32);
    let hits = |r: Rect| crate::anim::overlaps(r, area);
    let shift = |r: Rect| Rect { x: r.x - x0, ..r };
    layer
        .iter()
        .filter_map(|&c| match c {
            DrawCmd::FillRect { rect, color } => hits(rect).then(|| DrawCmd::FillRect {
                rect: shift(rect),
                color,
            }),
            DrawCmd::Sprite { id, dst, tint } => hits(dst).then(|| DrawCmd::Sprite {
                id,
                dst: shift(dst),
                tint,
            }),
            DrawCmd::Line {
                x0: a,
                y0,
                x1: b,
                y1,
                color,
            } => (a.min(b) < x0 + LAYER_W && a.max(b) >= x0).then_some(DrawCmd::Line {
                x0: a - x0,
                y0,
                x1: b - x0,
                y1,
                color,
            }),
            other => Some(other),
        })
        .collect()
}

/// マップが 40 × 18 より小さいときの、マップの外の矩形(層の座標。右と下)。
pub(crate) fn off_map(w: i32, h: i32) -> impl Iterator<Item = Rect> {
    let right = rect(w * TILE, 0, ((MAP_COLS - w) * TILE) as u32, WORLD_H as u32);
    let below = rect(
        0,
        h * TILE,
        (w * TILE) as u32,
        ((MAP_ROWS - h) * TILE) as u32,
    );
    [right, below].into_iter().filter(|r| r.w > 0 && r.h > 0)
}

/// 待っている人の点の矩形(マスの中心からの位置。左、上、幅、高さ)。
const DOTS_BOX: (i32, i32, i32, i32) =
    (7 * MAP_SCALE, -8 * MAP_SCALE, 5 * MAP_SCALE, 14 * MAP_SCALE);
/// 路線番号の札の大きさ。
const BADGE_W: i32 = 8 * MAP_SCALE;
const BADGE_H: i32 = 9 * MAP_SCALE;
/// 札を置く、端の駅の中心からの距離。
const BADGE_OUT: i32 = 11 * MAP_SCALE;

fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

fn badge_box(cx: i32, cy: i32) -> (i32, i32, i32, i32) {
    (cx - BADGE_W / 2, cy - BADGE_H / 2, BADGE_W, BADGE_H)
}

/// 路線番号の札の中心(層の座標)。端の駅から外向きに置き、人の点を避け、同じ位置の 2 枚目以降は直角にずらす。
pub(crate) fn badge_positions(paths: &[Vec<GridPos>]) -> Vec<(LineId, i32, i32)> {
    let mut out: Vec<(LineId, i32, i32)> = Vec::new();
    let mut bases: Vec<((i32, i32), u32)> = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        for (end, inner) in [
            (p.first(), p.get(1)),
            (p.last(), p.len().checked_sub(2).map(|j| &p[j])),
        ] {
            let (Some(&end), Some(&inner)) = (end, inner) else {
                continue;
            };
            let (dx, dy) = dir_between(inner, end).delta();
            let (lx, ly) = tile_layer(end);
            let (cx, cy) = (lx + TILE / 2, ly + TILE / 2);
            let (bx, mut by) = (cx + dx * BADGE_OUT, cy + dy * BADGE_OUT);
            let dots = (cx + DOTS_BOX.0, cy + DOTS_BOX.1, DOTS_BOX.2, DOTS_BOX.3);
            if overlaps(badge_box(bx, by), dots) {
                by = if dy < 0 {
                    dots.1 - (BADGE_H - BADGE_H / 2)
                } else {
                    dots.1 + dots.3 + BADGE_H / 2
                };
            }
            let perp = if dx == 0 { (1, 0) } else { (-dy, dx) };
            let k = match bases.iter_mut().find(|b| b.0 == (bx, by)) {
                Some(b) => {
                    b.1 += 1;
                    b.1
                }
                None => {
                    bases.push(((bx, by), 0));
                    0
                }
            } as i32;
            let step = BADGE_W.max(BADGE_H) + MAP_SCALE;
            out.push((
                LineId::from_index(i),
                bx + perp.0 * step * k,
                by + perp.1 * step * k,
            ));
        }
    }
    out
}

/// 列車の点の画面座標(マスの大きさの整数倍。補間しない。UI-D-18)。
pub(crate) fn train_screen(p: MapPoint, cam: Cam) -> (i32, i32) {
    let u = POS_UNITS_PER_TILE as i64;
    let t = i64::from(TILE);
    (
        MAP_X + (p.x * t / u) as i32 + TILE / 2 - cam.x,
        MAP_Y + (p.y * t / u) as i32 + TILE / 2 - cam.y,
    )
}

/// 静的な層を置く。幅 `view_w` の表示域にかかる層だけを出し、End の直後に毎フレーム `DrawCachedLayer` を出す(14 §3.5.3)。
pub(crate) fn draw_static(b: &mut DrawBuilder<'_>, cache: &MapCache, cam: Cam, view_w: i32) {
    for (k, content) in cache.chunks.iter().enumerate() {
        let x = MAP_X + k as i32 * LAYER_W - cam.x;
        if x >= MAP_X + view_w || x + LAYER_W <= MAP_X {
            continue;
        }
        b.cached_layer(
            LayerId(MAP_LAYER.0 + k as u8),
            cache.layer_version,
            rect(x, MAP_Y - cam.y, LAYER_W as u32, WORLD_H as u32),
            content,
        );
    }
}

/// 列車(絵 + 番号の札)。表示域の外の列車は描かない。
pub(crate) fn draw_trains(b: &mut DrawBuilder<'_>, view: GameView<'_>, cam: Cam, view_w: i32) {
    for t in view.trains() {
        let (x, y) = train_screen(t.point, cam);
        if !near_view(x, y, view_w) {
            continue;
        }
        let c = line_color(t.line);
        b.map_sprite(sprite::TRN[t.heading as usize], x, y, c);
        b.map_sprite(
            sprite::LINE_NO[t.line.index() % sprite::LINE_NO.len()],
            x + 7 * MAP_SCALE,
            y - 7 * MAP_SCALE,
            c,
        );
    }
}

/// 待っている人の点(駅で待てる人数に対する割合)と人数の札(10 人を超えたら)。`covered`(収入のポップアップ)と重なる札は今は出さない。
/// 札は幅 `view_w` の表示域に収める。表示域の外の駅は描かない。
pub(crate) fn draw_waiting(
    b: &mut DrawBuilder<'_>,
    view: GameView<'_>,
    tags: &[NumText],
    covered: &[Option<Rect>],
    cam: Cam,
    view_w: i32,
) {
    for st in view.stations() {
        let (Some(dots), tag) = (
            waiting_dots(st.waiting.0, st.capacity.0),
            waiting_marks(st.waiting.0).1,
        ) else {
            continue;
        };
        let (cx, cy) = tile_center(st.pos, cam);
        if !near_view(cx, cy, view_w) {
            continue;
        }
        b.map_sprite(dots, cx + DOTS_BOX.0, cy + DOTS_BOX.1, palette::WHITE);
        if tag && let Some(tag) = tags.get(st.id.index()) {
            let blk = &tag.block;
            let w = i32::from(blk.width);
            let x = crate::anim::clamp_to_map(cx - w / 2, w, view_w);
            let th = i32::from(blk.line_height);
            let y = cy - TILE / 2 - th;
            let r = rect(x - 2, y, (w + 4) as u32, th as u32);
            if covered
                .iter()
                .flatten()
                .any(|c| crate::anim::overlaps(r, *c))
            {
                continue;
            }
            b.group(Shed::PeopleTag, |b| {
                b.fill(r, palette::TAG_BG);
                b.text(blk, x, y, palette::TEXT);
            });
        }
    }
}

/// マップのカーソル。絵を 2 倍で描くので 4px の白い枠と 2px の黒い外枠。1 秒に 1 回ゆっくり明るさが変わる。
pub(crate) fn draw_cursor(b: &mut DrawBuilder<'_>, pos: GridPos, now: MonoTime, cam: Cam) {
    let (x, y) = tile_center(pos, cam);
    b.map_sprite(sprite::UI_MAP_CURSOR, x, y, pulse(now, 1000, 176));
}

/// 周期 `period_ms` の三角波で明るさを `low..=255` に変える白(整数だけ)。
pub(crate) fn pulse(now: MonoTime, period_ms: u64, low: u8) -> tm_platform_api::Color {
    let t = (now.0 / 1000) % period_ms;
    let half = period_ms / 2;
    let k = half.abs_diff(t);
    let v = u64::from(low) + (255 - u64::from(low)) * k / half.max(1);
    let v = v as u8;
    tm_platform_api::Color {
        r: v,
        g: v,
        b: v,
        a: 255,
    }
}

#[cfg(test)]
mod tests;
