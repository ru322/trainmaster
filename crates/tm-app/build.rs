//! 版の情報を `TM_RUSTC_VERSION` と `TM_GAME_VERSION` としてコンパイルに渡す(16 §1.14、APP-D-13)

use std::env;
use std::process::Command;

fn non_empty(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn rustc_version() -> String {
    if let Some(v) = non_empty("TM_RUSTC_VERSION") {
        return v;
    }
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=TM_RUSTC_VERSION");
    println!("cargo:rerun-if-env-changed=TM_BUILD_VERSION");
    println!("cargo:rerun-if-env-changed=RUSTC");
    let game = non_empty("TM_BUILD_VERSION")
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap_or_default());
    println!("cargo:rustc-env=TM_RUSTC_VERSION={}", rustc_version());
    println!("cargo:rustc-env=TM_GAME_VERSION={game}");
}
