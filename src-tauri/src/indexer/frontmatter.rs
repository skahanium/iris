use serde::Deserialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// v0.1 仅索引 YAML 中的 `tags`（数组或标量）；`title` 可选覆盖展示标题。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedNote {
    pub body: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    /// 完整 frontmatter 对象的 JSON，无 frontmatter 时为 `None`。
    pub frontmatter_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FrontmatterFields {
    title: Option<String>,
    tags: Option<TagsField>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TagsField {
    List(Vec<TagValue>),
    One(TagValue),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TagValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl TagValue {
    fn into_tag_string(self) -> String {
        match self {
            TagValue::Str(s) => s,
            TagValue::Int(n) => n.to_string(),
            TagValue::Float(f) => f.to_string(),
            TagValue::Bool(b) => b.to_string(),
        }
    }
}

impl TagsField {
    fn into_tags(self) -> Vec<String> {
        match self {
            TagsField::List(items) => items
                .into_iter()
                .map(TagValue::into_tag_string)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            TagsField::One(one) => {
                let s = one.into_tag_string();
                if s.is_empty() {
                    vec![]
                } else {
                    vec![s.trim().to_string()]
                }
            }
        }
    }
}

/// 拆分 `---` YAML frontmatter 与正文（Obsidian 风格）。
pub fn split_frontmatter(content: &str) -> (Option<String>, String) {
    let s = content.trim_start_matches('\u{feff}');
    if !s.starts_with("---") {
        return (None, content.to_string());
    }
    let after_marker = &s[3..];
    let after_marker = after_marker
        .strip_prefix('\n')
        .or_else(|| after_marker.strip_prefix("\r\n"))
        .unwrap_or(after_marker);

    let end = after_marker
        .find("\n---")
        .or_else(|| after_marker.find("\r\n---"));
    let Some(end) = end else {
        return (None, content.to_string());
    };

    let yaml_part = after_marker[..end].trim().to_string();
    let mut body = after_marker[end + 4..].to_string();
    if body.starts_with('\n') {
        body.remove(0);
    } else if body.starts_with("\r\n") {
        body.drain(..2);
    }
    if yaml_part.is_empty() {
        (None, body)
    } else {
        (Some(yaml_part), body)
    }
}

fn extract_fields(yaml: &str) -> (Option<String>, Vec<String>) {
    let Ok(fields) = serde_yaml::from_str::<FrontmatterFields>(yaml) else {
        return (None, vec![]);
    };
    let title = fields
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let tags = fields.tags.map(TagsField::into_tags).unwrap_or_default();
    (title, tags)
}

/// 解析笔记：提取 body、可选 title/tags、frontmatter JSON。
pub fn parse_note(content: &str) -> AppResult<ParsedNote> {
    let (yaml, body) = split_frontmatter(content);
    let Some(yaml) = yaml else {
        return Ok(ParsedNote {
            body,
            title: None,
            tags: vec![],
            frontmatter_json: None,
        });
    };

    let value: Value = serde_yaml::from_str(&yaml)
        .map_err(|e| AppError::msg(format!("Invalid frontmatter YAML: {e}")))?;
    let (title, tags) = extract_fields(&yaml);
    let frontmatter_json = serde_json::to_string(&value)?;

    Ok(ParsedNote {
        body,
        title,
        tags,
        frontmatter_json: Some(frontmatter_json),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_obsidian_style_frontmatter() {
        let md = "---\ntitle: Meeting\ntags: [work, iris]\n---\n\n# Body\n";
        let (yaml, body) = split_frontmatter(md);
        assert!(yaml.unwrap().contains("tags:"));
        assert!(body.contains("# Body"));
    }

    #[test]
    fn parses_tags_array_and_title() {
        let md = "---\ntitle: T\ntags: [a, b]\n---\ncontent";
        let note = parse_note(md).unwrap();
        assert_eq!(note.title.as_deref(), Some("T"));
        assert_eq!(note.tags, vec!["a", "b"]);
        assert!(note.frontmatter_json.is_some());
        assert_eq!(note.body, "content");
    }

    #[test]
    fn parses_tags_scalar() {
        let md = "---\ntags: solo\n---\n";
        let note = parse_note(md).unwrap();
        assert_eq!(note.tags, vec!["solo"]);
    }

    #[test]
    fn no_frontmatter_returns_none_json() {
        let note = parse_note("# Hi\n").unwrap();
        assert!(note.frontmatter_json.is_none());
        assert!(note.tags.is_empty());
    }
}
