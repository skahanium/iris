//! Provider-level circuit breaker to prevent repeated retries to failing providers.
//!
//! Tracks consecutive failures per provider. After N consecutive failures it opens
//! the circuit, blocking further requests for a cooldown period. After cooldown,
//! a single probe request is allowed (half-open). If the probe succeeds, the circuit
//! closes; if it fails, the circuit re-opens.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

const CONSECUTIVE_FAILURES_TO_OPEN: u32 = 5;
const COOLDOWN_DURATION_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitStatus {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
struct CircuitState {
    consecutive_failures: u32,
    opened_at: Option<Instant>,
    status: CircuitStatus,
}

static CIRCUITS: LazyLock<Mutex<HashMap<String, CircuitState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Check if a request to `provider_id` is allowed. Returns `true` if allowed.
/// Must be paired with `record_success` or `record_failure` after the request completes.
pub fn is_request_allowed(provider_id: &str) -> bool {
    let mut map = CIRCUITS.lock().unwrap_or_else(|e| e.into_inner());
    let state = map.entry(provider_id.to_string()).or_insert(CircuitState {
        consecutive_failures: 0,
        opened_at: None,
        status: CircuitStatus::Closed,
    });

    match state.status {
        CircuitStatus::Closed => true,
        CircuitStatus::Open => {
            if let Some(opened_at) = state.opened_at {
                if opened_at.elapsed().as_secs() >= COOLDOWN_DURATION_SECS {
                    state.status = CircuitStatus::HalfOpen;
                    tracing::info!(
                        provider = %provider_id,
                        "熔断器进入半开状态，允许探测请求"
                    );
                    return true;
                }
            }
            false
        }
        CircuitStatus::HalfOpen => true,
    }
}

pub fn record_success(provider_id: &str) {
    let mut map = CIRCUITS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = map.get_mut(provider_id) {
        let prev = state.status;
        state.consecutive_failures = 0;
        state.opened_at = None;
        state.status = CircuitStatus::Closed;
        if prev != CircuitStatus::Closed {
            tracing::info!(
                provider = %provider_id,
                "熔断器关闭，provider 已恢复"
            );
        }
    }
}

pub fn record_failure(provider_id: &str) {
    let mut map = CIRCUITS.lock().unwrap_or_else(|e| e.into_inner());
    let state = map.entry(provider_id.to_string()).or_insert(CircuitState {
        consecutive_failures: 0,
        opened_at: None,
        status: CircuitStatus::Closed,
    });

    state.consecutive_failures += 1;

    if state.status == CircuitStatus::HalfOpen {
        state.status = CircuitStatus::Open;
        state.opened_at = Some(Instant::now());
        tracing::warn!(
            provider = %provider_id,
            failures = state.consecutive_failures,
            "半开探测失败，熔断器重新打开"
        );
        return;
    }

    if state.consecutive_failures >= CONSECUTIVE_FAILURES_TO_OPEN {
        state.status = CircuitStatus::Open;
        state.opened_at = Some(Instant::now());
        tracing::warn!(
            provider = %provider_id,
            failures = state.consecutive_failures,
            cooldown_secs = COOLDOWN_DURATION_SECS,
            "熔断器打开，{} 秒内跳过该 provider",
            COOLDOWN_DURATION_SECS
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset_circuit(provider: &str) {
        let mut map = CIRCUITS.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(provider);
    }

    #[test]
    fn circuit_opens_after_consecutive_failures() {
        let prov = "circuit-test-open";
        reset_circuit(prov);
        for _ in 0..CONSECUTIVE_FAILURES_TO_OPEN - 1 {
            assert!(is_request_allowed(prov));
            record_failure(prov);
        }
        assert!(is_request_allowed(prov));
        record_failure(prov);
        assert!(!is_request_allowed(prov));
        reset_circuit(prov);
    }

    #[test]
    fn success_resets_circuit() {
        let prov = "circuit-test-reset";
        reset_circuit(prov);
        for _ in 0..CONSECUTIVE_FAILURES_TO_OPEN - 1 {
            record_failure(prov);
        }
        record_success(prov);
        for _ in 0..CONSECUTIVE_FAILURES_TO_OPEN - 1 {
            assert!(is_request_allowed(prov));
            record_failure(prov);
        }
        assert!(is_request_allowed(prov));
        record_failure(prov);
        assert!(!is_request_allowed(prov));
        reset_circuit(prov);
    }

    #[test]
    fn unknown_provider_is_allowed() {
        let prov = "circuit-test-unknown";
        reset_circuit(prov);
        assert!(is_request_allowed(prov));
        reset_circuit(prov);
    }
}
