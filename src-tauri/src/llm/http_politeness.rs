//! Shared HTTP politeness: per-host throttling for outbound fetches.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

static HOST_LAST_REQUEST: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(not(test))]
const MIN_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(test)]
const MIN_INTERVAL: Duration = Duration::from_millis(25);
const HOST_CACHE_LIMIT: usize = 1_024;
const HOST_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

/// Async throttle: atomically reserve a per-host request slot, then sleep until it.
/// The reservation is made while holding the mutex, so concurrent callers cannot
/// observe the same prior timestamp. The cache is bounded to prevent host churn
/// from growing process memory without limit.
pub async fn throttle_host(host: &str) -> AppResult<()> {
    let key = host.trim().to_lowercase();
    if key.is_empty() {
        return Ok(());
    }
    let wait = {
        let mut map = HOST_LAST_REQUEST
            .lock()
            .map_err(|_| AppError::msg("http_politeness_state_unavailable"))?;
        let now = Instant::now();
        map.retain(|_, slot| {
            now.checked_duration_since(*slot)
                .is_none_or(|age| age <= HOST_CACHE_TTL)
        });
        if map.len() >= HOST_CACHE_LIMIT {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, slot)| **slot)
                .map(|(host, _)| host.clone())
            {
                map.remove(&oldest);
            }
        }
        let reserved = map
            .get(&key)
            .and_then(|previous| previous.checked_add(MIN_INTERVAL))
            .map_or(now, |next| next.max(now));
        map.insert(key, reserved);
        reserved.saturating_duration_since(now)
    };

    if !wait.is_zero() {
        tokio::time::sleep(wait).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn throttle_host_accepts_empty() {
        throttle_host("").await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_waiters_reserve_distinct_host_slots() {
        let host = "politeness-race.invalid";
        throttle_host(host).await.unwrap();
        let started = Instant::now();
        let (first, second) = tokio::join!(throttle_host(host), throttle_host(host));
        first.unwrap();
        second.unwrap();
        assert!(started.elapsed() >= MIN_INTERVAL * 2);
    }
}
