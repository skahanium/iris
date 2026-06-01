//! Shared HTTP politeness: per-host throttling for outbound fetches.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

static HOST_LAST_REQUEST: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const MIN_INTERVAL_SECS: u64 = 2;

/// Block until at least `MIN_INTERVAL_SECS` since the last request to this host.
pub fn throttle_host(host: &str) -> AppResult<()> {
    let key = host.trim().to_lowercase();
    if key.is_empty() {
        return Ok(());
    }
    let mut map = HOST_LAST_REQUEST
        .lock()
        .map_err(|_| AppError::msg("Lock error"))?;
    if let Some(t) = map.get(&key) {
        let elapsed = t.elapsed();
        if elapsed < Duration::from_secs(MIN_INTERVAL_SECS) {
            std::thread::sleep(Duration::from_secs(MIN_INTERVAL_SECS) - elapsed);
        }
    }
    map.insert(key, Instant::now());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_host_accepts_empty() {
        throttle_host("").unwrap();
    }
}
