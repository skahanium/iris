//! Tests extracted from `web_evidence_broker` to satisfy `size:check`.

use super::web_evidence_broker::*;
use crate::ai_runtime::dual_path_search::SearchActionIdentity;
use crate::ai_runtime::{WebSearchBackend, WebSourceRank};
use crate::error::AppError;
use crate::llm::fetch_web_page::PageFetchResult;
use crate::storage::db::Database;
use std::time::Duration;

const MERGED_FETCH_MAX_CHARS: usize = 12_000;

fn flatten_planned_query_fetch_results(
    batches: Vec<Vec<Result<SearchProviderFetch, String>>>,
) -> Vec<Result<SearchProviderFetch, String>> {
    batches.into_iter().flatten().collect()
}

fn web_evidence_usage_from_search_fetches<'a>(
    fetches: impl IntoIterator<Item = &'a SearchProviderFetch>,
) -> WebEvidenceUsage {
    let mut usage = WebEvidenceUsage::default();
    for fetch in fetches {
        let items = web_evidence_items_from_search_fetch(fetch);
        record_successful_search_usage(&mut usage, fetch, &items);
    }
    usage
}

fn merge_page_provider_fetches(url: &str, fetches: Vec<PageProviderFetch>) -> PageProviderFetch {
    let mut titles = Vec::new();
    let mut texts = Vec::new();
    let mut provider_ids = Vec::new();
    let mut provider_kinds = Vec::new();
    let mut methods = Vec::new();

    for fetch in fetches {
        push_unique_string(&mut titles, fetch.title);
        push_unique_string(&mut texts, fetch.text);
        push_unique_string(&mut provider_ids, fetch.provider_id);
        push_unique_string(&mut provider_kinds, fetch.provider_kind);
        push_unique_string(&mut methods, fetch.extraction_method);
    }

    let merged_text = texts.join("\n\n---\n\n");
    PageProviderFetch {
        completeness: Default::default(),
        title: titles
            .into_iter()
            .find(|title| !title.trim().is_empty())
            .unwrap_or_else(|| url.to_string()),
        text: truncate_web_context_text(&merged_text, MERGED_FETCH_MAX_CHARS),
        provider_id: provider_ids.join("+"),
        provider_kind: if provider_kinds.len() == 1 {
            provider_kinds.remove(0)
        } else {
            "mixed".into()
        },
        extraction_method: if methods.len() == 1 {
            methods.remove(0)
        } else {
            "merged_fetch".into()
        },
    }
}

fn push_unique_string(values: &mut Vec<String>, value: String) {
    if value.trim().is_empty() {
        return;
    }
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn apply_page_fetch(item: &mut WebEvidenceItem, page: PageFetchResult) {
    apply_page_provider_fetch(item, page_provider_fetch_from_native(page));
}

#[test]
fn initial_run_search_uses_the_single_original_query_without_replanning() {
    assert_eq!(
        initial_run_search_queries("最近世界杯战况如何？"),
        vec!["最近世界杯战况如何？".to_string()]
    );
}

#[test]
fn freshness_label_is_extracted_from_search_snippet() {
    assert_eq!(
        parse_freshness_label("snippet deterministic date: 2026-08-18T07:00:00Z value"),
        Some("2026-08-18T07:00:00Z".to_string())
    );
    assert_eq!(
        parse_freshness_label("no date here"),
        None,
        "missing date must remain None so news validation fails closed"
    );
}

#[test]
fn search_fetch_with_dated_snippet_produces_news_usable_freshness() {
    let fetch = SearchProviderFetch {
        body: "[1] title: Contract\nurl: https://source.invalid/contract\nsnippet: deterministic date: 2026-08-18T07:00:00Z"
            .to_string(),
        search_backend: WebSearchBackend::Provider,
        provider_id: "mcp.test".into(),
        provider_kind: "mcp".into(),
        failure_reason: None,
        diagnostic_summary: None,
    };
    let items = web_evidence_items_from_search_fetch(&fetch);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].failure_reason, None);
    assert_eq!(
        items[0].freshness_label.as_deref(),
        Some("2026-08-18T07:00:00Z")
    );
}

fn item(url: &str) -> WebEvidenceItem {
    WebEvidenceItem {
        url: url.into(),
        canonical_url: canonicalize_url(url),
        title: "Title".into(),
        domain: domain_from_url(url).unwrap_or_default(),
        snippet: "Snippet".into(),
        fetched_excerpt: None,
        completeness: Default::default(),
        provider_id: "mcp.test".into(),
        provider_kind: "mcp".into(),
        cost_class: "free".into(),
        raw_result_hash: result_hash(&[url]),
        extraction_method: "search_snippet".into(),
        trust_level: "external_untrusted".into(),
        retrieval_reason: "web.search".into(),
        search_backend: WebSearchBackend::Provider,
        source_rank: WebSourceRank::Unknown,
        freshness_label: None,
        failure_reason: None,
        conflict_group: None,
        conflict_note: None,
    }
}

fn contract_mcp_transport(mode: &str) -> String {
    crate::ai_runtime::mcp_stdio_test_support::contract_mcp_stdio_transport_config(mode, "1")
        .to_string()
}

#[tokio::test]
async fn disabled_broker_returns_empty_without_search() {
    let db = Database::open_in_memory().unwrap();
    let items = collect_web_evidence(
        &db,
        WebEvidenceBrokerInput {
            query: "topic".into(),
            urls: Vec::new(),
            enabled: false,
            max_search_results: 5,
            max_fetches: 0,
            provider_snapshots: Vec::new(),
            provider_selection_frozen: false,
            search_identity: SearchActionIdentity::default(),
            native_endpoint: None,
        },
    )
    .await
    .unwrap();

    assert!(items.is_empty());
}

