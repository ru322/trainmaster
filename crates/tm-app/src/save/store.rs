//! セーブの置き場所: 2 つのスロットと `settings.ron` [16 §1.20、03 §5.7]。

use std::collections::VecDeque;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use log::warn;

use super::file::{MAX_SAVE_BYTES, SaveFile};

/// `settings.ron` の上限(バイト)。
pub const MAX_SETTINGS_BYTES: usize = 4096;
/// 設定のファイル名。
pub const SETTINGS_FILE: &str = "settings.ron";

/// セーブのスロット。同じ会社の新旧を交互に書く。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    /// `slot-a.tms`。
    A,
    /// `slot-b.tms`。
    B,
}

impl Slot {
    /// 両方。
    pub const ALL: [Slot; 2] = [Slot::A, Slot::B];

    /// もう一方。
    pub fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }

    /// ファイル名。
    pub fn file_name(self) -> &'static str {
        match self {
            Slot::A => "slot-a.tms",
            Slot::B => "slot-b.tms",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// 書き込みの途中の段階(電源断の模擬に使う)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteStage {
    /// 一時ファイルを半分まで書いた。
    TmpPartial,
    /// 一時ファイルを書いたが `fsync` の前。
    BeforeFsync,
    /// `fsync` の後、`rename` の前。
    BeforeRename,
    /// `rename` の後、ディレクトリの `fsync` の前。
    BeforeDirFsync,
}

/// セーブと設定の読み書き。検査は `SaveFile` が行う。
pub trait SaveStore: Send {
    /// 読めたスロットの中身(ないスロットは含めない)。
    fn load_slots(&mut self) -> Vec<(Slot, Vec<u8>)>;
    /// 一時ファイル → `fsync` → `rename` → ディレクトリの `fsync`。
    fn write_slot(&mut self, slot: Slot, bytes: &[u8]) -> io::Result<()>;
    /// 両方のスロットを消して、ディレクトリを `fsync`。
    fn clear(&mut self) -> io::Result<()>;
    /// `settings.ron` の中身。
    fn load_settings(&mut self) -> Option<Vec<u8>>;
    /// `settings.ron` をスロットと同じ書き方で書く。
    fn write_settings(&mut self, bytes: &[u8]) -> io::Result<()>;
}

/// ディレクトリのファイル(`slot-a.tms`、`slot-b.tms`、`settings.ron` と書きかけの `*.tmp`)。
#[derive(Debug)]
pub struct FsStore {
    dir: PathBuf,
    crash: Option<WriteStage>,
}

impl FsStore {
    /// ディレクトリがなければ作る。
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<FsStore> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        if !fs::metadata(&dir)?.is_dir() {
            return Err(io::Error::other("ディレクトリでない"));
        }
        Ok(FsStore { dir, crash: None })
    }

    /// 置き場所。
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    #[cfg(test)]
    pub(crate) fn crash_at(&mut self, stage: Option<WriteStage>) {
        self.crash = stage;
    }
}

fn tmp_name(name: &str) -> String {
    format!("{name}.tmp")
}

fn crashed() -> io::Error {
    io::Error::other("書き込みの途中で止めた(模擬)")
}

fn read_capped(path: &Path, max: usize) -> io::Result<Option<Vec<u8>>> {
    let f = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let len = f.metadata()?.len();
    if len > max as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{len} B は大きすぎる"),
        ));
    }
    let mut buf = Vec::with_capacity(len as usize);
    f.take(max as u64 + 1).read_to_end(&mut buf)?;
    if buf.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "大きすぎる"));
    }
    Ok(Some(buf))
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn write_atomic(dir: &Path, name: &str, bytes: &[u8], crash: Option<WriteStage>) -> io::Result<()> {
    let tmp = dir.join(tmp_name(name));
    let mut renamed = false;
    let mut run = || -> io::Result<()> {
        let mut f = File::create(&tmp)?;
        if crash == Some(WriteStage::TmpPartial) {
            f.write_all(&bytes[..bytes.len() / 2])?;
            return Err(crashed());
        }
        f.write_all(bytes)?;
        if crash == Some(WriteStage::BeforeFsync) {
            return Err(crashed());
        }
        f.sync_all()?;
        drop(f);
        if crash == Some(WriteStage::BeforeRename) {
            return Err(crashed());
        }
        fs::rename(&tmp, dir.join(name))?;
        renamed = true;
        if crash == Some(WriteStage::BeforeDirFsync) {
            return Err(crashed());
        }
        sync_dir(dir)
    };
    let r = run();
    if r.is_err() && !renamed && crash.is_none() {
        let _ = fs::remove_file(&tmp);
    }
    r
}

