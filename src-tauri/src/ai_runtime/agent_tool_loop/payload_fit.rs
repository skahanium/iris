//! Fit model observations as JSON values, preserving execution status and ranges.

use crate::ai_runtime::run_contract::SafeRunErrorCode;
use crate::error::{AppError, AppResult};
use serde_json::{json, Value};

/// Fit an observation without slicing its serialized representation. Unknown
/// shapes whose metadata cannot fit fail at the projection boundary; they never
/// become a fabricated tool failure or malformed model message.
pub(crate) fn fit_tool_payload(
    payload: &Value,
    budget: usize,
    serialized: &str,
) -> AppResult<(String, bool)> {
    if serialized.chars().count() <= budget {
        return Ok((serialized.to_owned(), false));
    }
    let mut value = payload.clone();
    // Lists contain independently useful records. Drop whole suffix records,
    // never cut evidence text while retaining a pointer to a different excerpt.
    for key in ["results", "packets"] {
        if value["output"][key].is_array() {
            let mut omitted = 0;
            loop {
                let items = value["output"][key].as_array_mut().expect("array checked");
                if items.pop().is_none() {
                    return Err(projection_limit());
                }
                omitted += 1;
                let count = items.len();
                value["output"]["truncated"] = json!(true);
                value["output"]["omittedResults"] = json!(omitted);
                if value["output"].get("count").is_some() {
                    value["output"]["count"] = json!(count);
                }
                let text = serde_json::to_string(&value)?;
                if text.chars().count() <= budget {
                    return Ok((text, true));
                }
            }
        }
    }
    if value["output"].is_string() {
        value["output"] = json!({"content":value["output"],"truncated":true});
    }
    // Preserve the existing closed set of prose slots. Only the read_note
    // content contract carries byte ranges; other prose has no inferred paging.
    let field = ["content", "excerpt", "text", "body"]
        .into_iter()
        .find(|field| value["output"][*field].is_string())
        .ok_or_else(projection_limit)?;
    let pointer = format!("/output/{field}");
    let ranged = field == "content" && value["output"]["sourceSpan"]["start"].as_u64().is_some();
    let original = value
        .pointer(&pointer)
        .and_then(Value::as_str)
        .expect("text checked");
    let chars: Vec<char> = original.chars().collect();
    let start = value["output"]["sourceSpan"]["start"].as_u64().unwrap_or(0);
    let mut low = 0;
    let mut high = chars.len();
    let mut fitted = None;
    while low <= high {
        let keep = low + (high - low) / 2;
        let prefix: String = chars[..keep].iter().collect();
        let end = start
            .checked_add(prefix.len() as u64)
            .ok_or_else(projection_limit)?;
        *value.pointer_mut(&pointer).expect("text checked") = Value::String(prefix);
        if value["output"].is_object() {
            value["output"]["truncated"] = json!(true);
            if ranged {
                value["output"]["sourceSpan"]["end"] = json!(end);
                value["output"]["nextStartByte"] = json!(end);
            }
        } else {
            value["outputTruncated"] = json!(true);
        }
        let text = serde_json::to_string(&value)?;
        if text.chars().count() <= budget {
            // An empty ranged read would hand back the same continuation.
            if keep > 0 || !ranged {
                fitted = Some(text);
            }
            low = keep + 1;
        } else if keep == 0 {
            break;
        } else {
            high = keep - 1;
        }
    }
    fitted.map(|text| (text, true)).ok_or_else(projection_limit)
}

fn projection_limit() -> AppError {
    AppError::run(SafeRunErrorCode::ToolLoopLimit)
}

use super::loop_projection::LoopProjection;
use super::{
    LlmMessage, MessageRole, ToolCall, ToolCallResult, MAX_TOOL_RESULT_CHARS,
    MAX_WEB_TOOL_RESULT_CHARS,
};

pub(crate) fn tool_result_message(
    call: &ToolCall,
    result: &ToolCallResult,
    projection: LoopProjection,
) -> AppResult<(LlmMessage, bool)> {
    let payload = tool_result_payload(result, projection);
    let serialized = serde_json::to_string(&payload)?;
    let budget = tool_result_char_budget(&call.function.name);
    // Web excerpts have already been sized and registered by the executor.
    // Changing them here would detach the visible content from its evidence.
    if serialized.chars().count() > budget
        && matches!(call.function.name.as_str(), "web_search" | "web_fetch")
    {
        return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
    }
    let (content, truncated) = fit_tool_payload(&payload, budget, &serialized)?;
    Ok((
        LlmMessage {
            role: MessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(call.id.clone()),
            tool_calls: None,
            reasoning_content: None,
        },
        truncated,
    ))
}

fn tool_result_payload(result: &ToolCallResult, projection: LoopProjection) -> serde_json::Value {
    serde_json::json!({
        "success": result.success, "output": result.output, "error": result.error,
        "loopObservation": projection.to_json()
    })
}

/// Size the output before the executor registers evidence. Reserve the widest
/// legal projection so the model-facing message cannot shorten it a second time.
pub(crate) fn prepare_tool_result(result: &mut ToolCallResult) -> AppResult<()> {
    let payload = tool_result_payload(result, LoopProjection::widest());
    let serialized = serde_json::to_string(&payload)?;
    let (fitted, _) = fit_tool_payload(
        &payload,
        tool_result_char_budget(&result.tool_name),
        &serialized,
    )?;
    result.output = serde_json::from_str::<serde_json::Value>(&fitted)?["output"].take();
    Ok(())
}

fn tool_result_char_budget(tool_name: &str) -> usize {
    if matches!(tool_name, "web_search" | "web_fetch") {
        MAX_WEB_TOOL_RESULT_CHARS
    } else {
        MAX_TOOL_RESULT_CHARS
    }
}
