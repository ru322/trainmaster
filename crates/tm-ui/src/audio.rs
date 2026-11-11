//! 効果音の間引きと同時 2 音、ジングルの時刻、`FanfareFinished` の判定、BGM の順番 [13 §5、14 §6.5]。

use tm_assets::sound::{BGM_TITLE, JINGLE_GOAL, SE_COIN, SE_CONFIRM, SE_CURSOR};
use tm_core::{CommandError, CompanyLevel, GameEvent, LineId};
use tm_platform_api::{AudioBus, AudioCmd, MonoTime, SoundId};

use crate::settings::Settings;
use crate::sound_map::{
    BGM_ROTATION_CROSSFADE_MS, SePriority, SoundRequest, bgm_rotation, bgm_stage,
    se_min_interval_ms, se_priority, sound_for_error, sound_for_event,
};

/// 終わる時刻に足す余裕 [02 §4.6、14 §4.8]。
const MARGIN_US: u64 = 100_000;
/// `FanfareFinished` の保険。最後のジングルが終わる時刻からの待ち [13 §5.3]。
const FANFARE_FALLBACK_US: u64 = 5_000_000;
/// 同時に鳴らす効果音の数 [01 §12.2]。
const MAX_VOICES: usize = 2;
/// お祝いの演出の最短の長さ。anim のゴールの帯(max(ジングル, 3000ms))にそろえる [13 §6.3 #14]。
const CELEBRATION_MIN_US: u64 = 3_000_000;
const SAMPLE_RATE: u64 = 22_050;

/// UI が鳴っているとみなす効果音。
#[derive(Clone, Copy, Debug)]
struct Voice {
    id: SoundId,
    prio: SePriority,
    start: MonoTime,
    end: MonoTime,
}

/// 会社レベルアップとゴールの演出の終わり待ち。
#[derive(Clone, Copy, Debug)]
struct Fanfare {
    opened_at: MonoTime,
    celebration_end: Option<MonoTime>,
    station_se_end: Option<MonoTime>,
}

/// ゲーム中の曲の順番。`played_us` はジングルの間を除いた実時間 [14 PL-D-31]。
#[derive(Clone, Copy, Debug)]
struct Rotation {
    stage: usize,
    index: usize,
    played_us: u64,
    counted_to: MonoTime,
}

fn after(t: MonoTime, us: u64) -> MonoTime {
    MonoTime(t.0.saturating_add(us))
}

/// 音の命令を出す係。時刻はすべて引数で受ける。
pub(crate) struct AudioDirector {
    len_us: Vec<u64>,
    booted: bool,
    last_play: Vec<Option<MonoTime>>,
    voices: Vec<Voice>,
    jingle_busy_until: Option<MonoTime>,
    station_se_pending: Option<SoundId>,
    fanfare: Option<Fanfare>,
    lines_done_at: Option<MonoTime>,
    lines_done: Vec<LineId>,
    rotation: Option<Rotation>,
}

impl AudioDirector {
    /// 長さは `SoundData` のサンプル数から求める。読めない音は長さ 0 とする。
    pub(crate) fn new() -> AudioDirector {
        let len_us = tm_assets::sound::sound_data()
            .map(|t| {
                t.iter()
                    .map(|s| s.samples.len() as u64 * 1_000_000 / SAMPLE_RATE)
                    .collect()
            })
            .unwrap_or_default();
        AudioDirector::with_lengths(len_us)
    }

    fn with_lengths(len_us: Vec<u64>) -> AudioDirector {
        let n = len_us.len();
        AudioDirector {
            len_us,
            booted: false,
            last_play: vec![None; n],
            voices: Vec::with_capacity(MAX_VOICES),
            jingle_busy_until: None,
            station_se_pending: None,
            fanfare: None,
            lines_done_at: None,
            lines_done: Vec::new(),
            rotation: None,
        }
    }

    fn len(&self, id: SoundId) -> u64 {
        self.len_us.get(usize::from(id.0)).copied().unwrap_or(0)
    }

    /// 最初の 1 回だけ、音量 2 つとタイトルの曲を出す(UI-CR-11)。
    pub(crate) fn boot(&mut self, settings: &Settings, out: &mut Vec<AudioCmd>) {
        if self.booted {
            return;
        }
        self.booted = true;
        out.extend([
            AudioCmd::SetVolume {
                bus: AudioBus::Bgm,
                level: settings.music,
            },
            AudioCmd::SetVolume {
                bus: AudioBus::Se,
                level: settings.sound,
            },
            AudioCmd::StopBgm { fade_ms: 0 },
            AudioCmd::PlayBgm {
                id: BGM_TITLE,
                crossfade_ms: 0,
            },
        ]);
    }

    /// タイトルで決定した: `se_confirm` と、ゲームの曲を頭から。
    pub(crate) fn start_game(&mut self, now: MonoTime, out: &mut Vec<AudioCmd>) {
        self.se(now, SE_CONFIRM, out);
        self.continue_game(now, CompanyLevel(0), out);
    }