#[test]
fn query_planner_strips_obvious_pii_and_limits_query_count() {
    let planned = plan_search_queries(
        "请帮我搜索 alice@example.com https://private.example/x \
         手机 13812345678 sqlite-vec 发布 2026 最新变化？再核对 Rust sqlite vec 用法。",
    );

    assert!(!planned.is_empty());
    assert!(planned.len() <= 3);
    let joined = planned.join("\n");
    assert!(!joined.contains("alice@example.com"));
    assert!(!joined.contains("https://private.example"));
    assert!(!joined.contains("13812345678"));
    assert!(joined.contains("sqlite-vec") || joined.contains("sqlite vec"));
    assert!(planned.iter().all(|query| query.chars().count() <= 160));
}

#[test]
fn deduplicates_urls() {
    let items = normalize_evidence_items(vec![
        item("https://example.com/a"),
        item("https://example.com/a"),
        item("https://example.com/b"),
    ]);

    assert_eq!(items.len(), 2);
}

#[test]
fn records_fetch_failure_without_failing_whole_task() {
    let item = failed_evidence_item(
        "https://example.com/a",
        "native.fetch",
        "web.fetch",
        "fetch_failed".into(),
    );

    assert_eq!(item.failure_reason.as_deref(), Some("fetch_failed"));
    assert_eq!(item.url, "https://example.com/a");
}

#[test]
fn web_usage_counts_only_successful_mcp_search_providers() {
    let mcp_fetch = SearchProviderFetch {
        body: "[1] title: MCP result\nurl: https://example.com/mcp\nsnippet: ok".into(),
        search_backend: WebSearchBackend::Provider,
        provider_id: "anysearch".into(),
        provider_kind: "mcp".into(),
        failure_reason: None,
        diagnostic_summary: None,
    };
    let empty_mcp_fetch = SearchProviderFetch {
        body: "no parseable rows".into(),
        search_backend: WebSearchBackend::Provider,
        provider_id: "empty-mcp".into(),
        provider_kind: "mcp".into(),
        failure_reason: None,
        diagnostic_summary: None,
    };

    let usage = web_evidence_usage_from_search_fetches([&mcp_fetch, &empty_mcp_fetch]);

    assert_eq!(usage.successful_search_requests.mcp, 1);
    assert_eq!(usage.successful_search_requests.native, 0);
    assert_eq!(usage.providers.len(), 1);
    assert!(usage.providers.iter().any(|provider| {
        provider.provider_id == "anysearch"
            && provider.provider_kind == "mcp"
            && provider.successful_search_requests == 1
    }));
}

#[test]
fn planned_query_fetch_flatten_preserves_query_order() {
    let first = SearchProviderFetch {
        body: "[1] title: first\nurl: https://example.com/first\nsnippet: first".into(),
        search_backend: WebSearchBackend::Provider,
        provider_id: "provider-a".into(),
        provider_kind: "mcp".into(),
        failure_reason: None,
        diagnostic_summary: None,
    };
    let second = SearchProviderFetch {
        body: "[1] title: second\nurl: https://example.com/second\nsnippet: second".into(),
        search_backend: WebSearchBackend::Provider,
        provider_id: "provider-b".into(),
        provider_kind: "mcp".into(),
        failure_reason: None,
        diagnostic_summary: None,
    };

    let flattened = flatten_planned_query_fetch_results(vec![
        vec![Ok(first)],
        vec![Err("query-two-failed".into()), Ok(second)],
    ]);

    assert_eq!(flattened.len(), 3);
    assert_eq!(flattened[0].as_ref().unwrap().provider_id, "provider-a");
    assert_eq!(flattened[1].as_ref().unwrap_err(), "query-two-failed");
    assert_eq!(flattened[2].as_ref().unwrap().provider_id, "provider-b");
}

#[tokio::test]
async fn broker_returns_page_fetch_failure_as_an_actionable_observation() {
    let db = Database::open_in_memory().unwrap();
    let (items, successful_fetch_providers) = enrich_with_page_fetches(
        &db,
        vec![item("https://localhost/a"), item("https://localhost/b")],
        1,
        &std::collections::BTreeSet::from(["https://localhost/a".to_string()]),
        &[],
        false,
    )
    .await
    .unwrap();

    assert_eq!(items.len(), 2);
    assert!(successful_fetch_providers.is_empty());
    assert!(items[0]
        .failure_reason
        .as_deref()
        .is_some_and(|reason| reason.starts_with("web_fetch_failed:")));
    assert_eq!(items[0].retrieval_reason, "web.fetch");
    assert!(items[0].fetched_excerpt.is_none());
    assert!(!items[0].snippet.is_empty());
    assert!(items[1].failure_reason.is_none());
}

#[tokio::test]
async fn nested_429_fetch_fails_over_within_the_frozen_route() {
    let db = Database::open_in_memory().unwrap();
    for (id, mode) in [
        ("fetch-primary", "fetch-rate-limit"),
        ("fetch-backup", "search-fetch"),
    ] {
        crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
            &db,
            &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
                id: id.into(),
                name: id.into(),
                kind: "mcp".into(),
                enabled: true,
                transport_kind: "stdio".into(),
                transport_config_json: contract_mcp_transport(mode),
                credential_refs_json: "{}".into(),
                web_search_mapping_json: Some(
                    r#"{"tool":"search","queryArg":"query","maxResultsArg":"max_results"}"#.into(),
                ),
                web_fetch_mapping_json: Some(r#"{"tool":"fetch","urlArg":"url"}"#.into()),
            },
        )
        .unwrap();
    }
    let available =
        crate::ai_runtime::mcp_runtime_registry::list_enabled_web_provider_mappings(&db).unwrap();
    let snapshots = ["fetch-primary", "fetch-backup"]
        .into_iter()
        .map(|id| {
            available
                .iter()
                .find(|snapshot| snapshot.id == id)
                .cloned()
                .expect("frozen provider snapshot")
        })
        .collect::<Vec<_>>();

    let output = collect_initial_run_web_evidence_with_usage(
        &db,
        WebEvidenceBrokerInput {
            query: "contract".into(),
            urls: vec!["https://source.invalid/contract".into()],
            enabled: true,
            max_search_results: 4,
            max_fetches: 1,
            provider_snapshots: snapshots,
            provider_selection_frozen: true,
            search_identity: SearchActionIdentity::default(),
            native_endpoint: None,
        },
    )
    .await
    .expect("backup fetch succeeds");

    assert_eq!(output.usage.successful_page_fetches, 1);
    assert!(output.items.iter().any(|item| {
        item.provider_id == "fetch-backup"
            && item
                .fetched_excerpt
                .as_deref()
                .is_some_and(|body| body.contains("fetch-result"))
    }));
    assert!(output.usage.providers.iter().any(|provider| {
        provider.provider_id == "fetch-backup" && provider.successful_page_fetches == 1
    }));
    let primary_health = crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_health(
        &db,
        "fetch-primary",
        "web.fetch",
    )
    .unwrap()
    .expect("primary health");
    assert_eq!(
        primary_health.last_failure_code.as_deref(),
        Some("mcp_provider_rate_limited")
    );
    let backup_health = crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_health(
        &db,
        "fetch-backup",
        "web.fetch",
    )
    .unwrap()
    .expect("backup health");
    assert!(backup_health.success_count >= 1);
}

