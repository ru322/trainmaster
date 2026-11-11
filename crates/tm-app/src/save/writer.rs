//! 書き込みスレッド: ファイルへの書き込みと `fsync` をフレームから外す [16 §1.20、02 §2.4]。

use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use log::{debug, info, warn};

use super::store::{SaveStore, Slot};

/// `Drop` で書き込みの終わりを待つ上限。
const DROP_WAIT: Duration = Duration::from_secs(5);

/// 書き込みの結果の数と時間。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WriterStats {
    pub saved: u32,
    pub failed: u32,
    pub consecutive_failures: u32,
    pub write_us_max: u64,
    pub write_us_total: u64,
}

#[derive(Default)]
struct Queue {
    clear: bool,
    save: Option<Vec<u8>>,
    settings: Option<Vec<u8>>,
    busy: bool,
    stop: bool,
    exited: bool,
    stats: WriterStats,
}

impl Queue {
    fn has_work(&self) -> bool {
        self.clear || self.save.is_some() || self.settings.is_some()
    }
}

struct Shared {
    q: Mutex<Queue>,
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.q.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// スレッドの終わり(パニックを含む)を知らせる。
struct ExitGuard<'a>(&'a Shared);

impl Drop for ExitGuard<'_> {
    fn drop(&mut self) {
        let mut q = self.0.lock();
        q.exited = true;
        drop(q);
        self.0.cv.notify_all();
    }
}

/// 書き込みスレッド 1 本。待っているセーブは最新の 1 つだけを残す。
pub(crate) struct Saver {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
    drop_wait: Duration,
}

impl Saver {
    /// `first` は最初のセーブを書くスロット。
    pub(crate) fn spawn(store: Box<dyn SaveStore>, first: Slot) -> io::Result<Saver> {
        let shared = Arc::new(Shared {
            q: Mutex::new(Queue::default()),
            cv: Condvar::new(),
        });
        let s = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name("tm-save".into())
            .spawn(move || {
                let _exit = ExitGuard(&s);
                run(&s, store, first);
            })?;
        Ok(Saver {
            shared,
            handle: Some(handle),
            drop_wait: DROP_WAIT,
        })
    }

    /// セーブを書く。まだ書いていない前のセーブは捨てる。
    pub(crate) fn submit(&self, bytes: Vec<u8>) {
        self.shared.lock().save = Some(bytes);
        self.shared.cv.notify_all();
    }

    /// `settings.ron` を書く。
    pub(crate) fn submit_settings(&self, bytes: Vec<u8>) {
        self.shared.lock().settings = Some(bytes);
        self.shared.cv.notify_all();
    }

    /// 両方のスロットを消す。まだ書いていないセーブは捨てる(書いている途中のものは消す側が後)。
    pub(crate) fn clear(&self) {
        let mut q = self.shared.lock();
        q.save = None;
        q.clear = true;
        drop(q);
        self.shared.cv.notify_all();
    }

    /// 書き込み中か、待っているものがある(スレッドが終わっていれば false)。
    pub(crate) fn busy(&self) -> bool {
        let q = self.shared.lock();
        !q.exited && (q.busy || q.has_work())
    }

    /// 書き込みスレッドが動いている(パニックで終わっていない)。
    pub(crate) fn alive(&self) -> bool {
        !self.shared.lock().exited
    }

    pub(crate) fn stats(&self) -> WriterStats {
        self.shared.lock().stats
    }

    /// 空くまで待つ。空けば true。
    pub(crate) fn wait_idle(&self, timeout: Duration) -> bool {
        self.wait(timeout, |q| q.exited || (!q.busy && !q.has_work()))
    }

    /// 書き込みを諦める。`Drop` で書き込みの終わりを待たない(16 §1.15)。
    pub(crate) fn abandon(&mut self) {
        self.drop_wait = Duration::ZERO;
    }

    fn wait(&self, timeout: Duration, done: impl Fn(&Queue) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        let mut q = self.shared.lock();
        while !done(&q) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            q = self
                .shared
                .cv
                .wait_timeout(q, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }
}

impl Drop for Saver {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.cv.notify_all();
        if self.wait(self.drop_wait, |q| q.exited) {
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        } else {
            warn!("セーブの書き込みが終わらない。待たずに進む");
        }
    }
}