    /// 続きから: 会社レベルの段階の並びの先頭の曲を頭から [13 §5.1]。
    pub(crate) fn continue_game(
        &mut self,
        now: MonoTime,
        level: CompanyLevel,
        out: &mut Vec<AudioCmd>,
    ) {
        let id = self.begin_stage(now, bgm_stage(level.0));
        out.push(AudioCmd::PlayBgm {
            id,
            crossfade_ms: 0,
        });
    }

    fn begin_stage(&mut self, now: MonoTime, stage: usize) -> SoundId {
        self.rotation = Some(Rotation {
            stage,
            index: 0,
            played_us: 0,
            counted_to: now,
        });
        bgm_rotation(stage)[0].0
    }

    /// 曲の順番を進める。ジングルの間は数えない。
    fn rotate(&mut self, now: MonoTime, out: &mut Vec<AudioCmd>) {
        let Some(r) = &mut self.rotation else {
            return;
        };
        let from = self
            .jingle_busy_until
            .map_or(r.counted_to, |b| b.max(r.counted_to));
        r.played_us = r.played_us.saturating_add(now.saturating_since(from));
        r.counted_to = r.counted_to.max(now);
        let seq = bgm_rotation(r.stage);
        if seq.len() > 1 && r.played_us >= seq[r.index].1 {
            r.index = (r.index + 1) % seq.len();
            r.played_us = 0;
            out.push(AudioCmd::PlayBgm {
                id: seq[r.index].0,
                crossfade_ms: BGM_ROTATION_CROSSFADE_MS,
            });
        }
    }

    /// タイトルに戻る。ジングルの記録は消し、鳴っている効果音の記録は残す [13 §5.2、§5.3]。
    pub(crate) fn go_to_title(&mut self, out: &mut Vec<AudioCmd>) {
        self.jingle_busy_until = None;
        self.station_se_pending = None;
        self.fanfare = None;
        self.rotation = None;
        out.extend([
            AudioCmd::StopBgm { fade_ms: 0 },
            AudioCmd::PlayBgm {
                id: BGM_TITLE,
                crossfade_ms: 0,
            },
        ]);
    }

    /// 効果音を鳴らす。間引きと同時 2 音の規則で捨てることがある [13 §5.2]。
    pub(crate) fn se(&mut self, now: MonoTime, id: SoundId, out: &mut Vec<AudioCmd>) {
        let Some(prio) = se_priority(id) else {
            debug_assert!(false, "効果音でない ID {id:?}");
            return;
        };
        let slot = usize::from(id.0);
        let gap = u64::from(se_min_interval_ms(id)) * 1000;
        if let Some(Some(t)) = self.last_play.get(slot)
            && now.saturating_since(*t) < gap
        {
            return;
        }
        self.voices.retain(|v| v.end > now);
        if self.voices.len() >= MAX_VOICES {
            let mut low = 0;
            for (i, v) in self.voices.iter().enumerate().skip(1) {
                let l = &self.voices[low];
                if v.prio > l.prio || (v.prio == l.prio && v.start < l.start) {
                    low = i;
                }
            }
            if !prio.is_higher_than(self.voices[low].prio) {
                return;
            }
            out.push(AudioCmd::StopSe(self.voices.remove(low).id));
        }
        out.push(AudioCmd::PlaySe(id));
        self.voices.push(Voice {
            id,
            prio,
            start: now,
            end: after(now, self.len(id) + MARGIN_US),
        });
        if let Some(t) = self.last_play.get_mut(slot) {
            *t = Some(now);
        }
    }

    /// `validate` の `Err` の音。
    pub(crate) fn error(&mut self, now: MonoTime, err: &CommandError, out: &mut Vec<AudioCmd>) {
        self.se(now, sound_for_error(err), out);
    }

    /// 音量を 1 段変えた: `SetVolume` の後、音楽なら `se_cursor`、音なら新しい音量で `se_coin` [13 §5.1]。
    pub(crate) fn volume_changed(
        &mut self,
        now: MonoTime,
        bus: AudioBus,
        level: u8,
        out: &mut Vec<AudioCmd>,
    ) {
        out.push(AudioCmd::SetVolume { bus, level });
        let id = match bus {
            AudioBus::Bgm => SE_CURSOR,
            AudioBus::Se => SE_COIN,
        };
        self.se(now, id, out);
    }

    /// ジングルを出し、終わる時刻を待ち行列の順に進める。返り値は鳴り始める時刻。
    fn jingle(&mut self, now: MonoTime, id: SoundId, out: &mut Vec<AudioCmd>) -> MonoTime {
        out.push(AudioCmd::PlayJingle(id));
        let start = self.jingle_busy_until.map_or(now, |t| t.max(now));
        self.jingle_busy_until = Some(after(start, self.len(id) + MARGIN_US));
        start
    }