#[tokio::test]
async fn search_without_usable_https_candidates_is_recorded_as_failed_health() {
    let db = Database::open_in_memory().unwrap();
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &db,
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "search-empty-health".into(),
            name: "search-empty-health".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: contract_mcp_transport("search-empty"),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(
                r#"{"tool":"search","queryArg":"query","maxResultsArg":"max_results"}"#.into(),
            ),
            web_fetch_mapping_json: None,
        },
    )
    .unwrap();
    let snapshot = crate::ai_runtime::mcp_runtime_registry::list_enabled_web_provider_mappings(&db)
        .unwrap()
        .into_iter()
        .find(|provider| provider.id == "search-empty-health")
        .expect("frozen search provider");

    let output = collect_initial_run_web_evidence_with_usage(
        &db,
        WebEvidenceBrokerInput {
            query: "contract".into(),
            urls: Vec::new(),
            enabled: true,
            max_search_results: 4,
            max_fetches: 0,
            provider_snapshots: vec![snapshot],
            provider_selection_frozen: true,
            search_identity: SearchActionIdentity::default(),
            native_endpoint: None,
        },
    )
    .await
    .expect("unusable search remains an actionable broker observation");

    assert_eq!(output.usage.successful_search_requests.mcp, 0);
    assert_eq!(output.usage.successful_search_requests.native, 0);
    assert!(!output.dual_path.native.supported);
    assert!(!output.dual_path.native.attempted);
    assert!(output.dual_path.mcp.supported);
    assert!(output.dual_path.mcp.attempted);
    assert!(!output.dual_path.mcp.succeeded);
    assert!(!output.dual_path.both_available_routes_attempted());
    let health = crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_health(
        &db,
        "search-empty-health",
        "web.search",
    )
    .unwrap()
    .expect("search health");
    assert_eq!(health.success_count, 0);
    assert_eq!(health.failure_count, 1);
    assert_eq!(
        health.last_failure_code.as_deref(),
        Some("mcp_search_parse_empty")
    );
}

#[test]
fn deep_fetches_are_limited_to_explicitly_allowed_urls() {
    let allowed = std::collections::BTreeSet::from(["https://example.com/current".into()]);

    assert!(is_allowed_page_fetch(
        &item("https://example.com/current#section"),
        &allowed,
    ));
    assert!(!is_allowed_page_fetch(
        &item("https://example.com/search-result"),
        &allowed,
    ));
}

#[test]
fn explicit_url_fetch_uses_the_page_title_instead_of_the_url_placeholder() {
    let mut item = explicit_url_item("https://example.com/article");
    apply_page_fetch(
        &mut item,
        PageFetchResult {
            completeness: Default::default(),
            title: "Actual article title".into(),
            text: "Article body".into(),
        },
    );
    assert_eq!(item.title, "Actual article title");
}

