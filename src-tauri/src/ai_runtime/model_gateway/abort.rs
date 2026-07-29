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
    let parent_request_id =
        crate::ai_runtime::agent_tool_loop::parent_run_id_for_provider_scope(request_id);
    aborted_requests()
        .lock()
        .map(|aborted| {
            aborted.contains(request_id)
                || (parent_request_id != request_id && aborted.contains(parent_request_id))
        })
        .unwrap_or(false)
}

/// Clear a cancellation marker after the active request observes it.
pub fn clear_abort(request_id: &str) {
    if let Ok(mut aborted) = aborted_requests().lock() {
        aborted.remove(request_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_children_inherit_parent_abort_without_clearing_it() {
        let parent = "run-parent-abort-scope";
        let children = (1..=3)
            .map(|index| {
                crate::ai_runtime::agent_tool_loop::scoped_child_provider_run_id(
                    parent,
                    &format!("child-{index}"),
                )
            })
            .collect::<Vec<_>>();

        request_abort(parent);
        assert!(children.iter().all(|child| is_abort_requested(child)));

        clear_abort(&children[0]);
        assert!(
            children.iter().all(|child| is_abort_requested(child)),
            "one completed child must not clear the parent cancellation marker"
        );
        assert!(is_abort_requested(parent));

        clear_abort(parent);
        assert!(children.iter().all(|child| !is_abort_requested(child)));
    }
}