    /// `GameEvent` の音 [10 §6.3、13 §5.1]。
    pub(crate) fn observe(&mut self, now: MonoTime, ev: &GameEvent, out: &mut Vec<AudioCmd>) {
        if self.lines_done_at != Some(now) {
            self.lines_done_at = Some(now);
            self.lines_done.clear();
        }
        let mut req = sound_for_event(ev);
        match *ev {
            GameEvent::LineCreated { line, .. } | GameEvent::LineExtended { line, .. } => {
                self.lines_done.push(line);
            }
            // UI-CR-06: 延伸と同じフレームの「つうか」は路線の音だけにする
            GameEvent::PassThroughAdded { line, .. } if self.lines_done.contains(&line) => {
                req = None;
            }
            _ => {}
        }
        let mut jingle_start = None;
        match req {
            None => {}
            Some(SoundRequest::Se(id)) => self.se(now, id, out),
            Some(SoundRequest::SeAfterJingles(id)) => self.station_se_pending = Some(id),
            Some(SoundRequest::Jingle(j)) => jingle_start = Some(self.jingle(now, j, out)),
            Some(SoundRequest::JingleThenBgm(j, b)) => {
                jingle_start = Some(self.jingle(now, j, out));
                out.push(AudioCmd::PlayBgm {
                    id: b,
                    crossfade_ms: 0,
                });
                if let GameEvent::CompanyLevelUp { new_level, .. } = *ev {
                    self.begin_stage(now, bgm_stage(new_level.0));
                }
            }
        }
        if matches!(
            ev,
            GameEvent::CompanyLevelUp { .. } | GameEvent::GoalReached { .. }
        ) {
            let goal_len = self.len(JINGLE_GOAL).max(CELEBRATION_MIN_US);
            let f = self.fanfare.get_or_insert(Fanfare {
                opened_at: now,
                celebration_end: None,
                station_se_end: None,
            });
            if let (GameEvent::GoalReached { .. }, Some(start)) = (ev, jingle_start) {
                f.celebration_end = Some(after(start, goal_len));
            }
        }
    }

    /// 遅らせた `se_station_new` を鳴らす時刻: 最後のジングルの終わり(なければ `now`)。
    pub(crate) fn after_jingles(&self, now: MonoTime) -> MonoTime {
        self.jingle_busy_until.map_or(now, |t| t.max(now))
    }

    /// 毎フレームの最初に呼ぶ。曲の順番を進め、遅らせた効果音を出し、`FanfareFinished` を出すときは true [13 §5.3]。
    pub(crate) fn poll(&mut self, now: MonoTime, out: &mut Vec<AudioCmd>) -> bool {
        self.rotate(now, out);
        if let Some(id) = self.station_se_pending
            && self.jingle_busy_until.is_none_or(|t| now >= t)
        {
            self.station_se_pending = None;
            self.se(now, id, out);
            let end = after(now, self.len(id));
            if let Some(f) = &mut self.fanfare {
                f.station_se_end = Some(f.station_se_end.map_or(end, |e| e.max(end)));
            }
        }
        let Some(f) = self.fanfare else {
            return false;
        };
        let jingles = self.jingle_busy_until.max(f.celebration_end);
        let settled = jingles
            .max(f.station_se_end)
            .map_or(f.opened_at, |t| t.max(f.opened_at));
        let done = self.station_se_pending.is_none() && now >= after(settled, MARGIN_US);
        let fallback = now >= after(jingles.unwrap_or(f.opened_at), FANFARE_FALLBACK_US);
        if done || fallback {
            self.fanfare = None;
        }
        done || fallback
    }
}

#[cfg(test)]
mod tests {
    use tm_assets::sound::{
        BGM_GAME_1, BGM_GAME_2, BGM_GAME_3, JINGLE_ALL_UNLOCKED, JINGLE_COMPANY_UP, SE_BUY,
        SE_CANCEL, SE_CANNOT_BUY, SE_ERROR, SE_LINE_EXTEND, SE_LINE_NEW, SE_MENU_OPEN, SE_POPUP,
        SE_RESET, SE_STATION_NEW, SE_UNIT_UP, SOUND_COUNT,
    };
    use tm_core::{CompanyLevel, GridPos, LineEnd, Money, StationId};

    use super::*;

    const MS: u64 = 1000;
    const SE_LEN: u64 = 200 * MS;
    const STATION_LEN: u64 = 400 * MS;
    const UP_LEN: u64 = 2000 * MS;
    const ALL_LEN: u64 = 3000 * MS;
    const GOAL_LEN: u64 = 6000 * MS;
    const MARGIN: u64 = 100 * MS;
    const FALLBACK: u64 = 5000 * MS;