#[tokio::test]
async fn fetch_only_broker_entrypoints_never_dispatch_search_even_with_a_query() {
    let db = Database::open_in_memory().unwrap();
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &db,
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "fetch-only-boundary".into(),
            name: "fetch-only-boundary".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: contract_mcp_transport("search-fetch"),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(
                r#"{"tool":"search","queryArg":"query","maxResultsArg":"max_results"}"#.into(),
            ),
            web_fetch_mapping_json: Some(r#"{"tool":"fetch","urlArg":"url"}"#.into()),
        },
    )
    .unwrap();
    let input = WebEvidenceBrokerInput {
        query: "nonempty original question".into(),
        urls: vec!["https://source.invalid/contract".into()],
        enabled: true,
        max_search_results: 0,
        max_fetches: 1,
        provider_snapshots:
            crate::ai_runtime::mcp_runtime_registry::list_enabled_web_provider_mappings(&db)
                .unwrap(),
        provider_selection_frozen: true,
        search_identity: SearchActionIdentity::default(),
        native_endpoint: None,
    };
    for output in [
        collect_initial_run_web_evidence_with_usage(&db, input.clone())
            .await
            .unwrap(),
        collect_web_evidence_with_usage(&db, input).await.unwrap(),
    ] {
        assert_eq!(output.usage.successful_search_requests.mcp, 0);
        assert_eq!(output.usage.successful_page_fetches, 1);
        assert_eq!(output.items.len(), 1);
        assert!(output.items[0].fetched_excerpt.is_some());
    }
    assert!(
        crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_health(
            &db,
            "fetch-only-boundary",
            "web.search"
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn broker_applies_successful_page_fetch_excerpt() {
    let mut item = item("https://example.com/a");

    apply_page_fetch(
        &mut item,
        PageFetchResult {
            completeness: Default::default(),
            title: "Fetched title".into(),
            text: "Fetched body".into(),
        },
    );

    assert_eq!(item.fetched_excerpt.as_deref(), Some("Fetched body"));
}

#[test]
fn web_packet_excerpt_is_capped_for_large_fetched_pages() {
    let mut item = item("https://example.com/apple-watch");
    item.fetched_excerpt = Some("苹".repeat(87_000));

    let packets = web_evidence_items_to_packets("apple最新的手表是什么？", &[item]);

    assert_eq!(packets.len(), 1);
    assert!(packets[0].excerpt.chars().count() <= 4_000);
    assert!(packets[0].excerpt.contains("网页正文已按上下文预算截断"));
}

#[test]
fn apple_latest_prefetch_packet_stays_under_fast_model_budget() {
    let mut item = item("https://www.apple.com/tw/apple-watch-series-11/");
    item.fetched_excerpt = Some("苹".repeat(87_521));

    let packets = web_evidence_items_to_packets("apple最新的手表是什么？", &[item]);
    let prompt = crate::ai_runtime::model_gateway::ModelGateway::format_evidence_packets(&packets);
    let estimated = crate::ai_runtime::text_support::estimate_tokens(&prompt);

    assert!(estimated < 55_808);
}

#[test]
fn rejects_non_https_fetch_targets() {
    let rejected = failed_evidence_item(
        "http://example.com/a",
        "native.url",
        "explicit_url",
        "non_https_rejected".into(),
    );

    assert!(!is_https_url(&rejected.url));
    assert_eq!(
        rejected.failure_reason.as_deref(),
        Some("non_https_rejected")
    );
}

#[test]
fn explicit_url_items_carry_provider_metadata() {
    let item = explicit_url_item("https://Example.com/a#section");

    assert_eq!(item.canonical_url, "https://example.com/a");
    assert_eq!(item.provider_id, "native.url");
    assert_eq!(item.retrieval_reason, "explicit_url");
    assert_eq!(item.trust_level, "external_untrusted");
    assert!(item.failure_reason.is_none());
}

#[test]
fn search_provider_candidates_require_mcp_provider() {
    let db = Database::open_in_memory().unwrap();

    let err = search_provider_candidates(&db, &[], false).unwrap_err();

    assert!(err.to_string().contains("web_search_provider_missing"));
}

#[test]
fn frozen_absent_search_provider_never_reselects_mid_run() {
    let db = Database::open_in_memory().unwrap();

    let error = search_provider_candidates(&db, &[], true)
        .expect_err("a frozen absence must not dynamically select a later provider");

    assert_eq!(error.to_string(), "web_search_provider_missing");
}

#[test]
fn frozen_fetch_route_keeps_native_transport_as_the_final_safe_fallback() {
    let db = Database::open_in_memory().expect("database");
    let snapshots = vec![
        crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary {
            id: "primary".into(),
            kind: "mcp".into(),
            transport_kind: "stdio".into(),
            web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
            web_fetch_mapping_json: Some(r#"{"tool":"fetch"}"#.into()),
            provider_config_hash: "primary-hash".into(),
        },
        crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary {
            id: "backup".into(),
            kind: "mcp".into(),
            transport_kind: "stdio".into(),
            web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
            web_fetch_mapping_json: Some(r#"{"tool":"fetch"}"#.into()),
            provider_config_hash: "backup-hash".into(),
        },
    ];

    assert_eq!(
        fetch_provider_candidates(&db, Some("primary"), &snapshots, true),
        vec![
            FetchProviderCandidate::Mcp("primary".into()),
            FetchProviderCandidate::Mcp("backup".into()),
            FetchProviderCandidate::Native,
        ]
    );
}

#[test]
fn diagnostic_search_smoke_observation_does_not_persist_provider_health() {
    let db = Database::open_in_memory().unwrap();
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &db,
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "diagnostic-provider".into(),
            name: "Diagnostic provider".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: r#"{"command":"mcp-server"}"#.into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .unwrap();

    observe_mcp_search_provider_call(
        &db,
        "diagnostic-provider",
        true,
        Duration::from_millis(8),
        None,
        false,
    );

    assert!(
        crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_runtime(
            &db,
            "diagnostic-provider"
        )
        .unwrap()
        .is_none()
    );
    assert!(
        crate::ai_runtime::mcp_runtime_registry::web_evidence_provider_health(
            &db,
            "diagnostic-provider",
            "web.search"
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn search_provider_candidates_preserve_ordered_mcp_failover_route() {
    let db = Database::open_in_memory().unwrap();
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &db,
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "anysearch".into(),
            name: "AnySearch".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: "{}".into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .unwrap();
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &db,
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "brave".into(),
            name: "Brave Search".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: "{}".into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"brave_web_search"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .unwrap();
    crate::ai_runtime::mcp_runtime_registry::save_web_search_route_config(
        &db,
        &crate::ai_runtime::mcp_runtime_registry::WebSearchRouteConfig {
            candidate_provider_ids: vec!["brave".into(), "anysearch".into()],
        },
    )
    .unwrap();

    let candidates = search_provider_candidates(&db, &[], false).unwrap();

    assert_eq!(
        candidates,
        vec![
            SearchProviderCandidate::Mcp("brave".into()),
            SearchProviderCandidate::Mcp("anysearch".into()),
        ]
    );
}

#[test]
fn search_provider_candidates_use_single_mcp_without_saved_choice() {
    let db = Database::open_in_memory().unwrap();
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &db,
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "mcp-search".into(),
            name: "MCP Search".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: "{}".into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
            web_fetch_mapping_json: None,
        },
    )
    .unwrap();

    let candidates = search_provider_candidates(&db, &[], false).unwrap();

    assert_eq!(
        candidates,
        vec![SearchProviderCandidate::Mcp("mcp-search".into())]
    );
}

#[test]
fn frozen_search_provider_snapshot_fails_closed_after_mapping_changes() {
    let db = Database::open_in_memory().unwrap();
    let provider = crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
        id: "frozen-search".into(),
        name: "Frozen Search".into(),
        kind: "mcp".into(),
        enabled: true,
        transport_kind: "stdio".into(),
        transport_config_json: "{}".into(),
        credential_refs_json: "{}".into(),
        web_search_mapping_json: Some(r#"{"tool":"search_v1"}"#.into()),
        web_fetch_mapping_json: None,
    };
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(&db, &provider).unwrap();
    let snapshot =
        crate::ai_runtime::mcp_runtime_registry::resolve_selected_web_search_provider(&db).unwrap();

    let mut changed = provider;
    changed.web_search_mapping_json = Some(r#"{"tool":"search_v2"}"#.into());
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(&db, &changed).unwrap();

    let error = resolve_mcp_provider_mapping(&db, "frozen-search", "web.search", Some(&snapshot))
        .unwrap_err();
    assert_eq!(error.to_string(), "web_search_provider_snapshot_changed");
}

#[test]
fn mcp_mapping_builds_provider_specific_search_and_fetch_args() {
    let search = build_mcp_search_arguments(
        r#"{"tool":"tavily-search","queryArg":"query","maxResultsArg":"max_results","extraArgs":{"topic":"general"}}"#,
        "rust mcp",
        7,
    );
    assert_eq!(
        search,
        serde_json::json!({"query": "rust mcp", "max_results": 7, "topic": "general"})
    );

    let initial_search = build_mcp_search_arguments(
        r#"{"tool":"tavily-search","queryArg":"query","maxResultsArg":"max_results"}"#,
        "current news",
        5,
    );
    assert!(
        initial_search["max_results"]
            .as_u64()
            .is_some_and(|limit| limit <= 5),
        "the initial evidence request must preserve its bounded result limit"
    );

    let fetch = build_mcp_fetch_arguments(
        r#"{"tool":"tavily-extract","urlListArg":"urls","extraArgs":{"extract_depth":"basic"}}"#,
        "https://example.com/a",
        12000,
    );
    assert_eq!(
        fetch,
        serde_json::json!({"urls": ["https://example.com/a"], "extract_depth": "basic"})
    );
}

#[test]
fn legacy_anysearch_mapping_gets_a_runtime_result_limit_without_mutation() {
    let db = Database::open_in_memory().unwrap();
    let legacy_mapping = r#"{"tool":"search","queryArg":"query"}"#;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO web_evidence_providers
             (id, name, kind, enabled, transport_kind, transport_config_json,
              credential_refs_json, web_search_mapping_json, web_fetch_mapping_json,
              provider_config_hash, updated_at)
             VALUES (?1, ?2, 'mcp', 1, 'https', ?3, '{}', ?4, NULL, 'legacy', datetime('now'))",
            rusqlite::params![
                "anysearch-legacy",
                "AnySearch",
                r#"{"url":"https://api.anysearch.com/mcp"}"#,
                legacy_mapping,
            ],
        )?;
        Ok(())
    })
    .unwrap();

    let effective = effective_mcp_search_mapping(&db, "anysearch-legacy", legacy_mapping);
    let arguments = build_mcp_search_arguments(&effective, "latest news", 5);

    assert_eq!(arguments["query"], "latest news");
    assert_eq!(arguments["max_results"], 5);
    let (stored_mapping, stored_hash): (String, String) = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT web_search_mapping_json, provider_config_hash
                 FROM web_evidence_providers WHERE id = ?1",
                ["anysearch-legacy"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert!(
        !stored_mapping.contains("maxResultsArg"),
        "runtime overlay must not persist mapping upgrades"
    );
    assert_eq!(stored_hash, "legacy");
}

#[test]
fn mcp_search_body_normalizes_structured_result_arrays() {
    let body = mcp_search_result_body(&serde_json::json!({
        "results": [
            {
                "title": "Iris MCP",
                "url": "https://example.com/iris",
                "content": "evidence snippet"
            }
        ]
    }));
    let rows = parse_search_result_rows(&body);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Iris MCP");
    assert_eq!(rows[0].url, "https://example.com/iris");
    assert_eq!(rows[0].snippet, "evidence snippet");
}

#[test]
fn mcp_search_body_normalizes_anysearch_markdown_content_text() {
    let body = mcp_search_result_body(&serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": "## Search Results\n\n### 1. Iris note app\n- **URL**: https://example.com/iris\n- Iris is a local-first note app.\n\n### 2. Iris docs\n- **URL**: https://docs.example.com/iris\n- Documentation snippet."
            }
        ]
    }));
    let rows = parse_search_result_rows(&body);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title, "Iris note app");
    assert_eq!(rows[0].url, "https://example.com/iris");
    assert_eq!(rows[0].snippet, "Iris is a local-first note app.");
    assert_eq!(rows[1].title, "Iris docs");
    assert_eq!(rows[1].url, "https://docs.example.com/iris");
}

