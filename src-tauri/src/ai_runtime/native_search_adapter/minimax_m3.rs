//! MiniMax-M3 native search: OpenAI Responses `web_search` on the chat API host.
//!
//! Live probe (2026-09-20): `POST https://api.minimaxi.com/v1/responses` returned
//! `web_search_call` + HTTPS `url_citation`. Anthropic Messages
//! `web_search_20250305` on the same host was HTTP 200 text-only and is not
//! used here. Main MiniMax chat stays Chat Completions.

use serde_json::{json, Value};

use crate::ai_runtime::dual_path_search::RouteFailureClass;
use crate::ai_runtime::native_search_adapter::NativeSearchModelAdapter;
use crate::ai_runtime::native_search_subrequest::{
    parse_native_search_payload, NativeSearchParse, NativeSearchPayloadFamily,
    NativeSearchSubrequest,
};
use crate::llm::model_catalog::is_minimax_m3_model;

const MAX_OUTPUT_TOKENS: u32 = 2048;

pub(super) struct MinimaxM3NativeSearchAdapter;

impl NativeSearchModelAdapter for MinimaxM3NativeSearchAdapter {
    fn id(&self) -> &'static str {
        "MiniMax-M3"
    }

    fn matches(&self, model_id: &str) -> bool {
        is_minimax_m3_model(model_id)
    }

    fn request_url(&self, api_base: &str) -> Result<String, RouteFailureClass> {
        minimax_responses_url(api_base)
    }

    fn outbound_body(&self, subrequest: &NativeSearchSubrequest) -> Value {
        json!({
            "model": self.id(),
            "input": subrequest.query,
            "instructions": "This is an isolated web search subrequest. You must use the web_search tool and return URL citations. Do not answer from memory.",
            "stream": false,
            "store": false,
            "temperature": 0.1,
            "tool_choice": "auto",
            "max_output_tokens": MAX_OUTPUT_TOKENS,
            "tools": [{ "type": "web_search" }],
        })
    }

    fn parse_response(&self, body: &Value) -> NativeSearchParse {
        if body.get("error").is_some_and(|error| !error.is_null()) {
            return minimax_protocol_insufficient();
        }
        let parse = parse_native_search_payload(NativeSearchPayloadFamily::OpenAiShaped, body);
        if parse.has_retrieval_credentials {
            return parse;
        }
        if let Some(status) = body.get("status").and_then(Value::as_str) {
            if status != "completed" {
                return minimax_protocol_insufficient();
            }
        }
        parse
    }

    fn retry_text_only_once(&self) -> bool {
        true
    }
}

fn minimax_protocol_insufficient() -> NativeSearchParse {
    NativeSearchParse {
        candidates: Vec::new(),
        has_retrieval_credentials: false,
        generated_text_only: false,
        failure: Some(RouteFailureClass::ProtocolOrResultInsufficient),
    }
}

fn minimax_responses_url(api_base: &str) -> Result<String, RouteFailureClass> {
    let mut base = api_base.trim().trim_end_matches('/').to_string();
    if !base.starts_with("https://") {
        return Err(RouteFailureClass::TransportOrProviderFailure);
    }
    for suffix in ["/chat/completions", "/responses", "/messages"] {
        if let Some(stripped) = base.strip_suffix(suffix) {
            base = stripped.trim_end_matches('/').to_string();
        }
    }
    if base.ends_with("/v1") {
        Ok(format!("{base}/responses"))
    } else {
        Ok(format!("{base}/v1/responses"))
    }
}
