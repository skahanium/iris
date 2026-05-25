pub mod anthropic;
pub mod engine;
pub mod providers;
pub mod search_web;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

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