#[test]
fn mcp_search_body_normalizes_live_anysearch_multilingual_markdown() {
    let body = mcp_search_result_body(&serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": "## Search Results (10 results, 4358ms)\n\n### 1. 高市総理がインドのモディ首相と会談へ　重要鉱物や半導体で連携確認\n- **URL**: https://www.fnn.jp/articles/-/1069028\n- 高市総理がインドのモディ首相と会談へ 重要鉱物や半導体で連携確認 対中国見据え協力深化へ協議...\n\n### 2. 中國正打擊日本的痛處，但高市早苗會屈服嗎？ - BBC News 中文\n- **URL**: https://www.bbc.com/zhongwen/articles/c178qrr29d1o/trad\n- 從日本首相高市早苗發表導致日中關係跌至多年來最低點的言論以來，北京方面一直在以各種方式加大對日本的施壓。"
            }
        ]
    }));
    let rows = parse_search_result_rows(&body);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].url, "https://www.fnn.jp/articles/-/1069028");
    assert_eq!(
        rows[1].url,
        "https://www.bbc.com/zhongwen/articles/c178qrr29d1o/trad"
    );
    assert!(rows[1].snippet.contains("北京方面"));
}

#[test]
fn mcp_search_body_normalizes_firecrawl_text_json_results() {
    let body = mcp_search_result_body(&serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": r#"{
                    "success": true,
                    "data": {
                        "web": [
                            {
                                "url": "https://example.com/firecrawl",
                                "title": "Firecrawl result",
                                "description": "Firecrawl description"
                            }
                        ]
                    }
                }"#
            }
        ]
    }));
    let rows = parse_search_result_rows(&body);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Firecrawl result");
    assert_eq!(rows[0].url, "https://example.com/firecrawl");
    assert_eq!(rows[0].snippet, "Firecrawl description");
}

