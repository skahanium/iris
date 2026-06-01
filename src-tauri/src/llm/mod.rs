pub mod anthropic;
pub mod config;
pub mod engine;
pub mod fetch_web_page;
pub mod http_politeness;
pub mod minimax_search;
pub mod model_catalog;
pub mod providers;
pub mod search_web;
pub mod web_search_config;

use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// 安全获取 mutex 锁，处理中毒情况
pub fn safe_lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("Mutex poisoned, recovering inner data");
        poisoned.into_inner()
    })
}

/// 单次流式 LLM 请求的共享上下文（OpenAI 兼容 / Anthropic Messages）。
pub struct LlmStreamContext<'a> {
    pub app: &'a AppHandle,
    pub request_id: &'a str,
    pub abort_flag: Arc<Mutex<bool>>,
    pub api_key: &'a str,
    pub base: &'a str,
    pub model: &'a str,
    pub messages: Vec<ChatMessage>,
    pub system: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmGenerateParams {
    pub provider: String,
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub system: Option<String>,
    pub stream: Option<bool>,
    pub custom_base_url: Option<String>,
    pub web_search: Option<bool>,
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
