//! End-to-end quality gates for the v1.2.6 hybrid retrieval broker.
//!
//! The suite indexes a real fixture vault then invokes the public broker.  It
//! deliberately disables vectors, so the default CI path never downloads a
//! model.  Vector quality belongs to a separately provisioned model gate.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use iris_lib::ai_runtime::retrieval_broker::{
    hybrid_retrieve_with_diagnostics, RetrievalLayerStatus, RetrievalLayers, RetrievalRequest,
};
use iris_lib::ai_runtime::retrieval_scope::RetrievalScope;
use iris_lib::indexer::scan::index_vault_incremental;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;
use serde::Deserialize;

const FIXTURE_VERSION: &str = "v1.2.6";
const POSITIVE_RECALL_AT_5_MIN: f64 = 0.80;
const POSITIVE_RECALL_AT_30_MIN: f64 = 0.95;
const NO_ANSWER_FALSE_POSITIVE_RATE_MAX: f64 = 0.10;
const NDCG_AT_10_MIN: f64 = 0.85;
const METADATA_MATCH_QUERY_MIN: usize = 10;
const SCOPE_LEAK_COUNT_MAX: usize = 0;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvalFixture {
    version: String,
    notes: Vec<FixtureNote>,
    queries: Vec<EvalQuery>,
}

#[derive(Debug, Deserialize)]
struct FixtureNote {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvalQuery {
    id: String,
    query: String,
    expected_paths: Vec<String>,
    #[serde(default)]
    scope: FixtureScope,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureScope {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    path_prefixes: Vec<String>,
    #[serde(default)]
    required_tags: Vec<String>,
}

#[derive(Debug, Default)]
struct BrokerMetrics {
    positive_queries: usize,
    hits_at_5: usize,
    hits_at_30: usize,
    reciprocal_rank_sum: f64,
    normalized_discounted_gain_sum: f64,
    metadata_match_queries: usize,
    no_answer_queries: usize,
    no_answer_false_positives: usize,
    scope_leaks: usize,

    retrieval_latencies_ms: Vec<f64>,
}

impl BrokerMetrics {
    fn recall_at_5(&self) -> f64 {
        ratio(self.hits_at_5, self.positive_queries)
    }

    fn recall_at_30(&self) -> f64 {
        ratio(self.hits_at_30, self.positive_queries)
    }

    fn mrr_at_10(&self) -> f64 {
        if self.positive_queries == 0 {
            0.0
        } else {
            self.reciprocal_rank_sum / self.positive_queries as f64
        }
    }

    fn ndcg_at_10(&self) -> f64 {
        if self.positive_queries == 0 {
            0.0
        } else {
            self.normalized_discounted_gain_sum / self.positive_queries as f64
        }
    }

    fn no_answer_false_positive_rate(&self) -> f64 {
        ratio(self.no_answer_false_positives, self.no_answer_queries)
    }

