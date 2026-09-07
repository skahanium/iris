//! In-process presence of a Run worker that may still mutate disk after the
//! durable status has become `cancelled`.
//!
//! Session delete/retract must not tear down Run receipts while this marker is
//! set. The marker is independent of `agent_runs.status`: cancellation may land
//! in SQLite before the worker's tools and writes have actually exited.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

static INFLIGHT_RUNS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn inflight_runs() -> &'static Mutex<HashSet<String>> {
    INFLIGHT_RUNS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Record that a Run worker is executing and may still write.
pub(crate) fn mark(run_id: &str) {
    if let Ok(mut inflight) = inflight_runs().lock() {
        inflight.insert(run_id.to_string());
    }
}

/// Clear the worker marker after teardown, including cancellation unwind.
pub(crate) fn clear(run_id: &str) {
    if let Ok(mut inflight) = inflight_runs().lock() {
        inflight.remove(run_id);
    }
}

/// Return whether a Run worker is still in process.
pub(crate) fn is_marked(run_id: &str) -> bool {
    inflight_runs()
        .lock()
        .map(|inflight| inflight.contains(run_id))
        .unwrap_or(false)
}

pub(crate) struct InflightGuard {
    run_id: String,
}

impl InflightGuard {
    pub(crate) fn new(run_id: impl Into<String>) -> Self {
        let run_id = run_id.into();
        mark(&run_id);
        Self { run_id }
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        clear(&self.run_id);
    }
}