    fn director() -> AudioDirector {
        let mut len = vec![SE_LEN; SOUND_COUNT];
        len[usize::from(SE_STATION_NEW.0)] = STATION_LEN;
        len[usize::from(JINGLE_COMPANY_UP.0)] = UP_LEN;
        len[usize::from(JINGLE_ALL_UNLOCKED.0)] = ALL_LEN;
        len[usize::from(JINGLE_GOAL.0)] = GOAL_LEN;
        AudioDirector::with_lengths(len)
    }

    fn t(ms: u64) -> MonoTime {
        MonoTime(ms * MS)
    }

    fn se(d: &mut AudioDirector, at: MonoTime, id: SoundId) -> Vec<AudioCmd> {
        let mut out = Vec::new();
        d.se(at, id, &mut out);
        out
    }

    fn observe(d: &mut AudioDirector, at: MonoTime, evs: &[GameEvent]) -> Vec<AudioCmd> {
        let mut out = Vec::new();
        for ev in evs {
            d.observe(at, ev, &mut out);
        }
        out
    }

    fn poll(d: &mut AudioDirector, at: MonoTime) -> (bool, Vec<AudioCmd>) {
        let mut out = Vec::new();
        let fin = d.poll(at, &mut out);
        (fin, out)
    }

    fn level_up(lv: u8) -> GameEvent {
        GameEvent::CompanyLevelUp {
            new_level: CompanyLevel(lv),
            all_features_unlocked: lv == 4,
        }
    }

    fn spawned(i: u32) -> GameEvent {
        GameEvent::StationSpawned {
            id: StationId(i),
            pos: GridPos { x: i as i32, y: 1 },
        }
    }

    const PLAY_STATION: AudioCmd = AudioCmd::PlaySe(SE_STATION_NEW);

    #[test]
    fn boot_once_with_settings() {
        let mut d = director();
        let s = Settings {
            music: 3,
            sound: 9,
            show_explanations: true,
        };
        let mut out = Vec::new();
        d.boot(&s, &mut out);
        d.boot(&s, &mut out);
        assert_eq!(
            out,
            [
                AudioCmd::SetVolume {
                    bus: AudioBus::Bgm,
                    level: 3
                },
                AudioCmd::SetVolume {
                    bus: AudioBus::Se,
                    level: 9
                },
                AudioCmd::StopBgm { fade_ms: 0 },
                AudioCmd::PlayBgm {
                    id: BGM_TITLE,
                    crossfade_ms: 0
                },
            ]
        );
    }

    #[test]
    fn start_game_confirms_and_plays_game_1() {
        let mut d = director();
        let mut out = Vec::new();
        d.start_game(t(10), &mut out);
        assert_eq!(
            out,
            [
                AudioCmd::PlaySe(SE_CONFIRM),
                AudioCmd::PlayBgm {
                    id: BGM_GAME_1,
                    crossfade_ms: 0
                }
            ]
        );
    }

    #[test]
    fn thinning_per_id() {
        let mut d = director();
        assert_eq!(se(&mut d, t(0), SE_COIN), [AudioCmd::PlaySe(SE_COIN)]);
        assert!(se(&mut d, t(0), SE_COIN).is_empty(), "同じフレームは 1 回");
        assert!(se(&mut d, MonoTime(150 * MS - 1), SE_COIN).is_empty());
        assert_eq!(se(&mut d, t(150), SE_COIN), [AudioCmd::PlaySe(SE_COIN)]);

        let mut d = director();
        assert_eq!(se(&mut d, t(0), SE_ERROR), [AudioCmd::PlaySe(SE_ERROR)]);
        assert!(se(&mut d, MonoTime(50 * MS - 1), SE_ERROR).is_empty());
        assert_eq!(se(&mut d, t(50), SE_ERROR), [AudioCmd::PlaySe(SE_ERROR)]);
    }

    #[test]
    fn voice_ends_after_length_plus_margin() {
        let mut d = director();
        se(&mut d, t(0), SE_CONFIRM);
        se(&mut d, t(0), SE_CANCEL);
        assert!(
            se(&mut d, MonoTime(SE_LEN + MARGIN - 1), SE_CURSOR).is_empty(),
            "同じ優先度は鳴っている方を残す"
        );
        assert_eq!(
            se(&mut d, MonoTime(SE_LEN + MARGIN), SE_CURSOR),
            [AudioCmd::PlaySe(SE_CURSOR)]
        );
    }

    #[test]
    fn higher_priority_stops_lowest() {
        let mut d = director();
        se(&mut d, t(0), SE_POPUP);
        se(&mut d, t(10), SE_COIN);
        assert_eq!(
            se(&mut d, t(20), SE_BUY),
            [AudioCmd::StopSe(SE_COIN), AudioCmd::PlaySe(SE_BUY)],
            "新しくても優先度の低い方を止める"
        );
        assert_eq!(
            se(&mut d, t(30), SE_ERROR),
            [AudioCmd::StopSe(SE_POPUP), AudioCmd::PlaySe(SE_ERROR)]
        );
        assert!(se(&mut d, t(40), SE_LINE_NEW).is_empty());
        assert!(se(&mut d, t(40), SE_COIN).is_empty());
    }

