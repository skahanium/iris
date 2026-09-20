//! Shared C13 model-turn ledger for Provider HTTP and native search POSTs.
//!
//! Outer tool-loop rounds still stop the conversation. The quota itself counts
//! every real HTTPS attempt bound to the Run, including retries, failovers, and
//! native search subrequests.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::ai_runtime::run_contract::SafeRunErrorCode;
use crate::error::{AppError, AppResult};

struct Slot {
    used: u32,
    max: u32,
}

fn slots() -> &'static Mutex<HashMap<String, Slot>> {
    static SLOTS: OnceLock<Mutex<HashMap<String, Slot>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_slots() -> std::sync::MutexGuard<'static, HashMap<String, Slot>> {
    slots().lock().unwrap_or_else(|error| error.into_inner())
}

/// Bind `run_id` to a C13 ceiling for the lifetime of the current loop.
/// Nested guards keep the existing used count so a child loop cannot reset the parent.
pub(crate) fn bind(run_id: &str, max: u32) -> bool {
    let mut slots = lock_slots();
    if slots.contains_key(run_id) {
        return false;
    }
    slots.insert(run_id.to_string(), Slot { used: 0, max });
    true
}

/// Drop the Run's C13 slot. Unbound claims are no-ops so isolated tests still POST.
pub(crate) fn unbind(run_id: &str) {
    lock_slots().remove(run_id);
}

/// RAII bind so every execute_internal return path unbinds the slot.
pub(crate) struct BindGuard {
    run_id: String,
    owns: bool,
}

impl BindGuard {
    pub(crate) fn new(run_id: impl Into<String>, max: u32) -> Self {
        let run_id = run_id.into();
        let owns = bind(&run_id, max);
        Self { run_id, owns }
    }
}

impl Drop for BindGuard {
    fn drop(&mut self) {
        if self.owns {
            unbind(&self.run_id);
        }
    }
}

/// Record one real Provider HTTP or native POST. Unbound Runs are not enforced.
pub(crate) fn claim(run_id: &str) -> AppResult<u32> {
    let mut slots = lock_slots();
    let Some(slot) = slots.get_mut(run_id) else {
        return Ok(0);
    };
    if slot.used >= slot.max {
        return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
    }
    slot.used = slot.used.saturating_add(1);
    Ok(slot.used)
}

/// Current C13 count. Unbound Runs report zero so outer-loop stop stays semantic.
pub(crate) fn used(run_id: &str) -> u32 {
    lock_slots()
        .get(run_id)
        .map(|slot| slot.used)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbound_claim_is_a_noop() {
        let run_id = "ledger-unbound";
        unbind(run_id);
        assert_eq!(claim(run_id).expect("unbound"), 0);
        assert_eq!(used(run_id), 0);
    }

    #[test]
    fn claims_count_retries_until_the_ceiling() {
        let run_id = "ledger-retry";
        let _guard = BindGuard::new(run_id, 2);
        assert_eq!(claim(run_id).expect("first http"), 1);
        assert_eq!(claim(run_id).expect("retry http"), 2);
        assert!(claim(run_id).is_err(), "third attempt must not send");
        assert_eq!(used(run_id), 2);
    }

    #[test]
    fn drop_unbinds_so_a_later_run_is_not_charged() {
        let run_id = "ledger-drop";
        {
            let _guard = BindGuard::new(run_id, 1);
            assert_eq!(claim(run_id).expect("bound"), 1);
        }
        assert_eq!(used(run_id), 0);
        assert_eq!(claim(run_id).expect("unbound after drop"), 0);
    }

    #[test]
    fn nested_bind_does_not_reset_or_unbind_the_parent_slot() {
        let run_id = "ledger-nested";
        let parent = BindGuard::new(run_id, 4);
        assert_eq!(claim(run_id).expect("parent http"), 1);
        {
            let _child = BindGuard::new(run_id, 4);
            assert_eq!(claim(run_id).expect("child http"), 2);
        }
        assert_eq!(used(run_id), 2);
        drop(parent);
        assert_eq!(used(run_id), 0);
    }
}