#[test]
fn mcp_search_parse_empty_returns_diagnostic_failure_item() {
    let items = web_evidence_items_from_search_fetch(&SearchProviderFetch {
        body: "MCP tool returned prose without links".into(),
        search_backend: WebSearchBackend::Provider,
        provider_id: "mcp.prose".into(),
        provider_kind: "mcp".into(),
        failure_reason: None,
        diagnostic_summary: None,
    });

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].provider_id, "mcp.prose");
    assert_eq!(
        items[0].failure_reason.as_deref(),
        Some("mcp_search_parse_empty:text_without_url")
    );
}

#[test]
fn mcp_search_parse_empty_diagnostic_classifies_empty_and_schema_failures() {
    let empty = diagnose_mcp_search_result(
        "anysearch",
        &serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "   "
                }
            ]
        }),
    );
    assert_eq!(
        empty.failure_reason.as_deref(),
        Some("mcp_search_parse_empty:empty_body")
    );

    let schema = diagnose_mcp_search_result(
        "anysearch",
        &serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "{\"items\":[{\"title\":\"No URL\"}]}"
                }
            ]
        }),
    );
    assert_eq!(
        schema.failure_reason.as_deref(),
        Some("mcp_search_parse_empty:unrecognized_schema")
    );
}

#[test]
fn mcp_search_diagnostic_rejects_http_only_rows_as_unusable_evidence() {
    let diagnostic = diagnose_mcp_search_result(
        "anysearch",
        &serde_json::json!({
            "content": [{
                "type": "text",
                "text": "### 1. Insecure result\n- **URL**: http://example.com/news\n- **Snippet**: must not become evidence"
            }]
        }),
    );

    assert_eq!(diagnostic.parsed_row_count, 1);
    assert_eq!(diagnostic.usable_https_row_count, 0);
    assert_eq!(diagnostic.rejected_non_https_row_count, 1);
    assert_eq!(
        diagnostic.failure_reason.as_deref(),
        Some("mcp_search_no_usable_https_results")
    );
    assert!(
        !search_probe_counts_toward_circuit(&diagnostic),
        "empty or unusable HTTPS rows must not open the provider circuit"
    );
}

#[test]
fn empty_search_parse_does_not_count_toward_the_provider_circuit() {
    let empty = diagnose_mcp_search_result(
        "anysearch",
        &serde_json::json!({
            "content": [{ "type": "text", "text": "" }]
        }),
    );
    assert_eq!(empty.usable_https_row_count, 0);
    assert!(!search_probe_counts_toward_circuit(&empty));

    let timeout = McpSearchResultDiagnostic {
        body: String::new(),
        result_shape: "error".into(),
        content_text_length: 0,
        contains_url_marker: false,
        parsed_row_count: 0,
        usable_https_row_count: 0,
        rejected_non_https_row_count: 0,
        first_url_domain: None,
        failure_reason: Some("mcp_provider_timeout".into()),
        application_failure: None,
    };
    assert!(search_probe_counts_toward_circuit(&timeout));
}

#[test]
fn mcp_search_is_error_result_is_not_reported_as_parse_empty() {
    let diagnostic = diagnose_mcp_search_result(
        "anysearch",
        &serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "invalid_api_key\nInvalid API key."
                }
            ],
            "isError": true
        }),
    );
    assert_eq!(diagnostic.content_text_length, 32);
    assert_eq!(
        diagnostic.application_failure,
        Some(McpApplicationFailureKind::AuthFailed)
    );
    let fetch = SearchProviderFetch {
        body: diagnostic.body,
        search_backend: WebSearchBackend::Provider,
        provider_id: "anysearch".into(),
        provider_kind: "mcp".into(),
        failure_reason: diagnostic.failure_reason,
        diagnostic_summary: None,
    };
    let items = web_evidence_items_from_search_fetch(&fetch);

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].failure_reason.as_deref(),
        Some("agent_run_web_provider_auth_failed")
    );
    assert!(!items[0].snippet.contains("Invalid API key"));
}
#[test]
fn mcp_success_suppresses_mcp_search_failure_item() {
    let mut items = web_evidence_items_from_search_fetch(&SearchProviderFetch {
        body: "[1] title: MCP result\nurl: https://example.com/mcp\nsnippet: ok".into(),
        search_backend: WebSearchBackend::Provider,
        provider_id: "anysearch".into(),
        provider_kind: "mcp".into(),
        failure_reason: None,
        diagnostic_summary: None,
    });
    items.extend(web_evidence_items_from_search_fetch(&SearchProviderFetch {
        body: "unparseable mcp body".into(),
        search_backend: WebSearchBackend::Provider,
        provider_id: "empty-mcp".into(),
        provider_kind: "mcp".into(),
        failure_reason: None,
        diagnostic_summary: None,
    }));

    suppress_search_provider_failures_if_success(&mut items);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].provider_id, "anysearch");
    assert!(items[0].failure_reason.is_none());
}