    #[test]
    fn lower_or_equal_is_dropped_without_touching_last_play() {
        let mut d = director();
        se(&mut d, t(0), SE_CONFIRM);
        se(&mut d, t(0), SE_MENU_OPEN);
        assert!(se(&mut d, t(100), SE_COIN).is_empty());
        assert!(se(&mut d, t(100), SE_UNIT_UP).is_empty());
        let free = SE_LEN + MARGIN;
        assert_eq!(
            se(&mut d, MonoTime(free), SE_COIN),
            [AudioCmd::PlaySe(SE_COIN)],
            "捨てた音は間引きの時刻にしない"
        );
    }

    #[test]
    fn same_lowest_stops_older() {
        let mut d = director();
        se(&mut d, t(0), SE_COIN);
        se(&mut d, t(150), SE_COIN);
        assert_eq!(
            se(&mut d, t(160), SE_POPUP),
            [AudioCmd::StopSe(SE_COIN), AudioCmd::PlaySe(SE_POPUP)]
        );
        let old_end = SE_LEN + MARGIN;
        assert!(
            se(&mut d, MonoTime(old_end + 10 * MS), SE_COIN).is_empty(),
            "残っているのは 150ms の方"
        );

        let mut d = director();
        se(&mut d, t(0), SE_LINE_NEW);
        se(&mut d, t(5), SE_POPUP);
        assert_eq!(
            se(&mut d, t(10), SE_RESET),
            [AudioCmd::StopSe(SE_LINE_NEW), AudioCmd::PlaySe(SE_RESET)]
        );
    }

    #[test]
    fn same_start_stops_first_played() {
        let mut d = director();
        se(&mut d, t(0), SE_LINE_EXTEND);
        se(&mut d, t(0), SE_POPUP);
        assert_eq!(
            se(&mut d, t(0), SE_CANNOT_BUY),
            [
                AudioCmd::StopSe(SE_LINE_EXTEND),
                AudioCmd::PlaySe(SE_CANNOT_BUY)
            ]
        );
    }

    #[test]
    fn ui_helpers() {
        let mut d = director();
        let mut out = Vec::new();
        d.volume_changed(t(0), AudioBus::Bgm, 4, &mut out);
        d.volume_changed(t(0), AudioBus::Se, 7, &mut out);
        assert_eq!(
            out,
            [
                AudioCmd::SetVolume {
                    bus: AudioBus::Bgm,
                    level: 4
                },
                AudioCmd::PlaySe(SE_CURSOR),
                AudioCmd::SetVolume {
                    bus: AudioBus::Se,
                    level: 7
                },
                AudioCmd::PlaySe(SE_COIN),
            ]
        );
        let mut d = director();
        let mut out = Vec::new();
        d.error(
            t(0),
            &CommandError::InsufficientFunds {
                required: Money::from_u64(2),
                available: Money::ONE,
            },
            &mut out,
        );
        d.error(t(0), &CommandError::UnknownLine(LineId(3)), &mut out);
        assert_eq!(
            out,
            [AudioCmd::PlaySe(SE_CANNOT_BUY), AudioCmd::PlaySe(SE_ERROR)]
        );
    }

    #[test]
    fn jingle_then_bgm_in_event_order() {
        let mut d = director();
        let out = observe(&mut d, t(0), &[level_up(5), level_up(6)]);
        assert_eq!(
            out,
            [
                AudioCmd::PlayJingle(JINGLE_COMPANY_UP),
                AudioCmd::PlayBgm {
                    id: BGM_GAME_2,
                    crossfade_ms: 0
                },
                AudioCmd::PlayJingle(JINGLE_COMPANY_UP),
            ]
        );
        assert_eq!(d.jingle_busy_until, Some(MonoTime(2 * (UP_LEN + MARGIN))));
    }

    #[test]
    fn jingle_starts_after_previous_or_now() {
        let mut d = director();
        observe(&mut d, t(0), &[level_up(3)]);
        observe(&mut d, t(1000), &[level_up(4)]);
        let first = UP_LEN + MARGIN;
        assert_eq!(
            d.jingle_busy_until,
            Some(MonoTime(first + ALL_LEN + MARGIN))
        );
        let late = t(60_000);
        observe(&mut d, late, &[level_up(5)]);
        assert_eq!(
            d.jingle_busy_until,
            Some(MonoTime(late.0 + UP_LEN + MARGIN))
        );
    }

    #[test]
    fn station_se_waits_for_last_jingle() {
        let mut d = director();
        let out = observe(&mut d, t(0), &[level_up(1), spawned(3), spawned(4)]);
        assert_eq!(out, [AudioCmd::PlayJingle(JINGLE_COMPANY_UP)]);
        let busy = UP_LEN + MARGIN;
        assert_eq!(poll(&mut d, MonoTime(busy - 1)).1, []);
        assert_eq!(poll(&mut d, MonoTime(busy)).1, [PLAY_STATION]);
        assert_eq!(poll(&mut d, MonoTime(busy + 1)).1, [], "1 回だけ");
    }