impl SaveStore for FsStore {
    fn load_slots(&mut self) -> Vec<(Slot, Vec<u8>)> {
        let mut out = Vec::new();
        for slot in Slot::ALL {
            match read_capped(&self.dir.join(slot.file_name()), MAX_SAVE_BYTES) {
                Ok(Some(b)) => out.push((slot, b)),
                Ok(None) => {}
                Err(e) => warn!("セーブ {} を読めない: {e}", slot.file_name()),
            }
        }
        out
    }

    fn write_slot(&mut self, slot: Slot, bytes: &[u8]) -> io::Result<()> {
        let _ = remove_if_exists(&self.dir.join(tmp_name(slot.other().file_name())));
        write_atomic(&self.dir, slot.file_name(), bytes, self.crash)
    }

    fn clear(&mut self) -> io::Result<()> {
        let mut first = Ok(());
        for slot in Slot::ALL {
            for name in [slot.file_name().to_owned(), tmp_name(slot.file_name())] {
                if let Err(e) = remove_if_exists(&self.dir.join(name)) {
                    first = first.and(Err(e));
                }
            }
        }
        first.and(sync_dir(&self.dir))
    }

    fn load_settings(&mut self) -> Option<Vec<u8>> {
        read_capped(&self.dir.join(SETTINGS_FILE), MAX_SETTINGS_BYTES).unwrap_or_else(|e| {
            warn!("{SETTINGS_FILE} を読めない: {e}");
            None
        })
    }

    fn write_settings(&mut self, bytes: &[u8]) -> io::Result<()> {
        write_atomic(&self.dir, SETTINGS_FILE, bytes, None)
    }
}

/// 保存しない(`TM_SAVE_DIR=off`)。何も読まず、書いたことにする。
#[derive(Clone, Copy, Debug, Default)]
pub struct NullStore;

impl SaveStore for NullStore {
    fn load_slots(&mut self) -> Vec<(Slot, Vec<u8>)> {
        Vec::new()
    }
    fn write_slot(&mut self, _slot: Slot, _bytes: &[u8]) -> io::Result<()> {
        Ok(())
    }
    fn clear(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn load_settings(&mut self) -> Option<Vec<u8>> {
        None
    }
    fn write_settings(&mut self, _bytes: &[u8]) -> io::Result<()> {
        Ok(())
    }
}

/// `MemStore` に書いた記録(新しいもの 64 個)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemWrite {
    /// 書いたスロット。
    pub slot: Slot,
    /// ヘッダの `seq`(読めなければ `None`)。
    pub seq: Option<u64>,
    /// ヘッダの `tick`。
    pub tick: Option<u64>,
}

/// `MemStore` の数。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemCounts {
    /// 成功したスロットの書き込み。
    pub slot_writes: u32,
    /// 失敗させたスロットの書き込み。
    pub failed_writes: u32,
    /// `clear` の回数。
    pub clears: u32,
    /// 成功した `settings.ron` の書き込み。
    pub settings_writes: u32,
}

const MEM_HISTORY: usize = 64;

#[derive(Default)]
struct MemInner {
    slots: [Option<Vec<u8>>; 2],
    tmp: [Option<Vec<u8>>; 2],
    settings: Option<Vec<u8>>,
    fail_writes: u32,
    panic_next: bool,
    crash: Option<WriteStage>,
    delay: Duration,
    hold: bool,
    held: bool,
    counts: MemCounts,
    history: VecDeque<MemWrite>,
}

/// メモリの置き場所(テスト用)。複製は同じ中身を指すので、別の `App` から読める。
/// 遅れ・失敗・書き込みの途中で止める位置を差し込める。
#[derive(Clone, Default)]
pub struct MemStore {
    inner: Arc<(Mutex<MemInner>, Condvar)>,
}

impl fmt::Debug for MemStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemStore({:?})", self.counts())
    }
}

impl MemStore {
    /// 空の置き場所。
    pub fn new() -> MemStore {
        MemStore::default()
    }

    fn lock(&self) -> MutexGuard<'_, MemInner> {
        self.inner.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// スロットの中身。
    pub fn slot(&self, slot: Slot) -> Option<Vec<u8>> {
        self.lock().slots[slot.index()].clone()
    }

