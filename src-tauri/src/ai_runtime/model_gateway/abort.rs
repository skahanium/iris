use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

static ABORTED_REQUESTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn aborted_requests() -> &'static Mutex<HashSet<String>> {
    ABORTED_REQUESTS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Request cancellation for an active harness/model request.
pub fn request_abort(request_id: &str) {
    if let Ok(mut aborted) = aborted_requests().lock() {
        aborted.insert(request_id.to_string());
    }
}

/// Return whether a request has been marked for cancellation.
pub fn is_abort_requested(request_id: &str) -> bool {
    aborted_requests()
        .lock()
        .map(|aborted| aborted.contains(request_id))
        .unwrap_or(false)
}

/// Clear a cancellation marker after the active request observes it.
pub fn clear_abort(request_id: &str) {
    if let Ok(mut aborted) = aborted_requests().lock() {
        aborted.remove(request_id);
    }
}