    fn p95_ms(&mut self) -> f64 {
        if self.retrieval_latencies_ms.is_empty() {
            return 0.0;
        }
        self.retrieval_latencies_ms.sort_by(f64::total_cmp);
        let index = ((self.retrieval_latencies_ms.len() - 1) as f64 * 0.95).ceil() as usize;
        self.retrieval_latencies_ms[index]
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("docs")
        .join("eval")
        .join("fixtures")
        .join("rag-v2-vault")
}

fn load_fixture() -> EvalFixture {
    let labels = fixture_root().join("labels.json");
    let content = std::fs::read_to_string(&labels)
        .unwrap_or_else(|error| panic!("read {}: {error}", labels.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|error| panic!("parse {}: {error}", labels.display()))
}

fn request_for(query: &EvalQuery) -> RetrievalRequest {
    RetrievalRequest {
        query: query.query.clone(),
        max_results: 30,
        layers: RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        note_context: None,
        file_id_context: None,
        scope: RetrievalScope {
            paths: query.scope.paths.clone(),
            path_prefixes: query.scope.path_prefixes.clone(),
            required_tags: query.scope.required_tags.clone(),
        },
        runtime_documents: Vec::new(),
        corpus_config: None,
    }
}

fn crate_content_hash(content: &str) -> String {
    iris_lib::cas::hash::content_hash_str(content)
}
fn first_expected_rank(paths: &[String], expected: &[String], max_results: usize) -> Option<usize> {
    paths
        .iter()
        .take(max_results)
        .position(|path| expected.iter().any(|candidate| candidate == path))
        .map(|index| index + 1)
}

fn packet_respects_scope(packet_path: &str, scope: &FixtureScope) -> bool {
    if !scope.paths.is_empty() && !scope.paths.iter().any(|path| path == packet_path) {
        return false;
    }
    if !scope.path_prefixes.is_empty()
        && !scope
            .path_prefixes
            .iter()
            .any(|prefix| packet_path.starts_with(prefix))
    {
        return false;
    }
    true
}

fn packet_has_valid_citation(packet: &iris_lib::ai_runtime::ContextPacket) -> bool {
    let Some(span) = packet.source_span.as_ref() else {
        return false;
    };
    packet
        .source_path
        .as_deref()
        .is_some_and(|path| !path.is_empty())
        && !packet.content_hash.is_empty()
        && span.end > span.start
        && !packet.excerpt.trim().is_empty()
}

#[test]
fn rag_v2_fixture_contract_has_48_notes_and_60_labeled_queries() {
    let fixture = load_fixture();
    assert_eq!(fixture.version, FIXTURE_VERSION);
    assert_eq!(
        fixture.notes.len(),
        48,
        "fixture must contain 48 synthetic notes"
    );
    assert_eq!(
        fixture.queries.len(),
        60,
        "fixture must contain 60 labeled queries"
    );

    let declared: BTreeSet<_> = fixture
        .notes
        .iter()
        .map(|note| note.path.as_str())
        .collect();
    assert_eq!(declared.len(), 48, "note paths must be unique");
    for note in &fixture.notes {
        assert!(
            fixture_root().join(&note.path).is_file(),
            "missing fixture note {}",
            note.path
        );
    }

    let query_ids: BTreeSet<_> = fixture
        .queries
        .iter()
        .map(|query| query.id.as_str())
        .collect();
    assert_eq!(query_ids.len(), 60, "query ids must be unique");
    assert!(fixture
        .queries
        .iter()
        .any(|query| !query.expected_paths.is_empty()));
    assert!(fixture
        .queries
        .iter()
        .any(|query| query.expected_paths.is_empty()));

    // Verify FTS CJK matching: index a known fixture and probe it.
    let conn = Connection::open_in_memory().expect("open in-memory database");
    migrate_up(&conn).expect("migrate database");
    index_vault_incremental(&conn, &fixture_root())
        .expect("index fixture vault without embeddings");

    let probe_safe = iris_lib::ai_runtime::retrieval_broker::escape_fts5_query("要约");
    let match_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH ?1",
            [&probe_safe],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert!(
        match_count > 0,
        "FTS5 must match Chinese bigrams from fixture notes (probe={})",
        probe_safe
    );
}

#[test]
fn rag_v2_hybrid_broker_meets_deterministic_fixture_gates() {
    let fixture = load_fixture();
    let conn = Connection::open_in_memory().expect("open in-memory database");
    migrate_up(&conn).expect("migrate database");
    index_vault_incremental(&conn, &fixture_root())
        .expect("index fixture vault without embeddings");

    let indexed_files: i64 = conn
        .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
        .expect("count indexed fixture notes");
    assert_eq!(indexed_files, 48, "all fixture notes must be indexed");

    let mut metrics = BrokerMetrics::default();
    for query in &fixture.queries {
        let start = Instant::now();
        let outcome = hybrid_retrieve_with_diagnostics(&conn, &request_for(query))
            .unwrap_or_else(|error| panic!("broker failed for {}: {error}", query.id));
        metrics
            .retrieval_latencies_ms
            .push(start.elapsed().as_secs_f64() * 1_000.0);

        assert!(
            outcome.diagnostics.iter().any(|diagnostic| {
                diagnostic.layer == "fts" && diagnostic.status != RetrievalLayerStatus::QueryError
            }),
            "{} must exercise the FTS broker layer",
            query.id
        );
        if outcome.diagnostics.iter().any(|diagnostic| {
            diagnostic.layer == "metadata" && diagnostic.status == RetrievalLayerStatus::Ok
        }) {
            metrics.metadata_match_queries += 1;
        }
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.layer == "metadata"),
            "{} must exercise the metadata broker layer",
            query.id
        );

        let paths: Vec<String> = outcome
            .packets
            .iter()
            .filter_map(|packet| packet.source_path.clone())
            .collect();
        metrics.scope_leaks += paths
            .iter()
            .filter(|path| !packet_respects_scope(path, &query.scope))
            .count();

        if query.expected_paths.is_empty() {
            metrics.no_answer_queries += 1;
            if !paths.is_empty() {
                metrics.no_answer_false_positives += 1;
            }
            continue;
        }

        metrics.positive_queries += 1;
        let rank_at_5 = first_expected_rank(&paths, &query.expected_paths, 5);
        let rank_at_30 = first_expected_rank(&paths, &query.expected_paths, 30);
        if rank_at_5.is_some() {
            metrics.hits_at_5 += 1;
        }
        if rank_at_30.is_some() {
            metrics.hits_at_30 += 1;
        }
        if let Some(rank) = first_expected_rank(&paths, &query.expected_paths, 10) {
            metrics.reciprocal_rank_sum += 1.0 / rank as f64;
            metrics.normalized_discounted_gain_sum += 1.0 / ((rank + 1) as f64).log2();
        }
    }

