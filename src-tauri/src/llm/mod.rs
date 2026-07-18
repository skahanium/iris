pub mod config;
pub mod engine;
pub mod fetch_web_page;
pub mod http_politeness;
pub mod model_catalog;
pub mod model_registry;
pub mod providers;
pub mod search_web;

use std::sync::{Mutex, MutexGuard};

/// 安全获取 mutex 锁，处理中毒情况
pub fn safe_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        let backtrace = std::backtrace::Backtrace::capture();
        tracing::warn!(
            backtrace = %backtrace,
            "Mutex poisoned, recovering inner data"
        );
        poisoned.into_inner()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_lock_normal() {
        let mutex = Mutex::new(42);
        let guard = safe_lock(&mutex);
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_safe_lock_poisoned() {
        let mutex = Mutex::new(42);

        // Poison the mutex
        let _ = std::panic::catch_unwind(|| {
            let _guard = mutex.lock().unwrap();
            panic!("poison");
        });

        // Verify mutex is poisoned
        assert!(mutex.lock().is_err());

        // safe_lock should still work
        let guard = safe_lock(&mutex);
        assert_eq!(*guard, 42);
    }
}
