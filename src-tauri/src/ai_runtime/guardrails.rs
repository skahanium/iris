//! Guardrails retained by the production tool-execution pipeline.

use serde::{Deserialize, Serialize};

/// 检测结果，用于 guard 检查的返回值。
///
/// 按严重程度递增：`Pass` → `Warn` → `Block`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardResult {
    /// 通过检查
    Pass,
    /// 警告 — 可疑但不阻断，记录日志供人工审查
    Warn { reason: String },
    /// 阻断 — 检测到明确风险，拒绝执行
    Block { reason: String },
}

/// 验证工具调用参数是否符合目录声明的 JSON Schema 子集。
///
/// 执行层不信任模型生成的 arguments；除了必填字段，还校验对象、数组、
/// 基础类型、枚举和数组项。目录当前只使用这个受控子集，未知 schema 类型
/// 一律 fail-closed，避免模型参数越过 Rust handler 的预期形状。
///
/// # Returns
///
/// - `GuardResult::Pass` — 参数合法
/// - `GuardResult::Block` — 缺少必需字段
pub fn verify_tool_args(
    tool_name: &str,
    args: &serde_json::Value,
    expected_schema: &serde_json::Value,
) -> GuardResult {
    match validate_tool_schema(args, expected_schema, "arguments") {
        Ok(()) => GuardResult::Pass,
        Err(reason) => GuardResult::Block {
            reason: format!("invalid arguments for tool '{tool_name}': {reason}"),
        },
    }
}

fn validate_tool_schema(
    value: &serde_json::Value,
    schema: &serde_json::Value,
    location: &str,
) -> Result<(), String> {
    let schema_object = schema
        .as_object()
        .ok_or_else(|| format!("{location} schema is not an object"))?;
    if let Some(allowed) = schema_object
        .get("enum")
        .and_then(serde_json::Value::as_array)
    {
        if !allowed.contains(value) {
            return Err(format!("{location} is outside the declared enum"));
        }
    }
    let Some(expected_type) = schema_object
        .get("type")
        .and_then(serde_json::Value::as_str)
    else {
        return Err(format!("{location} schema is missing type"));
    };

    match expected_type {
        "object" => {
            let object = value
                .as_object()
                .ok_or_else(|| format!("{location} must be an object"))?;
            if let Some(required) = schema_object
                .get("required")
                .and_then(serde_json::Value::as_array)
            {
                for field in required {
                    let field_name = field.as_str().ok_or_else(|| {
                        format!("{location} schema contains a non-string required field")
                    })?;
                    if !object.contains_key(field_name) {
                        return Err(format!("{location}.{field_name} is required"));
                    }
                }
            }
            if let Some(properties) = schema_object
                .get("properties")
                .and_then(serde_json::Value::as_object)
            {
                for (field_name, field_schema) in properties {
                    if let Some(field_value) = object.get(field_name) {
                        validate_tool_schema(
                            field_value,
                            field_schema,
                            &format!("{location}.{field_name}"),
                        )?;
                    }
                }
            }
        }
        "array" => {
            let array = value
                .as_array()
                .ok_or_else(|| format!("{location} must be an array"))?;
            if let Some(item_schema) = schema_object.get("items") {
                for (index, item) in array.iter().enumerate() {
                    validate_tool_schema(item, item_schema, &format!("{location}[{index}]"))?;
                }
            }
        }
        "string" if value.is_string() => {}
        "integer" if value.as_i64().is_some() || value.as_u64().is_some() => {}
        "number" if value.is_number() => {}
        "boolean" if value.is_boolean() => {}
        "null" if value.is_null() => {}
        "string" | "integer" | "number" | "boolean" | "null" => {
            return Err(format!("{location} must be a {expected_type}"));
        }
        unknown => return Err(format!("{location} schema type '{unknown}' is unsupported")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_argument_validation_enforces_object_required_types_and_nested_items() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer"},
                "urls": {"type": "array", "items": {"type": "string"}}
            },
            "required": ["query"]
        });

        assert!(matches!(
            verify_tool_args(
                "web_search",
                &serde_json::json!({"query": "Iris", "limit": 3, "urls": ["https://example.com"]}),
                &schema
            ),
            GuardResult::Pass
        ));
        assert!(matches!(
            verify_tool_args("web_search", &serde_json::json!({"limit": 3}), &schema),
            GuardResult::Block { .. }
        ));
        assert!(matches!(
            verify_tool_args("web_search", &serde_json::json!({"query": 7}), &schema),
            GuardResult::Block { .. }
        ));
        assert!(matches!(
            verify_tool_args(
                "web_search",
                &serde_json::json!({"query": "Iris", "urls": [7]}),
                &schema
            ),
            GuardResult::Block { .. }
        ));
    }
}
