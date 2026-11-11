//! リプレイログの書き出し [02 D10、16 §1.10、APP-D-06]。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use log::{info, warn};
use tm_core::ReplayLog;

/// リプレイログを記録して書き出すか、どこへ書くか。
#[derive(Clone, Debug)]
pub enum ReplayPolicy {
    /// 記録しない(配布用の既定)。
    Off,
    /// このディレクトリにファイルで書く(開発用。既定は `/tmp`)。
    Dir(PathBuf),
    /// メモリに積む(テスト用)。
    Memory(Arc<Mutex<Vec<ReplayLog>>>),
}

/// 書き出し先。失敗は警告だけで、展示は止めない。
#[derive(Debug)]
pub struct ReplaySink {
    policy: ReplayPolicy,
    written: u32,
}

impl ReplaySink {
    /// 方針から作る。
    pub fn new(policy: ReplayPolicy) -> Self {
        ReplaySink { policy, written: 0 }
    }

    /// 記録するか(`Off` でなければ true)。
    pub fn enabled(&self) -> bool {
        !matches!(self.policy, ReplayPolicy::Off)
    }

    /// `no` 番目のゲームのログを書き出す。書けたら true。
    pub fn write(&mut self, log: &ReplayLog, no: u32) -> bool {
        let ok = match &self.policy {
            ReplayPolicy::Off => false,
            ReplayPolicy::Memory(v) => {
                v.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(log.clone());
                true
            }
            ReplayPolicy::Dir(dir) => {
                let path = dir.join(file_name(no, log.header.seed));
                match write_atomic(&path, &log.to_ron()) {
                    Ok(()) => {
                        info!("リプレイを書き出した: {}", path.display());
                        true
                    }
                    Err(e) => {
                        warn!("リプレイを書き出せない({}): {e}", path.display());
                        false
                    }
                }
            }
        };
        self.written += u32::from(ok);
        ok
    }

    /// 書き出した数。
    pub fn written(&self) -> u32 {
        self.written
    }
}

/// `trainmaster-replay-<起動からの番号 4 桁>-<シード 16 進>.ron`。
pub fn file_name(no: u32, seed: u64) -> String {
    format!("trainmaster-replay-{no:04}-{seed:016x}.ron")
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let result = fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tm_core::{REPLAY_FORMAT_VERSION, RULES_VERSION, ReplayHeader, TICKS_PER_SEC};

    fn log(seed: u64) -> ReplayLog {
        ReplayLog::new(ReplayHeader {
            format: REPLAY_FORMAT_VERSION,
            rules_version: RULES_VERSION,
            game_version: "0".into(),
            rustc_version: "rustc".into(),
            ticks_per_sec: TICKS_PER_SEC,
            seed,
            config_hash: 0,
        })
    }

    #[test]
    fn file_name_has_number_and_hex_seed() {
        assert_eq!(
            file_name(3, 0xDEAD_BEEF),
            "trainmaster-replay-0003-00000000deadbeef.ron"
        );
    }

    #[test]
    fn dir_writes_whole_file_without_tmp() {
        let dir = std::env::temp_dir().join(format!("tm-app-replay-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut sink = ReplaySink::new(ReplayPolicy::Dir(dir.clone()));
        assert!(sink.write(&log(42), 1));
        let path = dir.join(file_name(1, 42));
        let back = ReplayLog::from_ron(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back, log(42));
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(sink.written(), 1);
    }

    #[test]
    fn failure_is_only_a_warning() {
        let dir = std::env::temp_dir().join("tm-app-replay-missing/none");
        let mut sink = ReplaySink::new(ReplayPolicy::Dir(dir));
        assert!(!sink.write(&log(1), 1));
        assert_eq!(sink.written(), 0);
        assert!(!ReplaySink::new(ReplayPolicy::Off).enabled());
    }
}
