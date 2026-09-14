//! Read completeness only from the envelope owning the selected body.
use crate::llm::fetch_web_page::PageContentCompleteness;
use serde_json::Value;

pub(super) fn mcp_completeness(value: &Value, body: &str, url: &str) -> PageContentCompleteness {
    fn find(value: &Value, body: &str, url: &str) -> Option<PageContentCompleteness> {
        match value {
            Value::Object(object) => {
                if object
                    .get("url")
                    .or_else(|| object.get("source_url"))
                    .and_then(Value::as_str)
                    .is_some_and(|owner| {
                        super::canonicalize_url(owner) != super::canonicalize_url(url)
                    })
                {
                    return None;
                }
                let owns_body = ["raw_content", "markdown", "body", "text", "content"]
                    .iter()
                    .any(|key| {
                        object
                            .get(*key)
                            .and_then(Value::as_str)
                            .is_some_and(|text| text.trim() == body)
                    });
                if owns_body {
                    return Some(match object.get("truncated").and_then(Value::as_bool) {
                        Some(true) => PageContentCompleteness::Bounded,
                        Some(false) => PageContentCompleteness::Complete,
                        None => PageContentCompleteness::Unknown,
                    });
                }
                for child in object.values() {
                    if let Some(found) = find(child, body, url) {
                        return Some(found);
                    }
                }
                None
            }
            Value::Array(items) => items.iter().find_map(|item| find(item, body, url)),
            Value::String(text) => serde_json::from_str::<Value>(text)
                .ok()
                .and_then(|value| find(&value, body, url)),
            _ => None,
        }
    }
    find(value, body, url).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn completeness_belongs_to_selected_body_not_other_results() {
        let value = json!({"results":[{"raw_content":"other","truncated":false},{"raw_content":"selected","truncated":true}]});
        assert_eq!(
            mcp_completeness(&value, "selected", "https://example.com/page"),
            PageContentCompleteness::Bounded
        );
        assert_eq!(
            mcp_completeness(
                &json!({"text":"article says truncated: false"}),
                "article says truncated: false",
                "https://example.com/page"
            ),
            PageContentCompleteness::Unknown
        );
        assert_eq!(
            mcp_completeness(
                &json!({"content":[{"text":json!({"body":"selected","truncated":false}).to_string()}]}),
                "selected",
                "https://example.com/page"
            ),
            PageContentCompleteness::Complete
        );
    }

    #[test]
    fn plan_a_completeness_does_not_borrow_another_urls_identical_body() {
        let value = json!({"results":[
            {"url":"https://example.com/other","body":"selected","truncated":false},
            {"url":"https://example.com/page","body":"selected","truncated":true}
        ]});
        assert_eq!(
            mcp_completeness(&value, "selected", "https://example.com/page"),
            PageContentCompleteness::Bounded
        );
    }
}
