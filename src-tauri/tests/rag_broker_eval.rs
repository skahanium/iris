//! End-to-end quality gates for the current hybrid retrieval broker.
//!
//! The fixture itself is frozen at v1.2.6. Its metadata binds that historic
//! label set to this current evaluation without presenting it as a v1.2.18
//! fixture revision.
//!
//! The suite indexes a real fixture vault then invokes the public broker.  It
//! deliberately disables vectors, so the default CI path never downloads a
//! model.  Vector quality belongs to a separately provisioned model gate.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use iris_lib::ai_runtime::retrieval_broker::{
    hybrid_retrieve_with_diagnostics, RetrievalLayerDiagnostic, RetrievalLayerStatus,
    RetrievalLayers, RetrievalRequest,
};
use iris_lib::ai_runtime::retrieval_scope::RetrievalScope;
use iris_lib::embedding::engine::{
    embed_texts_batch, f32_to_bytes, set_embedding_runtime_enabled, EMBEDDING_DIMENSION,
    EMBEDDING_MODEL_FINGERPRINT, EMBEDDING_MODEL_ID,
};
use iris_lib::indexer::scan::index_vault_incremental;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;
use serde::Deserialize;

const FIXTURE_VERSION: &str = "v1.2.6";
const FIXTURE_STATUS: &str = "historical_frozen";
const CURRENT_EVALUATION_VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));
const SEMANTIC_ONLY_RECALL_AT_5_MIN: f64 = 0.80;
const SEMANTIC_ONLY_RECALL_AT_30_MIN: f64 = 0.95;
const HYBRID_ANY_SOURCE_RECALL_AT_5_MIN: f64 = 0.95;
const HYBRID_ANY_SOURCE_RECALL_AT_30_MIN: f64 = 0.98;
const ALL_REQUIRED_SOURCE_RECALL_AT_5_MIN: f64 = 0.90;
const ALL_REQUIRED_SOURCE_RECALL_AT_30_MIN: f64 = 0.95;
const NO_ANSWER_FALSE_POSITIVE_RATE_MAX: f64 = 0.10;
const NDCG_AT_10_MIN: f64 = 0.85;
const METADATA_MATCH_QUERY_MIN: usize = 10;
const SCOPE_LEAK_COUNT_MAX: usize = 0;
const WARM_KNN_P95_MS_MAX: f64 = 750.0;
const END_TO_END_RETRIEVAL_P95_MS_MAX: f64 = 1_000.0;