    /// スロットの中身を差し替える(壊れたセーブの模擬)。
    pub fn set_slot(&self, slot: Slot, bytes: Option<Vec<u8>>) {
        self.lock().slots[slot.index()] = bytes;
    }

    /// 書きかけの一時ファイル。
    pub fn tmp(&self, slot: Slot) -> Option<Vec<u8>> {
        self.lock().tmp[slot.index()].clone()
    }

    /// `settings.ron` の中身。
    pub fn settings(&self) -> Option<Vec<u8>> {
        self.lock().settings.clone()
    }

    /// `settings.ron` を差し替える。
    pub fn set_settings(&self, bytes: Option<Vec<u8>>) {
        self.lock().settings = bytes;
    }

    /// 次の `n` 回のスロットの書き込みを失敗させる(`u32::MAX` でずっと)。
    pub fn fail_next_writes(&self, n: u32) {
        self.lock().fail_writes = n;
    }

    /// 次のスロットの書き込みでパニックする(書き込みスレッドの異常の模擬)。
    pub fn panic_next_write(&self) {
        self.lock().panic_next = true;
    }

    /// 次のスロットの書き込みを `stage` で止めて失敗させる(電源断の模擬)。
    pub fn crash_next_write(&self, stage: WriteStage) {
        self.lock().crash = Some(stage);
    }

    /// スロットの書き込みを毎回 `d` だけ遅らせる。
    pub fn set_delay(&self, d: Duration) {
        self.lock().delay = d;
    }

    /// `release` まで書き込みを止める。
    pub fn hold(&self) {
        self.lock().hold = true;
    }

    /// `hold` を解く。
    pub fn release(&self) {
        self.lock().hold = false;
        self.inner.1.notify_all();
    }