#[test]
fn provider_circuit_open_blocks_broker_provider_call() {
    let provider_id = "broker-circuit-open-test";
    for _ in 0..5 {
        record_provider_failure(provider_id);
    }

    let err = ensure_provider_circuit_allows(provider_id).unwrap_err();
    assert!(err.to_string().contains("provider_disabled"));
    assert!(err.to_string().contains("circuit_open"));
}

#[test]
fn credential_and_mapping_errors_do_not_participate_in_provider_circuit_breaking() {
    assert_eq!(
        sanitize_mcp_runtime_error(AppError::msg(
            "auth_failed: bearer credential must contain the raw key only",
        ))
        .to_string(),
        "agent_run_web_provider_auth_failed"
    );
    assert_eq!(
        sanitize_mcp_runtime_error(AppError::msg("auth_missing: credential_unreadable",))
            .to_string(),
        "agent_run_web_provider_auth_failed"
    );
    assert!(!is_transient_provider_error(&AppError::msg(
        "agent_run_web_provider_auth_failed",
    )));
    assert!(!is_transient_provider_error(&AppError::msg(
        "auth_missing: credential_unreadable",
    )));
    assert!(is_transient_provider_error(&AppError::msg(
        "connection reset by peer",
    )));
}

#[test]
fn host_runtime_failures_are_reduced_to_safe_distinct_health_codes() {
    assert_eq!(
        mcp_runtime_failure_code(&AppError::msg("output_too_large: response exceeded cap")),
        "mcp_provider_output_too_large"
    );
    assert_eq!(
        sanitize_mcp_runtime_error(AppError::msg("output_too_large: private provider body"))
            .to_string(),
        "mcp_provider_output_too_large"
    );
    assert_eq!(
        mcp_runtime_failure_code(&AppError::msg("connection reset by peer")),
        "mcp_provider_transport_error"
    );
    assert!(is_transient_provider_error(&AppError::msg(
        "mcp_provider_transport_error",
    )));
}

