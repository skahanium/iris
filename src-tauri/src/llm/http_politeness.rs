//! Shared HTTP politeness: per-host throttling for outbound fetches.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

static HOST_LAST_REQUEST: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const MIN_INTERVAL_SECS: u64 = 2;

/// Async throttle: sleep until at least `MIN_INTERVAL_SECS` since the last request to this host.
/// Lock is released before the async sleep to avoid blocking the runtime.
pub async fn throttle_host(host: &str) -> AppResult<()> {
    let key = host.trim().to_lowercase();
    if key.is_empty() {
        return Ok(());
    }
    let need_wait = {
        let map = HOST_LAST_REQUEST
            .lock()
            .map_err(|_| AppError::msg("Lock error"))?;
        if let Some(t) = map.get(&key) {
            let elapsed = t.elapsed();
            if elapsed < Duration::from_secs(MIN_INTERVAL_SECS) {
                Some(Duration::from_secs(MIN_INTERVAL_SECS) - elapsed)
            } else {
                None
            }
        } else {
            None
        }
    }; // lock dropped here

    if let Some(wait) = need_wait {
        tokio::time::sleep(tokio::time::Duration::from_millis(wait.as_millis() as u64)).await;
    }

    HOST_LAST_REQUEST
        .lock()
        .map_err(|_| AppError::msg("Lock error"))?
        .insert(key, Instant::now());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn throttle_host_accepts_empty() {
        throttle_host("").await.unwrap();
    }
}