    /// `hold` で止まった書き込みがあるまで待つ。来なければ false。
    pub fn wait_held(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut g = self.lock();
        while !g.held {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            g = self
                .inner
                .1
                .wait_timeout(g, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }

    /// 数。
    pub fn counts(&self) -> MemCounts {
        self.lock().counts
    }

    /// 成功したスロットの書き込みの記録(古い順、新しい 64 個)。
    pub fn history(&self) -> Vec<MemWrite> {
        self.lock().history.iter().copied().collect()
    }
}

impl SaveStore for MemStore {
    fn load_slots(&mut self) -> Vec<(Slot, Vec<u8>)> {
        let g = self.lock();
        Slot::ALL
            .into_iter()
            .filter_map(|s| g.slots[s.index()].clone().map(|b| (s, b)))
            .collect()
    }

    fn write_slot(&mut self, slot: Slot, bytes: &[u8]) -> io::Result<()> {
        let mut g = self.lock();
        while g.hold {
            g.held = true;
            self.inner.1.notify_all();
            g = self.inner.1.wait(g).unwrap_or_else(|e| e.into_inner());
        }
        g.held = false;
        if std::mem::take(&mut g.panic_next) {
            drop(g);
            panic!("書き込みスレッドのパニック(模擬)");
        }
        let delay = g.delay;
        if !delay.is_zero() {
            drop(g);
            std::thread::sleep(delay);
            g = self.lock();
        }
        let i = slot.index();
        g.tmp[slot.other().index()] = None;
        if let Some(stage) = g.crash.take() {
            g.counts.failed_writes += 1;
            match stage {
                WriteStage::TmpPartial => g.tmp[i] = Some(bytes[..bytes.len() / 2].to_vec()),
                WriteStage::BeforeFsync | WriteStage::BeforeRename => {
                    g.tmp[i] = Some(bytes.to_vec())
                }
                WriteStage::BeforeDirFsync => {
                    g.tmp[i] = None;
                    g.slots[i] = Some(bytes.to_vec());
                }
            }
            return Err(crashed());
        }
        if g.fail_writes > 0 {
            if g.fail_writes != u32::MAX {
                g.fail_writes -= 1;
            }
            g.counts.failed_writes += 1;
            return Err(io::Error::other("書き込みの失敗(模擬)"));
        }
        g.tmp[i] = None;
        g.slots[i] = Some(bytes.to_vec());
        g.counts.slot_writes += 1;
        let h = SaveFile::header(bytes).ok();
        if g.history.len() == MEM_HISTORY {
            g.history.pop_front();
        }
        g.history.push_back(MemWrite {
            slot,
            seq: h.as_ref().map(|h| h.seq),
            tick: h.as_ref().map(|h| h.tick),
        });
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        let mut g = self.lock();
        g.slots = [None, None];
        g.tmp = [None, None];
        g.counts.clears += 1;
        Ok(())
    }

    fn load_settings(&mut self) -> Option<Vec<u8>> {
        self.lock().settings.clone()
    }

    fn write_settings(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut g = self.lock();
        g.settings = Some(bytes.to_vec());
        g.counts.settings_writes += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tm-app-store-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn fs_store_writes_reads_and_clears() {
        let dir = temp_dir("basic");
        let mut s = FsStore::open(dir.join("nested")).unwrap();
        assert_eq!(s.dir(), dir.join("nested"));
        assert!(s.load_slots().is_empty());
        assert!(s.load_settings().is_none());
        s.write_slot(Slot::A, b"one").unwrap();
        s.write_slot(Slot::B, b"two").unwrap();
        s.write_slot(Slot::A, b"three").unwrap();
        s.write_settings(b"settings").unwrap();
        assert_eq!(
            s.load_slots(),
            [(Slot::A, b"three".to_vec()), (Slot::B, b"two".to_vec())]
        );
        assert_eq!(s.load_settings().as_deref(), Some(&b"settings"[..]));
        assert_eq!(
            names(s.dir()),
            ["settings.ron", "slot-a.tms", "slot-b.tms"],
            "一時ファイルは残らない"
        );
        fs::write(s.dir().join("slot-a.tms.tmp"), b"half").unwrap();
        fs::write(s.dir().join("slot-b.tms.tmp"), b"half").unwrap();
        s.clear().unwrap();
        assert!(s.load_slots().is_empty());
        assert_eq!(
            names(s.dir()),
            ["settings.ron"],
            "書きかけも消す。設定は消さない"
        );
        s.clear().unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fs_store_skips_too_large_files() {
        let dir = temp_dir("large");
        let mut s = FsStore::open(&dir).unwrap();
        fs::write(dir.join("slot-a.tms"), vec![b'x'; MAX_SAVE_BYTES + 1]).unwrap();
        fs::write(dir.join("slot-b.tms"), vec![b'x'; MAX_SAVE_BYTES]).unwrap();
        fs::write(dir.join(SETTINGS_FILE), vec![b'x'; MAX_SETTINGS_BYTES + 1]).unwrap();
        let slots = s.load_slots();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].0, Slot::B);
        assert!(s.load_settings().is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fs_store_open_fails_on_a_file() {
        let dir = temp_dir("file");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("f"), b"").unwrap();
        assert!(FsStore::open(dir.join("f")).is_err());
        assert!(FsStore::open(dir.join("f").join("x")).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn app_t_26_crash_at_each_stage_keeps_the_last_complete_slot() {
        let stages = [
            WriteStage::TmpPartial,
            WriteStage::BeforeFsync,
            WriteStage::BeforeRename,
            WriteStage::BeforeDirFsync,
        ];
        for stage in stages {
            let dir = temp_dir(&format!("{stage:?}"));
            let mut s = FsStore::open(&dir).unwrap();
            s.write_slot(Slot::A, b"seq1").unwrap();
            s.write_slot(Slot::B, b"seq2").unwrap();
            s.crash_at(Some(stage));
            assert!(s.write_slot(Slot::A, b"seq3-new").is_err());
            let mut again = FsStore::open(&dir).unwrap();
            let slots = again.load_slots();
            let a = if stage == WriteStage::BeforeDirFsync {
                b"seq3-new".to_vec()
            } else {
                b"seq1".to_vec()
            };
            assert_eq!(
                slots,
                [(Slot::A, a), (Slot::B, b"seq2".to_vec())],
                "{stage:?}"
            );
            let tmp = dir.join("slot-a.tms.tmp");
            assert_eq!(
                tmp.exists(),
                stage != WriteStage::BeforeDirFsync,
                "{stage:?}"
            );
            again.write_slot(Slot::B, b"seq4").unwrap();
            assert!(!tmp.exists(), "次の保存で消える: {stage:?}");
            again.write_slot(Slot::A, b"seq5").unwrap();
            assert_eq!(names(&dir), ["slot-a.tms", "slot-b.tms"]);
            fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[test]
    fn failed_write_removes_its_tmp() {
        let dir = temp_dir("fail");
        let mut s = FsStore::open(&dir).unwrap();
        fs::create_dir_all(dir.join("slot-a.tms")).unwrap();
        fs::write(dir.join("slot-a.tms").join("x"), b"").unwrap();
        assert!(
            s.write_slot(Slot::A, b"data").is_err(),
            "ディレクトリへの rename は失敗する"
        );
        assert!(!dir.join("slot-a.tms.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    fn header(seq: u64) -> Vec<u8> {
        format!(
            "SaveHeader(magic: \"TMSAVE\", save_format: {}, rules_version: 2, config_hash: 0, game_version: \"\", seq: {seq}, tick: {}, state_hash: 0, body_fnv: 0)\nbody",
            super::super::SAVE_FORMAT_VERSION,
            seq * 10
        )
        .into_bytes()
    }

    #[test]
    fn mem_store_models_failures_and_crashes() {
        let m = MemStore::new();
        let mut s: Box<dyn SaveStore> = Box::new(m.clone());
        s.write_slot(Slot::A, &header(1)).unwrap();
        m.fail_next_writes(2);
        assert!(s.write_slot(Slot::B, &header(2)).is_err());
        assert!(s.write_slot(Slot::B, &header(2)).is_err());
        s.write_slot(Slot::B, &header(2)).unwrap();
        m.crash_next_write(WriteStage::TmpPartial);
        assert!(s.write_slot(Slot::A, &header(3)).is_err());
        assert_eq!(m.slot(Slot::A), Some(header(1)));
        assert_eq!(m.tmp(Slot::A).map(|t| t.len()), Some(header(3).len() / 2));
        m.crash_next_write(WriteStage::BeforeDirFsync);
        assert!(s.write_slot(Slot::B, &header(4)).is_err());
        assert_eq!(m.slot(Slot::B), Some(header(4)), "rename の後");
        assert_eq!(m.tmp(Slot::A), None, "次の保存で消える");
        s.write_slot(Slot::A, &header(5)).unwrap();
        assert_eq!(
            m.history(),
            [
                MemWrite {
                    slot: Slot::A,
                    seq: Some(1),
                    tick: Some(10)
                },
                MemWrite {
                    slot: Slot::B,
                    seq: Some(2),
                    tick: Some(20)
                },
                MemWrite {
                    slot: Slot::A,
                    seq: Some(5),
                    tick: Some(50)
                },
            ]
        );
        assert_eq!(
            m.counts(),
            MemCounts {
                slot_writes: 3,
                failed_writes: 4,
                clears: 0,
                settings_writes: 0
            }
        );
        m.fail_next_writes(u32::MAX);
        for _ in 0..3 {
            assert!(s.write_slot(Slot::B, &header(6)).is_err());
        }
        m.fail_next_writes(0);
        s.write_settings(b"x").unwrap();
        assert_eq!(s.load_settings().as_deref(), Some(&b"x"[..]));
        s.clear().unwrap();
        assert!(s.load_slots().is_empty());
        assert_eq!(m.settings().as_deref(), Some(&b"x"[..]));
        assert!(format!("{m:?}").starts_with("MemStore("));
        for _ in 0..MEM_HISTORY + 5 {
            s.write_slot(Slot::A, &header(7)).unwrap();
        }
        assert_eq!(m.history().len(), MEM_HISTORY);
    }

    #[test]
    fn mem_store_hold_blocks_until_release() {
        let m = MemStore::new();
        m.hold();
        let mut s = m.clone();
        let t = std::thread::spawn(move || s.write_slot(Slot::A, b"x"));
        assert!(m.wait_held(Duration::from_secs(5)));
        assert_eq!(m.slot(Slot::A), None);
        m.release();
        t.join().unwrap().unwrap();
        assert_eq!(m.slot(Slot::A).as_deref(), Some(&b"x"[..]));
        assert!(!MemStore::new().wait_held(Duration::from_millis(1)));
    }

    #[test]
    fn null_store_does_nothing() {
        let mut s = NullStore;
        s.write_slot(Slot::A, b"x").unwrap();
        s.write_settings(b"x").unwrap();
        s.clear().unwrap();
        assert!(s.load_slots().is_empty());
        assert!(s.load_settings().is_none());
        assert_eq!(Slot::A.other(), Slot::B);
        assert_eq!(Slot::B.other(), Slot::A);
    }
}