#[test]
fn fetch_provider_candidates_use_mcp_then_native() {
    let db = Database::open_in_memory().unwrap();
    crate::ai_runtime::mcp_runtime_registry::upsert_web_evidence_provider(
        &db,
        &crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderInput {
            id: "mcp-fetch".into(),
            name: "MCP Fetch".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json: "{}".into(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: None,
            web_fetch_mapping_json: Some(r#"{"tool":"fetch"}"#.into()),
        },
    )
    .unwrap();

    let candidates = fetch_provider_candidates(&db, None, &[], false);

    assert_eq!(
        candidates,
        vec![
            FetchProviderCandidate::Mcp("mcp-fetch".into()),
            FetchProviderCandidate::Native,
        ]
    );
}

#[test]
fn frozen_fetch_candidates_prefer_origin_then_keep_frozen_order() {
    let snapshot = |id: &str, has_fetch: bool| {
        crate::ai_runtime::mcp_runtime_registry::WebEvidenceProviderMappingSummary {
            id: id.into(),
            kind: "mcp".into(),
            transport_kind: "stdio".into(),
            provider_config_hash: format!("hash-{id}"),
            web_search_mapping_json: Some(r#"{"tool":"search"}"#.into()),
            web_fetch_mapping_json: has_fetch.then(|| r#"{"tool":"fetch"}"#.into()),
        }
    };
    let frozen = vec![
        snapshot("primary", true),
        snapshot("origin", true),
        snapshot("search-only", false),
    ];

    let candidates = fetch_provider_candidates(
        &Database::open_in_memory().unwrap(),
        Some("origin"),
        &frozen,
        true,
    );

    assert_eq!(
        candidates,
        vec![
            FetchProviderCandidate::Mcp("origin".into()),
            FetchProviderCandidate::Mcp("primary".into()),
            FetchProviderCandidate::Native,
        ]
    );
}

#[test]
fn mcp_page_fetch_result_extracts_text_and_provider_metadata() {
    let fetch = mcp_page_fetch_result(
        "mcp-fetch",
        "https://example.com/a",
        &serde_json::json!({
            "title": "Fetched title",
            "text": "Fetched body"
        }),
    )
    .unwrap();

    assert_eq!(fetch.provider_id, "mcp-fetch");
    assert_eq!(fetch.provider_kind, "mcp");
    assert_eq!(fetch.title, "Fetched title");
    assert_eq!(fetch.text, "Fetched body");
    assert_eq!(fetch.extraction_method, "mcp_fetch_text");
}

#[test]
fn mcp_page_fetch_rejects_application_error_without_exposing_error_body() {
    let error = mcp_page_fetch_result(
        "mcp-fetch",
        "https://example.com/a",
        &serde_json::json!({
            "content": [{
                "type": "text",
                "text": "invalid_api_key\nInvalid API key."
            }],
            "isError": true
        }),
    )
    .unwrap_err()
    .to_string();

    assert_eq!(error, "agent_run_web_provider_auth_failed");
    assert!(!error.contains("Invalid API key"));
}

#[test]
fn mcp_page_fetch_rejects_nested_rate_limit_error_envelope() {
    let error = mcp_page_fetch_result(
        "mcp-fetch",
        "https://example.com/a",
        &serde_json::json!({
            "content": [{
                "type": "text",
                "text": r#"{"error":"Extract failed","status":429,"message":"rate limited"}"#
            }]
        }),
    )
    .expect_err("an MCP application error must never become fetched body")
    .to_string();

    assert_eq!(error, "mcp_provider_rate_limited");
    assert!(!error.contains("Extract failed"));
}

#[test]
fn mcp_page_fetch_rejects_rate_limited_result_entry_even_with_body() {
    let result = serde_json::json!({
        "results": [{
            "url": "https://example.com/a",
            "status": 429,
            "error": "rate limited",
            "raw_content": "this fallback error body must never become evidence"
        }]
    });
    let error = mcp_page_fetch_result("mcp-fetch", "https://example.com/a", &result)
        .expect_err("an errored result entry must never become fetched body")
        .to_string();

    assert_eq!(error, "mcp_provider_rate_limited");
    assert_eq!(
        mcp_fetch_failure_kind(&result),
        Some(McpApplicationFailureKind::RateLimited)
    );
}

#[test]
fn mcp_fetch_error_detection_ignores_empty_flags_and_article_words() {
    let fetch = mcp_page_fetch_result(
        "mcp-fetch",
        "https://example.com/a",
        &serde_json::json!({
            "error": "",
            "success": true,
            "url": "https://example.com/a",
            "title": "可靠性文章",
            "text": "正文讨论 error handling，但这只是文章内容，不是 MCP 错误信封。"
        }),
    )
    .expect("article words must not become an application failure");

    assert!(fetch.text.contains("error handling"));
}

#[test]
fn mcp_page_fetch_extracts_matching_nested_raw_content() {
    let fetch = mcp_page_fetch_result(
        "mcp-fetch",
        "https://example.com/a#section",
        &serde_json::json!({
            "content": [{
                "type": "text",
                "text": r#"{"results":[{"url":"https://example.com/a","title":"Fetched title","raw_content":"完整的网页正文，而不是搜索结果摘要。"}]}"#
            }]
        }),
    )
    .expect("matching nested raw_content");

    assert_eq!(fetch.title, "Fetched title");
    assert_eq!(fetch.text, "完整的网页正文，而不是搜索结果摘要。");
    assert_eq!(fetch.extraction_method, "mcp_fetch_raw_content");
}

#[test]
fn mcp_page_fetch_rejects_nested_body_for_a_different_url() {
    let error = mcp_page_fetch_result(
        "mcp-fetch",
        "https://example.com/requested",
        &serde_json::json!({
            "content": [{
                "type": "text",
                "text": r#"{"results":[{"url":"https://example.com/other","raw_content":"另一页面的完整正文。"}]}"#
            }]
        }),
    )
    .expect_err("a body from another URL must not be accepted")
    .to_string();

    assert_eq!(error, "agent_run_web_fetch_url_mismatch");
}

#[test]
fn mcp_page_fetch_preserves_case_sensitive_path_and_query_for_url_ownership() {
    let error = mcp_page_fetch_result(
        "mcp-fetch",
        "HTTPS://Example.COM/Article/ABC?sig=XyZ#section",
        &serde_json::json!({
            "results": [{
                "url": "https://example.com/article/abc?sig=xyz",
                "raw_content": "正文来自另一个大小写敏感资源。"
            }]
        }),
    )
    .expect_err("path and query case changes must not match the requested resource")
    .to_string();

    assert_eq!(error, "agent_run_web_fetch_url_mismatch");
    assert_eq!(
        canonicalize_url("HTTPS://Example.COM/Article/ABC?sig=XyZ#section"),
        "https://example.com/Article/ABC?sig=XyZ"
    );
}

#[test]
fn mcp_page_fetch_rejects_title_only_and_search_result_wrappers() {
    for result in [
        serde_json::json!({
            "title": "Only a title",
            "url": "https://example.com/a",
            "text": "Only a title\nhttps://example.com/a"
        }),
        serde_json::json!({
            "results": [{
                "title": "Search result",
                "url": "https://example.com/a",
                "content": "short search snippet"
            }]
        }),
    ] {
        let error = mcp_page_fetch_result("mcp-fetch", "https://example.com/a", &result)
            .expect_err("search metadata is not fetched body")
            .to_string();
        assert_eq!(error, "agent_run_web_fetch_body_unusable");
    }
}

#[test]
fn successful_fetch_provider_results_are_merged_not_raced() {
    let merged = merge_page_provider_fetches(
        "https://example.com/a",
        vec![
            PageProviderFetch {
                completeness: Default::default(),
                title: "Native title".into(),
                text: "Native body".into(),
                provider_id: "native.fetch".into(),
                provider_kind: "native".into(),
                extraction_method: "native_readability".into(),
            },
            PageProviderFetch {
                completeness: Default::default(),
                title: "MCP title".into(),
                text: "MCP body".into(),
                provider_id: "mcp-fetch".into(),
                provider_kind: "mcp".into(),
                extraction_method: "mcp_fetch".into(),
            },
        ],
    );

    assert!(merged.text.contains("Native body"));
    assert!(merged.text.contains("MCP body"));
    assert_eq!(merged.provider_id, "native.fetch+mcp-fetch");
    assert_eq!(merged.provider_kind, "mixed");
    assert_eq!(merged.extraction_method, "merged_fetch");
}

#[test]
fn merged_fetch_provider_text_has_total_cap() {
    let merged = merge_page_provider_fetches(
        "https://example.com/a",
        vec![
            PageProviderFetch {
                completeness: Default::default(),
                title: "Native title".into(),
                text: "苹".repeat(12_000),
                provider_id: "native.fetch".into(),
                provider_kind: "native".into(),
                extraction_method: "native_readability".into(),
            },
            PageProviderFetch {
                completeness: Default::default(),
                title: "MCP title".into(),
                text: "表".repeat(12_000),
                provider_id: "mcp-fetch".into(),
                provider_kind: "mcp".into(),
                extraction_method: "mcp_fetch".into(),
            },
        ],
    );

    assert!(merged.text.chars().count() <= 12_000);
    assert!(merged.text.contains("网页正文已按上下文预算截断"));
    assert_eq!(merged.provider_kind, "mixed");
}

#[test]
fn duplicate_url_conflicts_are_marked_without_adjudication() {
    let mut first = item("https://example.com/a");
    first.snippet = "One claim".into();
    let mut second = item("https://example.com/a#fragment");
    second.snippet = "Different claim".into();

    let items = normalize_evidence_items(vec![first, second]);

    assert_eq!(items.len(), 1);
    assert!(items[0].conflict_group.is_some());
    assert!(items[0]
        .conflict_note
        .as_deref()
        .unwrap_or_default()
        .contains("不完全一致"));
}
