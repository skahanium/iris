use std::collections::HashMap;

fn yaml_value_to_string(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::Bool(v) => Some(v.to_string()),
        serde_yaml::Value::Number(v) => Some(v.to_string()),
        serde_yaml::Value::String(v) => Some(v.clone()),
        serde_yaml::Value::Sequence(values) => {
            let items: Vec<String> = values.iter().filter_map(yaml_value_to_string).collect();
            Some(items.join(" "))
        }
        serde_yaml::Value::Mapping(_) => serde_json::to_string(value).ok(),
        _ => None,
    }
}

/// Parse YAML-like frontmatter from SKILL.md.
///
/// Returns (key-value map, body content after the closing `---`).
/// Handles both simple `key: value` lines and multi-line values.
pub(super) fn parse_frontmatter(raw: &str) -> (HashMap<String, String>, String) {
    let mut map = HashMap::new();
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (map, raw.to_string());
    }
    let rest = trimmed.trim_start_matches("---");
    let Some(end) = rest.find("\n---") else {
        return (map, raw.to_string());
    };
    let front = &rest[..end];
    let body = rest[end + 4..].trim_start();
    if let Ok(serde_yaml::Value::Mapping(mapping)) =
        serde_yaml::from_str::<serde_yaml::Value>(front)
    {
        for (key, value) in mapping {
            let Some(key) = key.as_str() else {
                continue;
            };
            if let Some(value) = yaml_value_to_string(&value) {
                map.insert(key.to_string(), value);
            }
        }
        return (map, body.to_string());
    }
    for line in front.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let value = v.trim().trim_matches('"').to_string();
            map.insert(k.trim().to_string(), value);
        }
    }
    (map, body.to_string())
}
