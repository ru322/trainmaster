//! 駅の位置と出現順の事前生成(11 §3)。

use crate::balance::{CompanyRules, MapRules};
use crate::geom::GridPos;
use crate::rng::{STREAM_STATION_SPAWN, StreamRng, below};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Area {
    x0: i32,
    y0: i32,
    w: i32,
    h: i32,
}

impl Area {
    fn intersect(self, o: Area) -> Area {
        let (x0, y0) = (self.x0.max(o.x0), self.y0.max(o.y0));
        let x1 = (self.x0 + self.w).min(o.x0 + o.w);
        let y1 = (self.y0 + self.h).min(o.y0 + o.h);
        Area {
            x0,
            y0,
            w: (x1 - x0).max(0),
            h: (y1 - y0).max(0),
        }
    }
}

/// 1 駅をどの手順で置いたか。`Stage(0)` は緩めない間隔、`Stage(i)` は `relax_spacings[i − 1]`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    Stage(u8),
    Fallback,
}

/// 全 `count` 駅の位置を出現順に決める(11 §3.3、SIM-D-11)。`(seed, map, company)` だけで決まる。
pub(crate) fn plan_stations(
    seed: u64,
    map: &MapRules,
    company: &CompanyRules,
    count: u32,
) -> Vec<GridPos> {
    plan_traced(seed, map, company, count).0
}

fn plan_traced(
    seed: u64,
    map: &MapRules,
    company: &CompanyRules,
    count: u32,
) -> (Vec<GridPos>, Vec<Placement>) {
    let mut rng = StreamRng::new(seed, STREAM_STATION_SPAWN);
    let (w, h, m) = (
        i32::from(map.width),
        i32::from(map.height),
        i32::from(map.margin),
    );
    let place = Area {
        x0: m,
        y0: m,
        w: w - 2 * m,
        h: h - 2 * m,
    };
    let (aw, ah) = (i32::from(map.initial_area.0), i32::from(map.initial_area.1));
    let init = Area {
        x0: (w - aw) / 2,
        y0: (h - ah) / 2,
        w: aw,
        h: ah,
    }
    .intersect(place);
    let spacings: Vec<i32> = core::iter::once(map.min_spacing)
        .chain(map.relax_spacings.iter().copied())
        .map(i32::from)
        .collect();
    let max_nearest = i32::from(map.max_nearest);

    let mut out: Vec<GridPos> = Vec::with_capacity(count as usize);
    let mut trace = Vec::with_capacity(count as usize);
    for i in 0..count {
        let area = if i < u32::from(company.stations_base) {
            init
        } else {
            place
        };
        let mut found = None;
        'stages: for (stage, &sp) in spacings.iter().enumerate() {
            for _ in 0..map.attempts_per_stage {
                let x = area.x0 + below(&mut rng, area.w as u32) as i32;
                let y = area.y0 + below(&mut rng, area.h as u32) as i32;
                let p = GridPos { x, y };
                if acceptable(p, &out, sp, max_nearest) {
                    found = Some((p, Placement::Stage(stage as u8)));
                    break 'stages;
                }
            }
        }
        // 10 TD-06 の最後の手段。最初の駅の範囲を先に走査する。
        let (p, how) = found.unwrap_or_else(|| {
            let p = first_free_cell(area, &out)
                .or_else(|| first_free_cell(place, &out))
                .expect("置ける範囲のマスは max_stations 以上(BV-08)");
            (p, Placement::Fallback)
        });
        out.push(p);
        trace.push(how);
    }
    (out, trace)
}

/// 既存の全駅とのチェビシェフ距離が `sp` 以上で、既存の駅があればいちばん近い駅まで `max_nearest` 以下。
fn acceptable(p: GridPos, existing: &[GridPos], sp: i32, max_nearest: i32) -> bool {
    let mut nearest = i32::MAX;
    for q in existing {
        let d = (p.x - q.x).abs().max((p.y - q.y).abs());
        if d < sp {
            return false;
        }
        nearest = nearest.min(d);
    }
    existing.is_empty() || nearest <= max_nearest
}

/// `area` を y、x の昇順に走査し、どの駅とも重ならない最初のマス。
fn first_free_cell(area: Area, existing: &[GridPos]) -> Option<GridPos> {
    (area.y0..area.y0 + area.h)
        .flat_map(|y| (area.x0..area.x0 + area.w).map(move |x| GridPos { x, y }))
        .find(|p| !existing.contains(p))
}

#[cfg(test)]
mod tests;
