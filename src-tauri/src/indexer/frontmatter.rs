use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::LazyLock;

use crate::error::{AppError, AppResult};

static BODY_TAG_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"#[\w\u{4e00}-\u{9fff}\-]+").expect("body tag regex"));

/// v0.1 仅索引 YAML 中的 `tags`（数组或标量）；`title` 可选覆盖展示标题。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedNote {
    pub body: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    /// 从 YAML frontmatter 的 `aliases` 提取的规范化别名。
    pub aliases: Vec<String>,
    /// 完整 frontmatter 对象的 JSON，无 frontmatter 时为 `None`。
    pub frontmatter_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FrontmatterFields {
    title: Option<String>,
    tags: Option<TagsField>,
    #[serde(default)]
    aliases: AliasesField,
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

/// `aliases` only accepts a string or a sequence of strings. Invalid values are ignored so
/// a malformed alias never prevents title or tag indexing.
#[derive(Debug, Default)]
struct AliasesField(Vec<String>);

impl<'de> Deserialize<'de> for AliasesField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let strings = match value {
            Value::String(value) => vec![value],
            Value::Array(values) => values
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        };

        let mut seen = HashSet::new();
        Ok(Self(
            strings
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .filter(|value| seen.insert(value.clone()))
                .collect(),
        ))
    }
}

impl AliasesField {
    fn into_aliases(self) -> Vec<String> {
        self.0
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

/// Legacy machine-only stems (`untitled-123`); never show to users.
fn is_internal_untitled_stem(stem: &str) -> bool {
    let Some(digits) = stem.strip_prefix("untitled-") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Resolve user-visible document title (frontmatter `title:` only; not body `#` headings).
pub fn resolve_display_title(
    _parsed_title: Option<&str>,
    _stored_title: &str,
    _frontmatter_json: Option<&str>,
    path_stem: &str,
) -> String {
    if is_internal_untitled_stem(path_stem) {
        return "未命名文档".to_string();
    }
    path_stem.to_string()
}

fn extract_fields(yaml: &str) -> (Option<String>, Vec<String>, Vec<String>) {
    let Ok(fields) = serde_yaml::from_str::<FrontmatterFields>(yaml) else {
        return (None, vec![], vec![]);
    };
    let title = fields
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let tags = fields.tags.map(TagsField::into_tags).unwrap_or_default();
    (title, tags, fields.aliases.into_aliases())
}

/// Extract `#tag` from body text (e.g. `#rust`, `#机器学习`, `#hello-world`).
/// Returns deduplicated tags.
pub fn extract_body_tags(body: &str) -> Vec<String> {
    let mut tags: Vec<String> = BODY_TAG_RE
        .find_iter(body)
        .map(|m| m.as_str().trim_start_matches('#').to_string())
        .filter(|s| !s.is_empty() && s.len() <= 64)
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

/// 解析笔记：提取 body、可选 title/tags（YAML + body）、frontmatter JSON。
pub fn parse_note(content: &str) -> AppResult<ParsedNote> {
    let (yaml, body) = split_frontmatter(content);
    let Some(yaml) = yaml else {
        let body_tags = extract_body_tags(&body);
        return Ok(ParsedNote {
            body,
            title: None,
            tags: body_tags,
            aliases: vec![],
            frontmatter_json: None,
        });
    };

    let value: Value = serde_yaml::from_str(&yaml)
        .map_err(|e| AppError::msg(format!("Invalid frontmatter YAML: {e}")))?;
    let (title, mut tags, aliases) = extract_fields(&yaml);
    let body_tags = extract_body_tags(&body);
    for t in body_tags {
        if !tags.contains(&t) {
            tags.push(t);
        }
    }
    let frontmatter_json = serde_json::to_string(&value)?;

    Ok(ParsedNote {
        body,
        title,
        tags,
        aliases,
        frontmatter_json: Some(frontmatter_json),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_frontmatter_title_does_not_override_filename() {
        let title = resolve_display_title(
            Some("Legacy title"),
            "Stored title",
            Some(r#"{"title":"Other title"}"#),
            "file-name",
        );
        assert_eq!(title, "file-name");
    }

    #[test]
    fn resolve_display_title_prefers_frontmatter_json() {
        let fm = r#"{"title":"吃早饭","tags":[]}"#;
        let title = resolve_display_title(None, "untitled-1", Some(fm), "untitled-1");
        assert_eq!(title, "吃早饭");
    }

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

    #[test]
    fn bom_prefixed_content_strips_bom() {
        let md = "\u{feff}---\ntitle: T\ntags: [a]\n---\nbody";
        let (yaml, _body) = split_frontmatter(md);
        assert!(yaml.is_some());
    }

    #[test]
    fn empty_frontmatter_returns_none() {
        let md = "---\n---\nbody";
        let (yaml, body) = split_frontmatter(md);
        assert!(yaml.is_none());
        assert!(body.contains("body"));
    }

    #[test]
    fn missing_closing_delimiter_treats_all_as_body() {
        let md = "---\ntitle: T\nno closing";
        let (yaml, body) = split_frontmatter(md);
        assert!(yaml.is_none());
        assert!(body.contains("---"));
    }

    #[test]
    fn invalid_yaml_returns_err() {
        let md = "---\n\tbad: :::\n---\nbody";
        let result = parse_note(md);
        assert!(result.is_err());
    }

    #[test]
    fn extracts_body_tags() {
        let tags = extract_body_tags("Some text #rust and #tauri are cool.");
        assert_eq!(tags, vec!["rust", "tauri"]);
    }

    #[test]
    fn body_tags_dedup() {
        let tags = extract_body_tags("#rust #rust #rust");
        assert_eq!(tags, vec!["rust"]);
    }

    #[test]
    fn merges_yaml_and_body_tags() {
        let md = "---\ntags: [yaml-tag]\n---\nbody with #body-tag";
        let note = parse_note(md).unwrap();
        assert!(note.tags.contains(&"yaml-tag".into()));
        assert!(note.tags.contains(&"body-tag".into()));
    }

    #[test]
    fn body_only_tags_work() {
        let note = parse_note("No frontmatter #solo").unwrap();
        assert_eq!(note.tags, vec!["solo"]);
        assert!(note.frontmatter_json.is_none());
    }
}
