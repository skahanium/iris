use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::error::AppResult;

#[derive(Debug)]
pub struct VaultAttemptState {
    pub failures: u32,
    pub locked_until: Option<Instant>,
}

pub struct BruteForceProtection {
    attempts: Mutex<HashMap<String, VaultAttemptState>>,
}

impl BruteForceProtection {
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
        }
    }

    fn key(vault_path: &std::path::Path) -> String {
        vault_path.to_string_lossy().into_owned()
    }

    /// 检查是否允许解锁尝试。
    ///
    /// - `locked_until` 过期时：若 `failures >= 10` 说明是硬锁，重置计数器；
    ///   否则是软退避（5-9 次），仅清除定时锁但保留失败计数，使后续失败继续累加。
    pub fn check(&self, vault_path: &std::path::Path) -> AppResult<()> {
        let mut guard = self
            .attempts
            .lock()
            .map_err(|_| crate::error::AppError::msg("Brute force lock error"))?;
        let state = guard
            .entry(Self::key(vault_path))
            .or_insert_with(|| VaultAttemptState {
                failures: 0,
                locked_until: None,
            });

        if let Some(locked_until) = state.locked_until {
            if Instant::now() < locked_until {
                let remaining = (locked_until - Instant::now()).as_secs();
                return Err(crate::error::AppError::msg(format!(
                    "密码错误次数过多，保险库已锁定，请 {} 秒后重试",
                    remaining
                )));
            }
            // 软退避（5-9 次）过期后保留失败计数，使后续失败继续累加至硬锁。
            // 硬锁（>=10 次）过期后完全重置。
            if state.failures >= 10 {
                state.failures = 0;
            }
            state.locked_until = None;
        }

        Ok(())
    }

    /// 记录一次失败尝试。5-9 次时设置指数退避定时锁；>=10 次时设置 30 分钟硬锁。
    pub fn record_failure(&self, vault_path: &std::path::Path) -> AppResult<()> {
        let mut guard = self
            .attempts
            .lock()
            .map_err(|_| crate::error::AppError::msg("Brute force lock error"))?;
        let state = guard
            .entry(Self::key(vault_path))
            .or_insert_with(|| VaultAttemptState {
                failures: 0,
                locked_until: None,
            });

        state.failures = state.failures.saturating_add(1);

        if state.failures >= 10 {
            state.locked_until = Some(Instant::now() + std::time::Duration::from_secs(1800)); // 30 min
            tracing::warn!(
                failures = state.failures,
                "涉密保险库因连续密码错误已锁定 30 分钟"
            );
        } else if state.failures >= 5 {
            let delay = 5u64
                .saturating_mul(2u64.saturating_pow(state.failures.saturating_sub(5)))
                .min(300);
            state.locked_until = Some(Instant::now() + std::time::Duration::from_secs(delay));
            tracing::warn!(
                failures = state.failures,
                delay_secs = delay,
                "涉密保险库因连续密码错误触发指数退避"
            );
        }

        Ok(())
    }

    /// 成功后重置计数器。
    pub fn record_success(&self, vault_path: &std::path::Path) {
        if let Ok(mut guard) = self.attempts.lock() {
            guard.remove(&Self::key(vault_path));
        }
    }
}