fn run(shared: &Shared, mut store: Box<dyn SaveStore>, mut next: Slot) {
    loop {
        let (clear, save, settings) = {
            let mut q = shared.lock();
            while !q.has_work() {
                if q.stop {
                    return;
                }
                q = shared.cv.wait(q).unwrap_or_else(|e| e.into_inner());
            }
            q.busy = true;
            (
                std::mem::take(&mut q.clear),
                q.save.take(),
                q.settings.take(),
            )
        };
        if clear {
            match store.clear() {
                Ok(()) => info!("セーブを消した"),
                Err(e) => warn!("セーブを消せない: {e}"),
            }
            next = Slot::A;
        }
        let mut result = None;
        if let Some(bytes) = save {
            let t = Instant::now();
            let r = store.write_slot(next, &bytes);
            let us = t.elapsed().as_micros() as u64;
            match &r {
                Ok(()) => {
                    debug!("セーブ {next:?} に {} B({us}µs)", bytes.len());
                    next = next.other();
                }
                Err(e) => warn!("セーブ {next:?} を書けない: {e}"),
            }
            result = Some((r.is_ok(), us));
        }
        if let Some(bytes) = settings
            && let Err(e) = store.write_settings(&bytes)
        {
            warn!("設定を書けない: {e}");
        }
        let mut q = shared.lock();
        q.busy = false;
        if let Some((ok, us)) = result {
            let st = &mut q.stats;
            if ok {
                st.saved += 1;
                st.consecutive_failures = 0;
                st.write_us_max = st.write_us_max.max(us);
                st.write_us_total += us;
            } else {
                st.failed += 1;
                st.consecutive_failures += 1;
            }
        }
        drop(q);
        shared.cv.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::store::{MemStore, MemWrite};

    fn bytes(seq: u64) -> Vec<u8> {
        format!(
            "SaveHeader(magic: \"TMSAVE\", save_format: 1, rules_version: 2, config_hash: 0, game_version: \"\", seq: {seq}, tick: 0, state_hash: 0, body_fnv: 0)\n"
        )
        .into_bytes()
    }

    fn seqs(m: &MemStore) -> Vec<(Slot, Option<u64>)> {
        m.history()
            .iter()
            .map(|w: &MemWrite| (w.slot, w.seq))
            .collect()
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn keeps_only_the_latest_waiting_save_and_alternates_slots() {
        let m = MemStore::new();
        let w = Saver::spawn(Box::new(m.clone()), Slot::B).unwrap();
        m.hold();
        w.submit(bytes(1));
        assert!(m.wait_held(WAIT));
        assert!(w.busy());
        w.submit(bytes(2));
        w.submit(bytes(3));
        w.submit_settings(b"s".to_vec());
        m.release();
        assert!(w.wait_idle(WAIT));
        assert!(!w.busy());
        assert_eq!(seqs(&m), [(Slot::B, Some(1)), (Slot::A, Some(3))]);
        assert_eq!(m.settings().as_deref(), Some(&b"s"[..]));
        let st = w.stats();
        assert_eq!((st.saved, st.failed, st.consecutive_failures), (2, 0, 0));
    }

    #[test]
    fn clear_drops_the_waiting_save_and_runs_after_the_current_write() {
        let m = MemStore::new();
        let w = Saver::spawn(Box::new(m.clone()), Slot::A).unwrap();
        m.hold();
        w.submit(bytes(1));
        assert!(m.wait_held(WAIT));
        w.submit(bytes(2));
        w.clear();
        w.submit(bytes(3));
        m.release();
        assert!(w.wait_idle(WAIT));
        assert_eq!(seqs(&m), [(Slot::A, Some(1)), (Slot::A, Some(3))]);
        assert_eq!(m.slot(Slot::A), Some(bytes(3)));
        assert_eq!(m.slot(Slot::B), None);
        assert_eq!(m.counts().clears, 1);

        m.hold();
        w.submit(bytes(4));
        assert!(m.wait_held(WAIT));
        w.submit(bytes(5));
        w.clear();
        m.release();
        assert!(w.wait_idle(WAIT));
        assert_eq!(
            m.slot(Slot::A),
            None,
            "待っていたセーブは消した後に書かない"
        );
        assert_eq!(m.slot(Slot::B), None);
    }

    #[test]
    fn failures_are_counted_and_retry_the_same_slot() {
        let m = MemStore::new();
        let w = Saver::spawn(Box::new(m.clone()), Slot::A).unwrap();
        let save = |seq| {
            w.submit(bytes(seq));
            assert!(w.wait_idle(WAIT));
        };
        save(1);
        m.fail_next_writes(1);
        save(2);
        assert_eq!(w.stats().consecutive_failures, 1);
        save(3);
        assert_eq!(seqs(&m), [(Slot::A, Some(1)), (Slot::B, Some(3))]);
        m.fail_next_writes(2);
        save(4);
        save(5);
        assert_eq!(w.stats().consecutive_failures, 2);
        assert_eq!(
            m.slot(Slot::B),
            Some(bytes(3)),
            "失敗の間も前のスロットを残す"
        );
        save(6);
        let st = w.stats();
        assert_eq!((st.saved, st.failed, st.consecutive_failures), (3, 3, 0));
        assert_eq!(seqs(&m)[2], (Slot::A, Some(6)));
    }

    #[test]
    fn a_panic_in_the_thread_does_not_leave_it_busy() {
        let m = MemStore::new();
        let w = Saver::spawn(Box::new(m.clone()), Slot::A).unwrap();
        m.panic_next_write();
        w.submit(bytes(1));
        assert!(w.wait_idle(WAIT));
        assert!(!w.alive());
        w.submit(bytes(2));
        assert!(!w.busy(), "終わったスレッドの仕事は待たない");
        let t = Instant::now();
        drop(w);
        assert!(t.elapsed() < WAIT, "Drop で待ち続けない");
        assert_eq!(m.slot(Slot::A), None);
    }

    #[test]
    fn drop_finishes_the_waiting_write() {
        let m = MemStore::new();
        m.set_delay(Duration::from_millis(20));
        let w = Saver::spawn(Box::new(m.clone()), Slot::A).unwrap();
        w.submit(bytes(1));
        drop(w);
        assert_eq!(m.slot(Slot::A), Some(bytes(1)));
    }
}