    let p95_ms = metrics.p95_ms();
    eprintln!(
        "RAG v2 broker eval: Recall@5={:.3} Recall@30={:.3} MRR@10={:.3} nDCG@10={:.3} metadata_matches={} no_answer_fpr={:.3} scope_leaks={} warm_p95_ms={p95_ms:.1}",
        metrics.recall_at_5(),
        metrics.recall_at_30(),
        metrics.mrr_at_10(),
        metrics.ndcg_at_10(),
        metrics.metadata_match_queries,
        metrics.no_answer_false_positive_rate(),
        metrics.scope_leaks,
    );

    assert!(metrics.recall_at_5() >= POSITIVE_RECALL_AT_5_MIN);
    assert!(metrics.recall_at_30() >= POSITIVE_RECALL_AT_30_MIN);
    assert!(metrics.ndcg_at_10() >= NDCG_AT_10_MIN);
    assert!(metrics.metadata_match_queries >= METADATA_MATCH_QUERY_MIN);
    assert!(metrics.no_answer_false_positive_rate() <= NO_ANSWER_FALSE_POSITIVE_RATE_MAX);
    assert_eq!(metrics.scope_leaks, SCOPE_LEAK_COUNT_MAX);
    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("docs")
        .join("eval")
        .join("results")
        .join("v1.2.5-hybrid.json");
    let baseline: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&baseline_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", baseline_path.display())),
    )
    .unwrap_or_else(|error| panic!("parse {}: {error}", baseline_path.display()));
    let labels = std::fs::read_to_string(fixture_root().join("labels.json"))
        .expect("read fixture labels for baseline verification");
    let label_hash = crate_content_hash(&labels);
    assert_eq!(
        baseline["fixture"]["labelsSha256"].as_str(),
        Some(label_hash.as_str()),
        "historical baseline must be tied to this exact label set"
    );
    let baseline_mrr = baseline["metrics"]["mrrAt10"]
        .as_f64()
        .expect("baseline mrrAt10");
    let baseline_ndcg = baseline["metrics"]["ndcgAt10"]
        .as_f64()
        .expect("baseline ndcgAt10");
    assert!(
        metrics.mrr_at_10() >= baseline_mrr + 0.05,
        "MRR@10 must improve by at least 0.05 over v1.2.5 ({baseline_mrr:.3})"
    );
    assert!(
        metrics.ndcg_at_10() >= baseline_ndcg + 0.05,
        "nDCG@10 must improve by at least 0.05 over v1.2.5 ({baseline_ndcg:.3})"
    );
}

/// This is intentionally strict: an E2E retrieval result is not valid evidence
/// until it carries an original-source span and content hash.  FTS, metadata,
/// graph, exact-regulation, vector and runtime packet constructors must all
/// uphold this contract before the release gate can turn green.
#[test]
fn rag_v2_every_returned_packet_has_a_valid_source_span_and_hash() {
    let fixture = load_fixture();
    let conn = Connection::open_in_memory().expect("open in-memory database");
    migrate_up(&conn).expect("migrate database");
    index_vault_incremental(&conn, &fixture_root())
        .expect("index fixture vault without embeddings");

    let mut violations = Vec::new();
    for query in fixture
        .queries
        .iter()
        .filter(|query| !query.expected_paths.is_empty())
    {
        let outcome = hybrid_retrieve_with_diagnostics(&conn, &request_for(query))
            .unwrap_or_else(|error| panic!("broker failed for {}: {error}", query.id));
        for packet in &outcome.packets {
            if !packet_has_valid_citation(packet) {
                violations.push(format!(
                    "{} -> {} ({})",
                    query.id,
                    packet.source_path.as_deref().unwrap_or("<no-path>"),
                    packet.retrieval_reason
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "invalid ContextPacket citations: {violations:?}"
    );
}
