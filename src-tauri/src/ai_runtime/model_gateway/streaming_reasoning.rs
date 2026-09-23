//! Minimax reasoning-detail helpers for streaming responses.
//!
//! Split out of `streaming.rs` to keep that file inside its pinned size
//! budget. These helpers normalise provider reasoning payloads and decide what
//! may be withheld from the visible stream; no streaming state lives here.

//! This module has no imports of its own as yet: the helpers below are pure
//! functions over caller-supplied JSON and string slices.

pub(crate) fn withhold_partial_provider_control_suffix(visible: &str, provider_id: &str) -> String {
    if !provider_id.eq_ignore_ascii_case("minimax") {
        return visible.to_string();
    }
    const CONTROL: &str = "<|minimax|>";
    let mut keep_len = visible.len();
    for prefix_len in 1..CONTROL.len() {
        if visible.ends_with(&CONTROL[..prefix_len]) {
            keep_len = keep_len.min(visible.len().saturating_sub(prefix_len));
        }
    }
    visible[..keep_len].to_string()
}

pub(crate) fn append_minimax_reasoning_details(
    value: &serde_json::Value,
    details: &mut Vec<serde_json::Value>,
) {
    let incoming = match value {
        serde_json::Value::Array(items) => items.iter().collect::<Vec<_>>(),
        serde_json::Value::Object(_) => vec![value],
        _ => return,
    };

    for (position, item) in incoming.into_iter().enumerate() {
        let Some(item_object) = item.as_object() else {
            continue;
        };
        let matching_index = details
            .iter()
            .position(|existing| minimax_reasoning_detail_stably_matches(existing, item))
            .or_else(|| {
                details
                    .get(position)
                    .filter(|existing| minimax_reasoning_detail_position_matches(existing, item))
                    .map(|_| position)
            });
        let Some(matching_index) = matching_index else {
            details.push(item.clone());
            continue;
        };
        let Some(existing_object) = details[matching_index].as_object_mut() else {
            details[matching_index] = item.clone();
            continue;
        };

        let merged_text = match (
            existing_object
                .get("text")
                .and_then(serde_json::Value::as_str),
            item_object.get("text").and_then(serde_json::Value::as_str),
        ) {
            (Some(existing), Some(incoming)) if incoming.starts_with(existing) => {
                Some(incoming.to_string())
            }
            (Some(existing), Some(incoming)) if existing.starts_with(incoming) => {
                Some(existing.to_string())
            }
            (Some(existing), Some(incoming)) => Some(format!("{existing}{incoming}")),
            (None, Some(incoming)) => Some(incoming.to_string()),
            _ => None,
        };
        for (key, value) in item_object {
            if key != "text" {
                existing_object.insert(key.clone(), value.clone());
            }
        }
        if let Some(text) = merged_text {
            existing_object.insert("text".to_string(), serde_json::Value::String(text));
        }
    }
}

pub(crate) fn minimax_reasoning_detail_stably_matches(
    existing: &serde_json::Value,
    incoming: &serde_json::Value,
) -> bool {
    let (Some(existing), Some(incoming)) = (existing.as_object(), incoming.as_object()) else {
        return false;
    };
    if let Some(id) = incoming.get("id") {
        return existing.get("id") == Some(id);
    }
    if let Some(index) = incoming.get("index") {
        return existing.get("index") == Some(index)
            && existing.get("type") == incoming.get("type");
    }
    false
}

pub(crate) fn minimax_reasoning_detail_position_matches(
    existing: &serde_json::Value,
    incoming: &serde_json::Value,
) -> bool {
    let (Some(existing), Some(incoming)) = (existing.as_object(), incoming.as_object()) else {
        return false;
    };
    existing.get("id").is_none()
        && existing.get("index").is_none()
        && incoming.get("id").is_none()
        && incoming.get("index").is_none()
        && existing.get("type").is_some()
        && existing.get("type") == incoming.get("type")
}

pub(crate) fn minimax_reasoning_continuation(
    details: Vec<serde_json::Value>,
    fallback_reasoning: String,
) -> Option<String> {
    if !details.is_empty() {
        return serde_json::to_string(&details).ok();
    }
    // `reasoning_split` should yield structured details. Some compatible
    // gateways still stream only a dedicated text delta; preserve it in the
    // documented details envelope rather than dropping a tool continuation.
    (!fallback_reasoning.is_empty()).then(|| {
        serde_json::json!([{
            "type": "reasoning.text",
            "text": fallback_reasoning,
        }])
        .to_string()
    })
}

pub(crate) fn withhold_partial_reasoning_open_suffix(visible: &str) -> String {
    const OPEN_TAGS: [&str; 3] = ["<thinking>", "<think>", "<reasoning>"];
    let lower = visible.to_ascii_lowercase();
    let mut keep_len = visible.len();
    for tag in OPEN_TAGS {
        for prefix_len in 1..tag.len() {
            if lower.ends_with(&tag[..prefix_len]) {
                keep_len = keep_len.min(visible.len().saturating_sub(prefix_len));
            }
        }
    }
    visible[..keep_len].to_string()
}