fn vector_scale_fixture_sizes() -> [i64; 4] {
    [1_000, 10_000, 25_000, 50_000]
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvalFixture {
    version: String,
    notes: Vec<FixtureNote>,
    queries: Vec<EvalQuery>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureMetadata {
    fixture_version: String,
    fixture_status: String,
    current_evaluation_version: String,
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
    any_source_hits_at_5: usize,
    any_source_hits_at_30: usize,
    all_required_hits_at_5: usize,
    all_required_hits_at_30: usize,
    reciprocal_rank_sum: f64,
    normalized_discounted_gain_sum: f64,
    metadata_match_queries: usize,
    no_answer_queries: usize,
    no_answer_false_positives: usize,
    scope_leaks: usize,

    retrieval_latencies_ms: Vec<f64>,
}

#[derive(Debug, Default)]
struct VectorPerformanceMetrics {
    warm_knn_latencies_ms: Vec<f64>,
    end_to_end_latencies_ms: Vec<f64>,
}

#[test]
fn vector_quality_gate_fails_when_all_required_recall_at_30_is_below_095() {
    let metrics = BrokerMetrics {
        positive_queries: 50,
        any_source_hits_at_5: 50,
        any_source_hits_at_30: 50,
        all_required_hits_at_5: 50,
        all_required_hits_at_30: 47,
        reciprocal_rank_sum: 50.0,
        normalized_discounted_gain_sum: 50.0,
        metadata_match_queries: 0,
        no_answer_queries: 10,
        no_answer_false_positives: 0,
        scope_leaks: 0,
        retrieval_latencies_ms: vec![1.0],
    };

    assert!(!meets_vector_release_gates(&metrics));
}

#[test]
fn vector_performance_gate_fails_when_warm_knn_p95_exceeds_750_ms() {
    let metrics = VectorPerformanceMetrics {
        warm_knn_latencies_ms: vec![740.0, 751.0],
        end_to_end_latencies_ms: vec![100.0, 100.0],
    };

    assert!(!meets_vector_performance_release_gates(&metrics));
}

#[test]
fn scale_fixture_ladder_includes_the_50k_release_boundary() {
    assert_eq!(
        vector_scale_fixture_sizes(),
        [1_000, 10_000, 25_000, 50_000]
    );
}

fn metadata_match_increment(diagnostics: &[RetrievalLayerDiagnostic]) -> usize {
    usize::from(diagnostics.iter().any(|diagnostic| {
        diagnostic.layer == "metadata" && diagnostic.status == RetrievalLayerStatus::Ok
    }))
}

#[test]
fn metadata_match_query_is_counted_once_when_diagnostics_repeat() {
    let diagnostics = vec![
        RetrievalLayerDiagnostic {
            layer: "metadata".to_string(),
            status: RetrievalLayerStatus::Ok,
            message: None,
            backend: None,
            model_id: None,
            generation_id: None,
        },
        RetrievalLayerDiagnostic {
            layer: "metadata".to_string(),
            status: RetrievalLayerStatus::Ok,
            message: None,
            backend: None,
            model_id: None,
            generation_id: None,
        },
    ];

    assert_eq!(metadata_match_increment(&diagnostics), 1);
}

impl BrokerMetrics {
    fn any_source_recall_at_5(&self) -> f64 {
        ratio(self.any_source_hits_at_5, self.positive_queries)
    }

    fn any_source_recall_at_30(&self) -> f64 {
        ratio(self.any_source_hits_at_30, self.positive_queries)
    }

    fn all_required_source_recall_at_5(&self) -> f64 {
        ratio(self.all_required_hits_at_5, self.positive_queries)
    }

    fn all_required_source_recall_at_30(&self) -> f64 {
        ratio(self.all_required_hits_at_30, self.positive_queries)
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

    fn p95_ms(&self) -> f64 {
        percentile_ms(&self.retrieval_latencies_ms, 0.95)
    }
}

fn percentile_ms(samples: &[f64], percentile: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = ((sorted.len() - 1) as f64 * percentile).ceil() as usize;
    sorted[index]
}

fn meets_semantic_release_gates(metrics: &BrokerMetrics) -> bool {
    metrics.any_source_recall_at_5() >= SEMANTIC_ONLY_RECALL_AT_5_MIN
        && metrics.any_source_recall_at_30() >= SEMANTIC_ONLY_RECALL_AT_30_MIN
        && metrics.ndcg_at_10() >= NDCG_AT_10_MIN
        && metrics.no_answer_false_positive_rate() <= NO_ANSWER_FALSE_POSITIVE_RATE_MAX
        && metrics.scope_leaks == SCOPE_LEAK_COUNT_MAX
}

fn meets_vector_release_gates(metrics: &BrokerMetrics) -> bool {
    metrics.any_source_recall_at_5() >= HYBRID_ANY_SOURCE_RECALL_AT_5_MIN
        && metrics.any_source_recall_at_30() >= HYBRID_ANY_SOURCE_RECALL_AT_30_MIN
        && metrics.all_required_source_recall_at_5() >= ALL_REQUIRED_SOURCE_RECALL_AT_5_MIN
        && metrics.all_required_source_recall_at_30() >= ALL_REQUIRED_SOURCE_RECALL_AT_30_MIN
        && metrics.ndcg_at_10() >= NDCG_AT_10_MIN
        && metrics.no_answer_false_positive_rate() <= NO_ANSWER_FALSE_POSITIVE_RATE_MAX
        && metrics.scope_leaks == SCOPE_LEAK_COUNT_MAX
}

fn meets_vector_performance_release_gates(metrics: &VectorPerformanceMetrics) -> bool {
    percentile_ms(&metrics.warm_knn_latencies_ms, 0.95) <= WARM_KNN_P95_MS_MAX
        && percentile_ms(&metrics.end_to_end_latencies_ms, 0.95) <= END_TO_END_RETRIEVAL_P95_MS_MAX
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

fn load_fixture_metadata() -> FixtureMetadata {
    let metadata = fixture_root().join("fixture-metadata.json");
    assert!(
        metadata.is_file(),
        "missing frozen fixture metadata at {}",
        metadata.display()
    );
    let content = std::fs::read_to_string(&metadata)
        .unwrap_or_else(|error| panic!("read {}: {error}", metadata.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|error| panic!("parse {}: {error}", metadata.display()))
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

fn vector_request_for(query: &EvalQuery) -> RetrievalRequest {
    let mut request = request_for(query);
    request.layers.vector = true;
    request
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

fn all_expected_paths_within(paths: &[String], expected: &[String], max_results: usize) -> bool {
    !expected.is_empty()
        && expected.iter().all(|required| {
            paths
                .iter()
                .take(max_results)
                .any(|candidate| candidate == required)
        })
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

fn fixture_labels_hash() -> String {
    let labels = std::fs::read_to_string(fixture_root().join("labels.json"))
        .expect("read fixture labels for result metadata");
    crate_content_hash(&labels)
}

fn emit_result_metadata(gate: &str, metrics: &BrokerMetrics) {
    let revision = option_env!("GITHUB_SHA")
        .or(option_env!("VERGEN_GIT_SHA"))
        .unwrap_or("workspace");
    eprintln!(
        "RAG evaluation result: {}",
        serde_json::json!({
            "gate": gate,
            "revision": revision,
            "model": EMBEDDING_MODEL_FINGERPRINT,
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            "fixtureLabelsSha256": fixture_labels_hash(),
            "rawMetrics": {
                "positiveQueries": metrics.positive_queries,
                "noAnswerQueries": metrics.no_answer_queries,
                "anySourceRecallAt5": metrics.any_source_recall_at_5(),
                "anySourceRecallAt30": metrics.any_source_recall_at_30(),
                "allRequiredSourceRecallAt5": metrics.all_required_source_recall_at_5(),
                "allRequiredSourceRecallAt30": metrics.all_required_source_recall_at_30(),
                "ndcgAt10": metrics.ndcg_at_10(),
                "noAnswerFalsePositiveRate": metrics.no_answer_false_positive_rate(),
                "scopeLeaks": metrics.scope_leaks,
                "endToEndP95Milliseconds": metrics.p95_ms(),
            }
        })
    );
}

fn populate_fixture_chunk_embeddings(conn: &Connection) {
    let mut statement = conn
        .prepare(
            "SELECT c.id, c.content, COALESCE(c.content_hash, '')
             FROM chunks c
             ORDER BY c.id",
        )
        .expect("prepare fixture chunks");
    let chunks: Vec<(i64, String, String)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("query fixture chunks")
        .collect::<Result<_, _>>()
        .expect("collect fixture chunks");
    assert!(
        !chunks.is_empty(),
        "fixture must contain chunks for vector evaluation"
    );

    for batch in chunks.chunks(16) {
        let texts: Vec<&str> = batch
            .iter()
            .map(|(_, content, _)| content.as_str())
            .collect();
        let vectors = embed_texts_batch(&texts).expect("embed provisioned vector fixture batch");
        assert_eq!(vectors.len(), batch.len());
        for ((chunk_id, _, fingerprint), vector) in batch.iter().zip(vectors) {
            conn.execute(
                "INSERT INTO chunk_embeddings_v2
                     (chunk_id, embedding, source_fingerprint, model_id, dimension)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    chunk_id,
                    f32_to_bytes(&vector),
                    fingerprint,
                    EMBEDDING_MODEL_ID,
                    EMBEDDING_DIMENSION as i64,
                ],
            )
            .expect("insert provisioned fixture embedding");
        }
    }

    let indexed = i64::try_from(chunks.len()).expect("fixture chunk count fits i64");
    conn.execute(
        "UPDATE embedding_generation_state
         SET active_model_id = ?1,
             target_model_id = ?1,
             target_dimension = ?2,
             phase = 'ready',
             indexed_items = ?3,
             total_items = ?3
         WHERE singleton = 1",
        rusqlite::params![EMBEDDING_MODEL_ID, EMBEDDING_DIMENSION as i64, indexed],
    )
    .expect("mark provisioned vector generation ready");
}

#[test]
fn rag_v2_fixture_contract_has_48_notes_and_60_labeled_queries() {
    let fixture = load_fixture();
    let metadata = load_fixture_metadata();
    assert_eq!(fixture.version, FIXTURE_VERSION);
    assert_eq!(metadata.fixture_version, FIXTURE_VERSION);
    assert_eq!(metadata.fixture_status, FIXTURE_STATUS);
    assert_eq!(
        metadata.current_evaluation_version,
        CURRENT_EVALUATION_VERSION
    );
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
    assert_eq!(
        fixture
            .queries
            .iter()
            .filter(|query| !query.expected_paths.is_empty())
            .count(),
        50,
        "fixture has 50 answerable queries"
    );
    assert_eq!(
        fixture
            .queries
            .iter()
            .filter(|query| query.expected_paths.is_empty())
            .count(),
        10,
        "fixture has 10 no-answer queries"
    );
    assert_eq!(
        fixture
            .queries
            .iter()
            .filter(|query| query.expected_paths.len() > 1)
            .count(),
        10,
        "ten link queries require two independent sources"
    );

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
fn all_required_source_recall_requires_every_labeled_path_within_the_cutoff() {
    let ranked = vec![
        "notes/first.md".to_string(),
        "notes/distractor.md".to_string(),
        "notes/second.md".to_string(),
    ];
    let required = vec!["notes/first.md".to_string(), "notes/second.md".to_string()];

    assert!(!all_expected_paths_within(&ranked, &required, 2));
    assert!(all_expected_paths_within(&ranked, &required, 3));
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
        metrics.metadata_match_queries += metadata_match_increment(&outcome.diagnostics);
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
            metrics.any_source_hits_at_5 += 1;
        }
        if rank_at_30.is_some() {
            metrics.any_source_hits_at_30 += 1;
        }
        if all_expected_paths_within(&paths, &query.expected_paths, 5) {
            metrics.all_required_hits_at_5 += 1;
        }
        if all_expected_paths_within(&paths, &query.expected_paths, 30) {
            metrics.all_required_hits_at_30 += 1;
        }
        if let Some(rank) = first_expected_rank(&paths, &query.expected_paths, 10) {
            metrics.reciprocal_rank_sum += 1.0 / rank as f64;
            metrics.normalized_discounted_gain_sum += 1.0 / ((rank + 1) as f64).log2();
        }
    }

    let p95_ms = metrics.p95_ms();
    eprintln!(
        "RAG v2 broker eval: any_source_recall@5={:.3} any_source_recall@30={:.3} all_required_source_recall@5={:.3} all_required_source_recall@30={:.3} MRR@10={:.3} nDCG@10={:.3} metadata_matches={} no_answer_fpr={:.3} scope_leaks={} warm_p95_ms={p95_ms:.1}",
        metrics.any_source_recall_at_5(),
        metrics.any_source_recall_at_30(),
        metrics.all_required_source_recall_at_5(),
        metrics.all_required_source_recall_at_30(),
        metrics.mrr_at_10(),
        metrics.ndcg_at_10(),
        metrics.metadata_match_queries,
        metrics.no_answer_false_positive_rate(),
        metrics.scope_leaks,
    );

    assert!(
        meets_semantic_release_gates(&metrics),
        "deterministic semantic-only release gates failed"
    );
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
    emit_result_metadata("deterministic-semantic", &metrics);
}

/// Release-only vector gate. It is deliberately ignored in the normal model-free
/// suite; packaging workflows invoke it explicitly after restoring the verified
/// BGE model. A missing/invalid model fails this test rather than downgrading to
/// FTS or claiming a vector-quality result.
#[test]
#[ignore = "requires the verified bundled BGE model and sqlite-vec"]
fn rag_v2_provisioned_sqlite_vec_model_meets_release_quality_gates() {
    set_embedding_runtime_enabled(true);
    let fixture = load_fixture();
    let database = iris_lib::storage::db::Database::open_in_memory()
        .expect("open provisioned sqlite-vec evaluation database");
    database
        .with_conn(|conn| {
            index_vault_incremental(conn, &fixture_root())?;
            populate_fixture_chunk_embeddings(conn);

            let mut metrics = BrokerMetrics::default();
            for query in &fixture.queries {
                let start = Instant::now();
                let outcome = hybrid_retrieve_with_diagnostics(conn, &vector_request_for(query))?;
                metrics
                    .retrieval_latencies_ms
                    .push(start.elapsed().as_secs_f64() * 1_000.0);

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
                    metrics.no_answer_false_positives += usize::from(!paths.is_empty());
                    continue;
                }

                metrics.positive_queries += 1;
                if first_expected_rank(&paths, &query.expected_paths, 5).is_some() {
                    metrics.any_source_hits_at_5 += 1;
                }
                if first_expected_rank(&paths, &query.expected_paths, 30).is_some() {
                    metrics.any_source_hits_at_30 += 1;
                }
                if all_expected_paths_within(&paths, &query.expected_paths, 5) {
                    metrics.all_required_hits_at_5 += 1;
                }
                if all_expected_paths_within(&paths, &query.expected_paths, 30) {
                    metrics.all_required_hits_at_30 += 1;
                }
                if let Some(rank) = first_expected_rank(&paths, &query.expected_paths, 10) {
                    metrics.reciprocal_rank_sum += 1.0 / rank as f64;
                    metrics.normalized_discounted_gain_sum += 1.0 / ((rank + 1) as f64).log2();
                }
            }

            emit_result_metadata("provisioned-sqlite-vec", &metrics);
            assert!(
                meets_vector_release_gates(&metrics),
                "provisioned sqlite-vec vector-quality release gates failed"
            );
            assert!(
                metrics.p95_ms() <= END_TO_END_RETRIEVAL_P95_MS_MAX,
                "provisioned end-to-end p95 {}ms exceeds {}ms",
                metrics.p95_ms(),
                END_TO_END_RETRIEVAL_P95_MS_MAX
            );
            Ok(())
        })
        .expect("run provisioned sqlite-vec evaluation");
}

/// The scale ladder is an explicitly invoked CI quality evaluation, not a
/// desktop package build. Fixtures live in fresh temporary directories and the
/// reference-machine label is mandatory so a random developer workstation can
/// never be mistaken for a release measurement.
#[cfg(feature = "sqlite-vec")]
#[test]
#[ignore = "requires IRIS_RAG_PERFORMANCE_REFERENCE and is run by the scale-ladder workflow"]
fn sqlite_vec_50k_scale_fixture_meets_warm_knn_release_gate() {
    let reference_machine = std::env::var("IRIS_RAG_PERFORMANCE_REFERENCE")
        .expect("IRIS_RAG_PERFORMANCE_REFERENCE is required for a release performance result");
    let mut all_knn_samples = Vec::new();
    let mut all_retrieval_samples = Vec::new();

    for scale in vector_scale_fixture_sizes() {
        let fixture = tempfile::tempdir().expect("create temporary synthetic scale fixture");
        let fixture_manifest = fixture.path().join("fixture.json");
        std::fs::write(
            &fixture_manifest,
            serde_json::json!({
                "kind": "synthetic-sqlite-vec-scale-fixture",
                "records": scale,
                "containsUserVaultData": false,
            })
            .to_string(),
        )
        .expect("write synthetic fixture manifest");

        let database = iris_lib::storage::db::Database::open_in_memory()
            .expect("open sqlite-vec scale database");
        database
            .with_conn(|conn| {
                let mut query_vector = vec![0.0_f32; EMBEDDING_DIMENSION];
                query_vector[0] = 1.0;
                let mut distractor_vector = vec![0.0_f32; EMBEDDING_DIMENSION];
                distractor_vector[1] = 1.0;

                conn.execute_batch("BEGIN")?;
                conn.execute(
                    "WITH RECURSIVE ids(id) AS (
                         VALUES(1) UNION ALL SELECT id + 1 FROM ids WHERE id < ?1
                     )
                     INSERT INTO files
                         (id, path, title, content_hash, word_count, created_at, updated_at)
                     SELECT id, printf('scale/%d.md', id), printf('Scale %d', id),
                            printf('file-hash-%d', id), 1,
                            '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
                     FROM ids",
                    [scale],
                )?;
                conn.execute(
                    "WITH RECURSIVE ids(id) AS (
                         VALUES(1) UNION ALL SELECT id + 1 FROM ids WHERE id < ?1
                     )
                     INSERT INTO chunks
                         (id, file_id, chunk_index, content, source_start, source_end, content_hash, char_count)
                     SELECT id, id, 0, CASE WHEN id = ?1 THEN 'needle' ELSE 'bulk' END,
                            0, 6, printf('chunk-hash-%d', id), 6
                     FROM ids",
                    [scale],
                )?;
                conn.execute(
                    "INSERT INTO chunk_embeddings_v2
                         (chunk_id, embedding, source_fingerprint, model_id, dimension)
                     SELECT id, ?1, content_hash, ?2, ?3
                     FROM chunks WHERE id < ?4",
                    rusqlite::params![
                        f32_to_bytes(&distractor_vector),
                        EMBEDDING_MODEL_ID,
                        EMBEDDING_DIMENSION as i64,
                        scale,
                    ],
                )?;
                conn.execute(
                    "INSERT INTO chunk_embeddings_v2
                         (chunk_id, embedding, source_fingerprint, model_id, dimension)
                     SELECT id, ?1, content_hash, ?2, ?3
                     FROM chunks WHERE id = ?4",
                    rusqlite::params![
                        f32_to_bytes(&query_vector),
                        EMBEDDING_MODEL_ID,
                        EMBEDDING_DIMENSION as i64,
                        scale,
                    ],
                )?;
                conn.execute_batch("COMMIT")?;

                let query_bytes = f32_to_bytes(&query_vector);
                let mut warm_samples = Vec::new();
                let mut retrieval_samples = Vec::new();
                for _ in 0..20 {
                    let started = Instant::now();
                    let mut statement = conn.prepare(
                        "SELECT chunk_id FROM vec_chunks_v3
                         WHERE embedding MATCH ?1 AND k = 32",
                    )?;
                    let ids: Vec<i64> = statement
                        .query_map([&query_bytes], |row| row.get(0))?
                        .collect::<Result<_, _>>()?;
                    warm_samples.push(started.elapsed().as_secs_f64() * 1_000.0);

                    let retrieval_started = Instant::now();
                    let needle_found: bool = conn.query_row(
                        "WITH nearest AS (
                             SELECT chunk_id FROM vec_chunks_v3
                             WHERE embedding MATCH ?1 AND k = 32
                         )
                         SELECT EXISTS(
                             SELECT 1 FROM nearest
                             JOIN chunks ON chunks.id = nearest.chunk_id
                             JOIN files ON files.id = chunks.file_id
                             WHERE files.path = ?2
                         )",
                        rusqlite::params![query_bytes, format!("scale/{scale}.md")],
                        |row| row.get(0),
                    )?;
                    retrieval_samples
                        .push(retrieval_started.elapsed().as_secs_f64() * 1_000.0);
                    assert!(needle_found, "KNN missed scale-{scale} synthetic needle");
                    assert!(!ids.is_empty(), "KNN returned no candidates at scale {scale}");
                }
                all_knn_samples.extend(warm_samples);
                all_retrieval_samples.extend(retrieval_samples);
                Ok(())
            })
            .expect("run temporary sqlite-vec scale fixture");
    }

    let metrics = VectorPerformanceMetrics {
        warm_knn_latencies_ms: all_knn_samples,
        end_to_end_latencies_ms: all_retrieval_samples,
    };
    eprintln!(
        "RAG vector scale result: {}",
        serde_json::json!({
            "revision": option_env!("GITHUB_SHA").unwrap_or("workspace"),
            "model": EMBEDDING_MODEL_FINGERPRINT,
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            "fixture": "temporary synthetic 1k/10k/25k/50k sqlite-vec fixtures",
            "referenceMachine": reference_machine,
            "rawMetrics": {
                "warmKnnP95Milliseconds": percentile_ms(&metrics.warm_knn_latencies_ms, 0.95),
                "endToEndRetrievalP95Milliseconds": percentile_ms(&metrics.end_to_end_latencies_ms, 0.95),
                "warmKnnSamplesMilliseconds": metrics.warm_knn_latencies_ms,
                "endToEndSamplesMilliseconds": metrics.end_to_end_latencies_ms,
            }
        })
    );
    assert!(
        meets_vector_performance_release_gates(&metrics),
        "sqlite-vec performance release gate failed"
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
