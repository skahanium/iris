//! Dev-only NDJSON append for agent debug sessions (workspace `debug-*.log`).
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

const SESSION_LOG: &str = "debug-8589f0.log";

fn log_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(SESSION_LOG)
}

/// Append one NDJSON line from the `debug_session_log` IPC command or Rust internals.
pub fn append(payload: serde_json::Value) {
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    else {
        return;
    };
    if let Ok(line) = serde_json::to_string(&payload) {
        let _ = writeln!(file, "{line}");
    }
}
