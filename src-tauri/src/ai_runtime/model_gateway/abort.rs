use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static ABORT_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();
static WEB_REVOKED: AtomicU64 = AtomicU64::new(0);
static WEB_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();

static ABORTED_REQUESTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn aborted_requests() -> &'static Mutex<HashSet<String>> {
    ABORTED_REQUESTS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Request cancellation for an active harness/model request.
pub fn request_abort(request_id: &str) {
    if let Ok(mut aborted) = aborted_requests().lock() {
        aborted.insert(request_id.to_string());
    }
    ABORT_NOTIFY.notify_waiters();
}

/// Wait for cancellation, including cancellation of this scope's parent.
pub(crate) async fn wait_for_abort(request_id: &str) {
    loop {
        let notified = ABORT_NOTIFY.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if is_abort_requested(request_id) {
            return;
        }
        notified.await;
    }
}

/// Capture the revocation generation once per web action, never per retry.
pub(crate) fn web_revocation_epoch() -> u64 {
    WEB_REVOKED.load(Ordering::Acquire)
}

/// Re-enabling networking cannot revive an action from an earlier generation.
pub(crate) fn notify_web_revoked() {
    WEB_REVOKED.fetch_add(1, Ordering::AcqRel);
    WEB_NOTIFY.notify_waiters();
}

/// Register before rechecking so an intervening revoke cannot lose its signal.
pub(crate) async fn wait_for_web_revocation(epoch: u64) {
    loop {
        let notified = WEB_NOTIFY.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if web_revocation_epoch() != epoch {
            return;
        }
        notified.await;
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

    #[tokio::test]
    async fn review_regression_d_waiters_observe_existing_and_in_flight_revocation() {
        let run = "review-regression-d-cancel";
        let child = crate::ai_runtime::agent_tool_loop::scoped_child_provider_run_id(run, "child");
        let waiting = wait_for_abort(&child);
        tokio::pin!(waiting);
        assert!(futures_util::poll!(&mut waiting).is_pending());
        request_abort(run);
        assert!(futures_util::poll!(&mut waiting).is_ready());
        assert!(futures_util::poll!(Box::pin(wait_for_abort(run))).is_ready());
        clear_abort(run);
        let epoch = web_revocation_epoch();
        notify_web_revoked();
        assert!(futures_util::poll!(Box::pin(wait_for_web_revocation(epoch))).is_ready());
    }

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
