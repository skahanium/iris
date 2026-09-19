//! DeepSeek-Flash native search: Anthropic Messages hosted `web_search`.
//!
//! Live probe (2026-09-20) on `deepseek-flash`:
//! - `POST https://api.deepseek.com/responses` and `/v1/responses` with
//!   `{type:web_search}` / `{type:web_search_2025_08_26}` were HTTP 200
//!   tools-echo (`type=web_search`) with **no** `web_search_call`. Official
//!   Responses guide currently ignores built-in `web_search`.
//! - Chat Completions `{type:web_search}` returned HTTP 422.
//! - `POST https://api.deepseek.com/anthropic/v1/messages` with
//!   `web_search_20250305` returned `server_tool_use` +
//!   `web_search_tool_result` + `web_search_result`. Bare `{type:web_search}`
//!   on that host was HTTP 422.
//!
//! Main DeepSeek chat stays Chat Completions. Do not reuse MiniMax Responses
//! fields (`input`, `store`, `tool_choice: auto`, `/v1/responses`).

use serde_json::{json, Value};

use crate::ai_runtime::dual_path_search::RouteFailureClass;
use crate::ai_runtime::native_search_adapter::NativeSearchModelAdapter;
use crate::ai_runtime::native_search_subrequest::{
    parse_native_search_payload, NativeSearchParse, NativeSearchPayloadFamily,
    NativeSearchSubrequest,
};
use crate::llm::model_catalog::is_deepseek_flash_model;
use crate::llm::providers::ANTHROPIC_API_VERSION;

const CATALOG_ID: &str = "deepseek-v4-flash";
const OUTBOUND_MODEL: &str = "deepseek-flash";
const MAX_TOKENS: u32 = 2048;

pub(super) struct DeepSeekFlashNativeSearchAdapter;

impl NativeSearchModelAdapter for DeepSeekFlashNativeSearchAdapter {
    fn id(&self) -> &'static str {
        CATALOG_ID
    }

    fn matches(&self, model_id: &str) -> bool {
        is_deepseek_flash_model(model_id)
    }

    fn request_url(&self, api_base: &str) -> Result<String, RouteFailureClass> {
        deepseek_anthropic_messages_url(api_base)
    }

    fn outbound_body(&self, subrequest: &NativeSearchSubrequest) -> Value {
        json!({
            "model": OUTBOUND_MODEL,
            "max_tokens": MAX_TOKENS,
            "stream": false,
            "thinking": { "type": "disabled" },
            "system": "This is an isolated web search subrequest. You must use the web_search tool and return source URLs. Do not answer from memory.",
            "messages": [{
                "role": "user",
                "content": subrequest.query,
            }],
            "tools": [{
                "type": "web_search_20250305",
                "name": "web_search",
            }],
            "tool_choice": {
                "type": "tool",
                "name": "web_search",
            },
        })
    }

    fn parse_response(&self, body: &Value) -> NativeSearchParse {
        if body.get("error").is_some_and(|error| !error.is_null()) {
            return deepseek_protocol_insufficient();
        }
        parse_native_search_payload(NativeSearchPayloadFamily::AnthropicShaped, body)
    }

    fn http_headers(&self, token: &str) -> Vec<(String, String)> {
        vec![
            ("x-api-key".into(), token.to_string()),
            (
                "anthropic-version".into(),
                ANTHROPIC_API_VERSION.to_string(),
            ),
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
        ]
    }

    fn retry_text_only_once(&self) -> bool {
        true
    }
}

fn deepseek_protocol_insufficient() -> NativeSearchParse {
    NativeSearchParse {
        candidates: Vec::new(),
        has_retrieval_credentials: false,
        generated_text_only: false,
        failure: Some(RouteFailureClass::ProtocolOrResultInsufficient),
    }
}

fn deepseek_anthropic_messages_url(api_base: &str) -> Result<String, RouteFailureClass> {
    let mut base = api_base.trim().trim_end_matches('/').to_string();
    if !base.starts_with("https://") {
        return Err(RouteFailureClass::TransportOrProviderFailure);
    }
    for suffix in [
        "/chat/completions",
        "/responses",
        "/v1/messages",
        "/messages",
    ] {
        if let Some(stripped) = base.strip_suffix(suffix) {
            base = stripped.trim_end_matches('/').to_string();
        }
    }
    if let Some(stripped) = base.strip_suffix("/v1") {
        base = stripped.trim_end_matches('/').to_string();
    }
    if base.ends_with("/anthropic") {
        Ok(format!("{base}/v1/messages"))
    } else {
        Ok(format!("{base}/anthropic/v1/messages"))
    }
}
