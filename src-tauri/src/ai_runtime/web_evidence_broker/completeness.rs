//! Decode the selected body's own declaration; never search article metadata.
use crate::llm::fetch_web_page::PageContentCompleteness;
use serde_json::Value;

pub(super) fn declared_completeness(truncated: Option<&Value>) -> PageContentCompleteness {
    match truncated.and_then(Value::as_bool) {
        Some(true) => PageContentCompleteness::Bounded,
        Some(false) => PageContentCompleteness::Complete,
        None => PageContentCompleteness::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decoded_completeness(value: &Value) -> PageContentCompleteness {
        super::super::mcp_page_fetch_result("fixture", "https://example.com/page", value)
            .unwrap()
            .completeness
    }

    #[test]
    fn completeness_belongs_to_selected_body_not_other_results() {
        let value = json!({"results":[
            {"url":"https://example.com/other","raw_content":"other","truncated":false},
            {"url":"https://example.com/page","raw_content":"selected","truncated":true}
        ]});
        assert_eq!(
            decoded_completeness(&value),
            PageContentCompleteness::Bounded
        );
        assert_eq!(
            decoded_completeness(&json!({"text":"article says truncated: false"}),),
            PageContentCompleteness::Unknown
        );
        assert_eq!(
            decoded_completeness(
                &json!({"content":[{"text":json!({"body":"selected","truncated":false}).to_string()}]}),
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
            decoded_completeness(&value),
            PageContentCompleteness::Bounded
        );
    }

    #[test]
    fn plan_a_review_completeness_comes_from_the_decoded_body_not_matching_metadata() {
        let url = "https://example.com/page";
        let result = json!({
            "metadata":{"body":"selected article","truncated":false},
            "results":[{"url":url,"body":"selected article"}]
        });
        let decoded = super::super::mcp_page_fetch_result("fixture", url, &result).unwrap();
        assert_eq!(decoded.text, "selected article");
        assert_eq!(decoded.completeness, PageContentCompleteness::Unknown);
    }
}