    #[test]
    fn station_se_without_jingle_is_immediate() {
        let mut d = director();
        observe(&mut d, t(5), &[spawned(3)]);
        assert_eq!(poll(&mut d, t(5)).1, [PLAY_STATION]);
    }

    #[test]
    fn fanfare_after_jingle_and_station_se() {
        let mut d = director();
        observe(&mut d, t(0), &[level_up(1), spawned(3)]);
        let busy = UP_LEN + MARGIN;
        let mut fins = Vec::new();
        let mut now = 0;
        while now < 10_000 * MS {
            if poll(&mut d, MonoTime(now)).0 {
                fins.push(now);
            }
            now += 1000;
        }
        assert_eq!(fins, [busy + STATION_LEN + MARGIN]);
    }

    #[test]
    fn fanfare_without_station_waits_for_jingle() {
        let mut d = director();
        observe(&mut d, t(0), &[level_up(1)]);
        let busy = UP_LEN + MARGIN;
        assert!(!poll(&mut d, MonoTime(busy + MARGIN - 1)).0);
        assert!(poll(&mut d, MonoTime(busy + MARGIN)).0);
        assert!(!poll(&mut d, MonoTime(busy + MARGIN + 1)).0);
    }

    #[test]
    fn short_goal_jingle_waits_for_the_3s_celebration() {
        let mut len = vec![SE_LEN; SOUND_COUNT];
        len[usize::from(JINGLE_GOAL.0)] = 1720 * MS;
        let mut d = AudioDirector::with_lengths(len);
        observe(
            &mut d,
            t(0),
            &[GameEvent::GoalReached {
                total_earned: Money::ONE,
            }],
        );
        let due = 3000 * MS + MARGIN;
        assert!(!poll(&mut d, MonoTime(due - 1)).0);
        assert!(poll(&mut d, MonoTime(due)).0);
    }

    #[test]
    fn fanfare_for_goal() {
        let mut d = director();
        observe(
            &mut d,
            t(0),
            &[GameEvent::GoalReached {
                total_earned: Money::ONE,
            }],
        );
        assert_eq!(
            d.fanfare.and_then(|f| f.celebration_end),
            Some(MonoTime(GOAL_LEN))
        );
        let busy = GOAL_LEN + MARGIN;
        assert!(!poll(&mut d, MonoTime(busy + MARGIN - 1)).0);
        assert!(poll(&mut d, MonoTime(busy + MARGIN)).0);
    }

    #[test]
    fn goal_celebration_starts_after_queued_jingles() {
        let mut d = director();
        observe(
            &mut d,
            t(0),
            &[
                level_up(2),
                GameEvent::GoalReached {
                    total_earned: Money::ONE,
                },
            ],
        );
        let start = UP_LEN + MARGIN;
        assert_eq!(
            d.fanfare.and_then(|f| f.celebration_end),
            Some(MonoTime(start + GOAL_LEN))
        );
    }

    #[test]
    fn many_level_ups_do_not_trip_the_fallback() {
        let mut d = director();
        let evs: Vec<GameEvent> = (1..=16).map(level_up).chain([spawned(5)]).collect();
        observe(&mut d, t(0), &evs);
        let busy = 15 * (UP_LEN + MARGIN) + ALL_LEN + MARGIN;
        assert_eq!(d.jingle_busy_until, Some(MonoTime(busy)));
        let mut now = 0;
        while now < busy {
            assert!(!poll(&mut d, MonoTime(now)).0, "{now}");
            now += 16_667;
        }
    }

    #[test]
    fn fallback_five_seconds_after_last_jingle() {
        let mut d = director();
        observe(&mut d, t(0), &[level_up(1)]);
        if let Some(f) = &mut d.fanfare {
            f.station_se_end = Some(MonoTime(u64::MAX / 2));
        }
        let busy = UP_LEN + MARGIN;
        assert!(!poll(&mut d, MonoTime(busy + FALLBACK - 1)).0);
        assert!(poll(&mut d, MonoTime(busy + FALLBACK)).0);
        assert!(d.fanfare.is_none());
    }

