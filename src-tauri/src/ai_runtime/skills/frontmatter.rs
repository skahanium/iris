use std::collections::HashMap;

use super::model_impl::SkillScopeRule;

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum ScopeFrontmatter {
    One(SkillScopeRule),
    Many(Vec<SkillScopeRule>),
}

#[derive(serde::Deserialize)]
struct SkillFrontmatterScope {
    #[serde(default)]
    scope: Option<ScopeFrontmatter>,
}

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

fn frontmatter_block(raw: &str) -> Option<&str> {
    let trimmed = raw.trim_start();
    let rest = trimmed.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    Some(&rest[..end])
}

pub(super) fn parse_scope_rules(raw: &str) -> Vec<SkillScopeRule> {
    let Some(frontmatter) = frontmatter_block(raw) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_yaml::from_str::<SkillFrontmatterScope>(frontmatter) else {
        return Vec::new();
    };
    let mut rules = match parsed.scope {
        Some(ScopeFrontmatter::One(rule)) => vec![rule],
        Some(ScopeFrontmatter::Many(rules)) => rules,
        None => Vec::new(),
    };
    for rule in &mut rules {
        rule.kind = rule.kind.trim().to_ascii_lowercase();
        rule.pattern = rule.pattern.trim().to_string();
    }
    rules.retain(|rule| {
        matches!(rule.kind.as_str(), "path" | "glob" | "tag") && !rule.pattern.is_empty()
    });
    rules
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
