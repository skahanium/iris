//! Payload shaping for the Agent tool loop: fitting an over-budget tool result
//! into the model's character budget without breaking its JSON envelope.

use super::truncate_chars;

/// Prose fields a tool payload may give up to fit the character budget.
const SHRINKABLE_TEXT_FIELDS: &[&str] = &["content", "excerpt", "text", "body"];

/// Fit an over-budget tool payload into `budget` characters **without breaking
/// its JSON envelope**.
///
/// Slicing the serialized string produced invalid JSON and dropped
/// `nextStartByte`, so the model lost both the tail of the body and the pointer
/// for continuing the read. This shortens a known prose field instead and, for a
/// ranged read, recomputes the continuation pointer from the prefix that
/// actually survived — so what the model can see and where the next read
/// resumes agree.
///
/// The tool result envelope is `{success, output, error, loopObservation}`, so
/// the shrinkable text is either `output` itself or one of `SHRINKABLE_TEXT_FIELDS`
/// on the `output` object. A payload with no such field falls back to a plain
/// slice; that is a last resort, not the normal path.
pub(crate) fn fit_tool_payload(
    payload: &serde_json::Value,
    budget: usize,
    serialized: &str,
) -> (String, bool) {
    if serialized.chars().count() <= budget {
        return (serialized.to_string(), false);
    }
    let mut value = payload.clone();

    // Decide the shrink plan with immutable borrows only; the mutable pass below
    // must not overlap with the overhead measurement.
    let output = value.get("output");
    let field: Option<&'static str> = match output {
        Some(serde_json::Value::Object(object)) => SHRINKABLE_TEXT_FIELDS
            .iter()
            .find(|field| {
                object
                    .get(**field)
                    .is_some_and(serde_json::Value::is_string)
            })
            .copied(),
        _ => None,
    };
    let shrinkable = matches!(output, Some(serde_json::Value::String(_))) || field.is_some();
    if !shrinkable {
        return (truncate_chars(serialized, budget), true);
    }
    let overhead = match field {
        Some(field) => measure_without_output_field(&value, field),
        None => measure_without_output(&value),
    };
    let allowed = budget.saturating_sub(overhead + 32);
    if allowed < 256 {
        return (truncate_chars(serialized, budget), true);
    }

    let Some(serde_json::Value::Object(object)) = value.get_mut("output") else {
        // `output` is a plain string: shorten it in place.
        if let Some(serde_json::Value::String(text)) = value.get_mut("output") {
            if text.chars().count() <= allowed {
                return (truncate_chars(serialized, budget), true);
            }
            let kept: String = text.chars().take(allowed).collect();
            *text = kept;
            return match serde_json::to_string(&value) {
                Ok(fitted) if fitted.chars().count() <= budget => (fitted, true),
                _ => (truncate_chars(serialized, budget), true),
            };
        }
        return (truncate_chars(serialized, budget), true);
    };

    let Some(field) = field else {
        return (truncate_chars(serialized, budget), true);
    };
    let Some(serde_json::Value::String(text)) = object.get_mut(field) else {
        return (truncate_chars(serialized, budget), true);
    };
    if text.chars().count() <= allowed {
        return (truncate_chars(serialized, budget), true);
    }
    let kept: String = text.chars().take(allowed).collect();
    let kept_bytes = kept.len();
    *text = kept;
    object.insert("truncated".into(), serde_json::Value::Bool(true));
    if field == "content" {
        let start = object
            .get("sourceSpan")
            .and_then(|span| span.get("start"))
            .and_then(serde_json::Value::as_u64);
        if let Some(start) = start {
            object.insert(
                "nextStartByte".into(),
                serde_json::Value::Number((start + kept_bytes as u64).into()),
            );
        }
    }
    match serde_json::to_string(&value) {
        Ok(fitted) if fitted.chars().count() <= budget => (fitted, true),
        _ => (truncate_chars(serialized, budget), true),
    }
}

/// Serialized length once `output` is emptied.
fn measure_without_output(value: &serde_json::Value) -> usize {
    let mut probe = value.clone();
    if let Some(object) = probe.as_object_mut() {
        object.insert("output".into(), serde_json::Value::Null);
    }
    serde_json::to_string(&probe).map_or(0, |text| text.chars().count())
}

/// Serialized length once `output.<field>` is emptied.
fn measure_without_output_field(value: &serde_json::Value, field: &str) -> usize {
    let mut probe = value.clone();
    if let Some(object) = probe
        .get_mut("output")
        .and_then(serde_json::Value::as_object_mut)
    {
        object.insert(field.into(), serde_json::Value::String(String::new()));
    }
    serde_json::to_string(&probe).map_or(0, |text| text.chars().count())
}