    #[test]
    fn go_to_title_clears_jingles_but_keeps_voices() {
        let mut d = director();
        se(&mut d, t(0), SE_RESET);
        observe(&mut d, t(0), &[level_up(1), spawned(3)]);
        let mut out = Vec::new();
        d.go_to_title(&mut out);
        assert_eq!(
            out,
            [
                AudioCmd::StopBgm { fade_ms: 0 },
                AudioCmd::PlayBgm {
                    id: BGM_TITLE,
                    crossfade_ms: 0
                }
            ]
        );
        assert_eq!(poll(&mut d, t(60_000)), (false, vec![]));
        assert!(se(&mut d, t(10), SE_RESET).is_empty(), "間引きの記録は残る");
        se(&mut d, t(10), SE_CONFIRM);
        assert!(
            se(&mut d, t(10), SE_CANCEL).is_empty(),
            "鳴っている記録も残る"
        );
        observe(&mut d, t(100_000), &[level_up(2)]);
        assert_eq!(
            d.jingle_busy_until,
            Some(MonoTime(100_000 * MS + UP_LEN + MARGIN))
        );
    }

    #[test]
    fn pass_through_with_line_in_same_frame_is_silent() {
        let line = LineId(1);
        let pass = GameEvent::PassThroughAdded {
            line,
            station: StationId(2),
        };
        let mut d = director();
        let out = observe(
            &mut d,
            t(0),
            &[
                GameEvent::LineExtended {
                    line,
                    end: LineEnd::Head,
                    station: StationId(2),
                    cost: tm_core::Money::ZERO,
                    new_tiles: 0,
                },
                pass,
            ],
        );
        assert_eq!(out, [AudioCmd::PlaySe(SE_LINE_EXTEND)]);

        let mut d = director();
        let out = observe(
            &mut d,
            t(0),
            &[
                GameEvent::LineCreated {
                    line,
                    station_count: 3,
                    cost: tm_core::Money::ZERO,
                    new_tiles: 0,
                },
                pass,
            ],
        );
        assert_eq!(out, [AudioCmd::PlaySe(SE_LINE_NEW)]);
        assert_eq!(
            observe(&mut d, t(100), &[pass]),
            [AudioCmd::PlaySe(SE_POPUP)]
        );

        let mut d = director();
        let other = observe(
            &mut d,
            t(0),
            &[
                GameEvent::LineCreated {
                    line: LineId(0),
                    station_count: 2,
                    cost: tm_core::Money::ZERO,
                    new_tiles: 0,
                },
                pass,
            ],
        );
        assert_eq!(
            other,
            [AudioCmd::PlaySe(SE_LINE_NEW), AudioCmd::PlaySe(SE_POPUP)]
        );
    }

    #[test]
    fn unit_up_only() {
        let mut d = director();
        assert_eq!(
            observe(&mut d, t(0), &[GameEvent::MoneyUnitReached { unit: 2 }]),
            [AudioCmd::PlaySe(SE_UNIT_UP)]
        );
    }

    const SEC: u64 = 1000 * MS;
    const UP_BUSY: u64 = UP_LEN + MARGIN;

    fn bgm(id: SoundId, crossfade_ms: u16) -> AudioCmd {
        AudioCmd::PlayBgm { id, crossfade_ms }
    }

    fn turn(id: SoundId) -> AudioCmd {
        bgm(id, 2000)
    }

    /// `from` から `to` まで `step` ごとに `poll` し、出た `PlayBgm` を時刻と一緒に返す。
    fn bgm_log(d: &mut AudioDirector, from: u64, to: u64, step: u64) -> Vec<(u64, AudioCmd)> {
        let mut log = Vec::new();
        let mut now = from;
        while now <= to {
            let (_, out) = poll(d, MonoTime(now));
            log.extend(
                out.into_iter()
                    .filter(|c| matches!(c, AudioCmd::PlayBgm { .. }))
                    .map(|c| (now, c)),
            );
            now += step;
        }
        log
    }

    fn continue_at(d: &mut AudioDirector, at: u64, lv: u8) -> Vec<AudioCmd> {
        let mut out = Vec::new();
        d.continue_game(MonoTime(at), CompanyLevel(lv), &mut out);
        out
    }

    #[test]
    fn continue_plays_the_head_of_the_stage() {
        for lv in 0..=24u8 {
            let want = match lv {
                0..=4 => BGM_GAME_1,
                5..=13 => BGM_GAME_2,
                _ => BGM_GAME_3,
            };
            let mut d = director();
            assert_eq!(continue_at(&mut d, 0, lv), [bgm(want, 0)], "Lv{lv}");
        }
    }

    #[test]
    fn stage_1_does_not_rotate() {
        let mut d = director();
        let mut out = Vec::new();
        d.start_game(t(0), &mut out);
        assert_eq!(bgm_log(&mut d, 0, 7200 * SEC, SEC), []);
        let mut d = director();
        continue_at(&mut d, 0, 4);
        assert_eq!(bgm_log(&mut d, 0, 7200 * SEC, SEC), []);
    }

    #[test]
    fn stage_2_rotation_after_the_level_up_jingle() {
        let mut d = director();
        let mut out = Vec::new();
        d.start_game(t(0), &mut out);
        bgm_log(&mut d, 0, 1000 * MS, 100 * MS);
        assert_eq!(
            observe(&mut d, t(1000), &[level_up(5)]),
            [AudioCmd::PlayJingle(JINGLE_COMPANY_UP), bgm(BGM_GAME_2, 0)]
        );
        let b = 1000 * MS + UP_BUSY;
        assert_eq!(
            bgm_log(&mut d, 1000 * MS, b + 1100 * SEC, MS),
            [
                (b + 360 * SEC, turn(BGM_GAME_1)),
                (b + 540 * SEC, turn(BGM_GAME_2)),
                (b + 900 * SEC, turn(BGM_GAME_1)),
                (b + 1080 * SEC, turn(BGM_GAME_2)),
            ]
        );
    }

    #[test]
    fn stage_3_rotation_order_and_time() {
        let mut d = director();
        assert_eq!(continue_at(&mut d, 0, 24), [bgm(BGM_GAME_3, 0)]);
        assert_eq!(
            bgm_log(&mut d, 0, 1500 * SEC, MS),
            [
                (360 * SEC, turn(BGM_GAME_1)),
                (540 * SEC, turn(BGM_GAME_3)),
                (900 * SEC, turn(BGM_GAME_2)),
                (1080 * SEC, turn(BGM_GAME_3)),
                (1440 * SEC, turn(BGM_GAME_1)),
            ]
        );
    }

    #[test]
    fn jingles_are_not_counted() {
        let mut d = director();
        continue_at(&mut d, 0, 14);
        bgm_log(&mut d, 0, 100 * SEC, SEC);
        assert_eq!(
            observe(&mut d, MonoTime(100 * SEC), &[level_up(15)]),
            [AudioCmd::PlayJingle(JINGLE_COMPANY_UP)]
        );
        bgm_log(&mut d, 100 * SEC, 200 * SEC, SEC);
        observe(&mut d, MonoTime(200 * SEC), &[level_up(16), level_up(17)]);
        let paused = 3 * UP_BUSY;
        assert_eq!(
            bgm_log(&mut d, 200 * SEC, 600 * SEC, MS),
            [
                (360 * SEC + paused, turn(BGM_GAME_1)),
                (540 * SEC + paused, turn(BGM_GAME_3)),
            ]
        );
    }

    #[test]
    fn new_stage_starts_from_its_own_song() {
        let mut d = director();
        continue_at(&mut d, 0, 13);
        assert_eq!(
            bgm_log(&mut d, 0, 400 * SEC, SEC),
            [(360 * SEC, turn(BGM_GAME_1))]
        );
        assert_eq!(
            observe(&mut d, MonoTime(400 * SEC), &[level_up(14), level_up(15)]),
            [
                AudioCmd::PlayJingle(JINGLE_COMPANY_UP),
                bgm(BGM_GAME_3, 0),
                AudioCmd::PlayJingle(JINGLE_COMPANY_UP),
            ]
        );
        let b = 400 * SEC + 2 * UP_BUSY;
        assert_eq!(
            bgm_log(&mut d, 400 * SEC, b + 600 * SEC, MS),
            [
                (b + 360 * SEC, turn(BGM_GAME_1)),
                (b + 540 * SEC, turn(BGM_GAME_3)),
            ]
        );
    }

    #[test]
    fn one_turn_per_poll_after_a_long_gap() {
        let mut d = director();
        continue_at(&mut d, 0, 20);
        assert_eq!(
            bgm_log(&mut d, 3600 * SEC, 3600 * SEC, SEC),
            [(3600 * SEC, turn(BGM_GAME_1))]
        );
        assert_eq!(
            bgm_log(&mut d, 3601 * SEC, 3780 * SEC, SEC),
            [(3780 * SEC, turn(BGM_GAME_3))]
        );
    }

    #[test]
    fn title_stops_the_rotation() {
        let mut d = director();
        continue_at(&mut d, 0, 20);
        bgm_log(&mut d, 0, 100 * SEC, SEC);
        let mut out = Vec::new();
        d.go_to_title(&mut out);
        assert_eq!(bgm_log(&mut d, 100 * SEC, 3600 * SEC, SEC), []);
        let mut out = Vec::new();
        d.start_game(MonoTime(3600 * SEC), &mut out);
        assert_eq!(out.last(), Some(&bgm(BGM_GAME_1, 0)));
        assert_eq!(bgm_log(&mut d, 3600 * SEC, 7200 * SEC, SEC), []);
    }

    #[test]
    fn real_lengths_come_from_samples() {
        let d = AudioDirector::new();
        let data = tm_assets::sound::sound_data().expect("音のデータ");
        assert_eq!(d.len_us.len(), SOUND_COUNT);
        for (i, s) in data.iter().enumerate() {
            assert_eq!(
                d.len_us[i],
                s.samples.len() as u64 * 1_000_000 / 22_050,
                "{}",
                s.name
            );
        }
        assert_eq!(
            d.len(SE_COIN),
            data[usize::from(SE_COIN.0)].samples.len() as u64 * 1_000_000 / 22_050
        );
    }
}
