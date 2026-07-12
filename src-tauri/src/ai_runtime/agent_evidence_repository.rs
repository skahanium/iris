//! Normal-domain evidence ledger repository for unified Agent Runs.
//!
//! `session_evidence` remains the sole source of truth. Runs, messages and
//! checkpoints retain only stable evidence identifiers; this module neither
//! writes to legacy evidence packets nor reads a current editor document.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ai_runtime::run_contract::{EvidenceRef, EvidenceSourceKind};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const MAX_BOUNDED_WEB_EXCERPT_CHARS: usize = 2_000;
const MAX_METADATA_CHARS: usize = 2_000;

/// Role the explicitly registered material serves for this Run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MaterialRole {
    /// Source constrains a factual or normative claim.
    Authority,
    /// Source supplies style or form only.
    Exemplar,
    /// Source provides supplementary supporting information.
    Reference,
    /// Source was retrieved as a non-authoritative lookup result.
    Lookup,
}

/// Local-vault metadata to register without ever accepting local source text.
#[derive(Debug, Clone)]
pub(crate) struct LocalEvidenceInput {
    /// Normal-domain SQLite session that owns the Run.
    pub(crate) session_id: i64,
    /// Run that first used this source.
    pub(crate) run_id: String,
    /// First message sequence that can cite this evidence.
    pub(crate) message_seq_first: i64,
    /// Explicit purpose of the material for this Run.
    pub(crate) material_role: MaterialRole,
    /// Safe display title.
    pub(crate) title: String,
    /// Vault-relative source path.
    pub(crate) source_path: String,
    /// UTF-8 byte range start in the source at read time.
    pub(crate) source_span_start: i64,
    /// UTF-8 byte range end in the source at read time.
    pub(crate) source_span_end: i64,
    /// Optional source heading hierarchy.
    pub(crate) heading_path: Option<String>,
    /// Hash of the complete source content read at registration time.
    pub(crate) content_hash: String,
    /// Safe explanation of why this source was retrieved.
    pub(crate) retrieval_reason: Option<String>,
    /// Retrieval score, when a retrieval system produced one.
    pub(crate) score: Option<f64>,
}

/// Web metadata and the single bounded excerpt actually supporting a response.
#[derive(Debug, Clone)]
pub(crate) struct WebEvidenceInput {
    /// Normal-domain SQLite session that owns the Run.
    pub(crate) session_id: i64,
    /// Run that first used this source.
    pub(crate) run_id: String,
    /// First message sequence that can cite this evidence.
    pub(crate) message_seq_first: i64,
    /// Explicit purpose of the material for this Run.
    pub(crate) material_role: MaterialRole,
    /// Safe display title.
    pub(crate) title: String,
    /// Source URL as fetched.
    pub(crate) url: String,
    /// Canonical URL used for source identity.
    pub(crate) normalized_url: String,
    /// URL host or provider-supplied domain.
    pub(crate) domain: String,
    /// Fetch timestamp.
    pub(crate) retrieved_at: String,
    /// Web evidence provider identity.
    pub(crate) provider_id: String,
    /// Provider transport or implementation kind.
    pub(crate) provider_kind: String,
    /// Hash of the raw provider result, not its body.
    pub(crate) raw_result_hash: String,
    /// Extraction algorithm used to derive the excerpt.
    pub(crate) extraction_method: String,
    /// Actual answer-supporting excerpt; never a whole page.
    pub(crate) bounded_excerpt: String,
    /// Safe explanation of why this source was retrieved.
    pub(crate) retrieval_reason: Option<String>,
    /// Retrieval score, when a retrieval system produced one.
    pub(crate) score: Option<f64>,
    /// Provider result rank, when available.
    pub(crate) source_rank: Option<i64>,
    /// Optional conflicting-source group.
    pub(crate) conflict_group: Option<String>,
    /// Optional provider failure reason retained as metadata.
    pub(crate) failure_reason: Option<String>,
}

/// Lossless ledger identifier plus the UI-safe reference shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegisteredEvidence {
    /// SQLite evidence-ledger primary key. It remains numeric internally.
    pub(crate) evidence_id: i64,
    /// Cross-process safe reference, whose decimal identifier is lossless.
    pub(crate) reference: EvidenceRef,
}

/// Storage-only normal-domain evidence ledger operations.
pub(crate) struct AgentEvidenceRepository;

impl AgentEvidenceRepository {
    /// Register local source metadata without accepting or persisting note text.
    pub(crate) fn register_local(
        db: &Database,
        input: LocalEvidenceInput,
    ) -> AppResult<RegisteredEvidence> {
        validate_local_input(&input)?;
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                ensure_normal_run_ownership(conn, input.session_id, &input.run_id)?;
                ensure_reference_message(conn, input.session_id, input.message_seq_first)?;
                let packet_key = local_packet_key(&input);
                if let Some(existing) = find_registered(conn, input.session_id, &packet_key)? {
                    return reactivate_and_return(conn, existing);
                }
                let citation = next_citation(conn, input.session_id)?;
                let now = chrono::Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO session_evidence
                     (session_id, citation_index, citation_label, packet_key, message_seq_first,
                      source_type, title, source_path, source_span_start, source_span_end,
                      heading_path, content_hash, retrieval_reason, score, origin_run_id,
                      material_role, stale, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5,
                             'local', ?6, ?7, ?8, ?9,
                             ?10, ?11, ?12, ?13, ?14,
                             ?15, 0, ?16)",
                    params![
                        input.session_id,
                        citation.index,
                        citation.label,
                        packet_key,
                        input.message_seq_first,
                        input.title,
                        input.source_path,
                        input.source_span_start,
                        input.source_span_end,
                        input.heading_path,
                        input.content_hash,
                        input.retrieval_reason,
                        input.score,
                        input.run_id,
                        material_role_wire(input.material_role),
                        now,
                    ],
                )?;
                registered_by_id(conn, conn.last_insert_rowid())
            })
        })
    }

    /// Register one answer-supporting Web excerpt, rejecting page-sized input.
    pub(crate) fn register_web(
        db: &Database,
        input: WebEvidenceInput,
    ) -> AppResult<RegisteredEvidence> {
        validate_web_input(&input)?;
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                ensure_normal_run_ownership(conn, input.session_id, &input.run_id)?;
                ensure_reference_message(conn, input.session_id, input.message_seq_first)?;
                let packet_key = web_packet_key(&input);
                if let Some(existing) = find_registered(conn, input.session_id, &packet_key)? {
                    return reactivate_and_return(conn, existing);
                }
                let citation = next_citation(conn, input.session_id)?;
                let now = chrono::Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO session_evidence
                     (session_id, citation_index, citation_label, packet_key, message_seq_first,
                      source_type, title, retrieval_reason, score, url, normalized_url, domain,
                      retrieved_at, source_rank, failure_reason, provider_id, provider_kind,
                      raw_result_hash, extraction_method, conflict_group, origin_run_id,
                      material_role, stale, bounded_excerpt, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5,
                             'web', ?6, ?7, ?8, ?9, ?10, ?11,
                             ?12, ?13, ?14, ?15, ?16,
                             ?17, ?18, ?19, ?20,
                             ?21, 0, ?22, ?23)",
                    params![
                        input.session_id,
                        citation.index,
                        citation.label,
                        packet_key,
                        input.message_seq_first,
                        input.title,
                        input.retrieval_reason,
                        input.score,
                        input.url,
                        input.normalized_url,
                        input.domain,
                        input.retrieved_at,
                        input.source_rank,
                        input.failure_reason,
                        input.provider_id,
                        input.provider_kind,
                        input.raw_result_hash,
                        input.extraction_method,
                        input.conflict_group,
                        input.run_id,
                        material_role_wire(input.material_role),
                        input.bounded_excerpt,
                        now,
                    ],
                )?;
                registered_by_id(conn, conn.last_insert_rowid())
            })
        })
    }
}

#[derive(Debug)]
struct Citation {
    index: i64,
    label: String,
}

#[derive(Debug)]
struct ExistingEvidence {
    id: i64,
    retired_at: Option<String>,
}

fn in_immediate_transaction<T>(
    conn: &Connection,
    operation: impl FnOnce(&Connection) -> AppResult<T>,
) -> AppResult<T> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match operation(conn) {
        Ok(value) => match conn.execute_batch("COMMIT") {
            Ok(()) => Ok(value),
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(error.into())
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn ensure_normal_run_ownership(conn: &Connection, session_id: i64, run_id: &str) -> AppResult<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM agent_runs
         WHERE run_id = ?1 AND session_id = ?2 AND security_domain = 'normal'",
        params![run_id, session_id],
        |row| row.get(0),
    )?;
    if count == 1 {
        Ok(())
    } else {
        Err(AppError::msg("agent_evidence_run_not_found"))
    }
}

fn ensure_reference_message(
    conn: &Connection,
    session_id: i64,
    message_seq_first: i64,
) -> AppResult<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_messages WHERE session_id = ?1 AND seq = ?2",
        params![session_id, message_seq_first],
        |row| row.get(0),
    )?;
    if count == 1 {
        Ok(())
    } else {
        Err(AppError::msg("agent_evidence_message_not_found"))
    }
}

fn next_citation(conn: &Connection, session_id: i64) -> AppResult<Citation> {
    let index: i64 = conn.query_row(
        "SELECT COALESCE(MAX(citation_index), 0) + 1
         FROM session_evidence WHERE session_id = ?1",
        [session_id],
        |row| row.get(0),
    )?;
    Ok(Citation {
        index,
        label: format!("[C{index}]"),
    })
}

fn find_registered(
    conn: &Connection,
    session_id: i64,
    packet_key: &str,
) -> AppResult<Option<ExistingEvidence>> {
    conn.query_row(
        "SELECT id, retired_at FROM session_evidence
         WHERE session_id = ?1 AND packet_key = ?2",
        params![session_id, packet_key],
        |row| {
            Ok(ExistingEvidence {
                id: row.get(0)?,
                retired_at: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn reactivate_and_return(
    conn: &Connection,
    existing: ExistingEvidence,
) -> AppResult<RegisteredEvidence> {
    if existing.retired_at.is_some() {
        conn.execute(
            "UPDATE session_evidence SET retired_at = NULL WHERE id = ?1",
            [existing.id],
        )?;
    }
    registered_by_id(conn, existing.id)
}

fn registered_by_id(conn: &Connection, evidence_id: i64) -> AppResult<RegisteredEvidence> {
    conn.query_row(
        "SELECT id, citation_label, source_type, title, stale
         FROM session_evidence WHERE id = ?1",
        [evidence_id],
        |row| {
            let source_type: String = row.get(2)?;
            let source_kind = match source_type.as_str() {
                "local" => EvidenceSourceKind::Local,
                "web" => EvidenceSourceKind::Web,
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            Ok(RegisteredEvidence {
                evidence_id: row.get(0)?,
                reference: EvidenceRef {
                    evidence_id: evidence_id.to_string(),
                    source_kind,
                    title: Some(row.get(3)?),
                    display_label: row.get(1)?,
                    stale: row.get::<_, i64>(4)? != 0,
                },
            })
        },
    )
    .map_err(Into::into)
}

fn local_packet_key(input: &LocalEvidenceInput) -> String {
    format!(
        "local:{}:span:{}-{}:hash:{}",
        input.source_path.trim(),
        input.source_span_start,
        input.source_span_end,
        input.content_hash.trim()
    )
}

fn web_packet_key(input: &WebEvidenceInput) -> String {
    let digest = Sha256::digest(input.bounded_excerpt.as_bytes());
    format!(
        "web:{}:excerpt:{}",
        input.normalized_url.trim().to_ascii_lowercase(),
        hex::encode(&digest[..8])
    )
}

fn material_role_wire(role: MaterialRole) -> &'static str {
    match role {
        MaterialRole::Authority => "authority",
        MaterialRole::Exemplar => "exemplar",
        MaterialRole::Reference => "reference",
        MaterialRole::Lookup => "lookup",
    }
}

fn validate_local_input(input: &LocalEvidenceInput) -> AppResult<()> {
    validate_common(
        input.session_id,
        &input.run_id,
        input.message_seq_first,
        &input.title,
    )?;
    if input.source_path.trim().is_empty()
        || input.content_hash.trim().is_empty()
        || input.source_span_start < 0
        || input.source_span_end < input.source_span_start
    {
        return Err(AppError::msg("agent_evidence_invalid_local_metadata"));
    }
    for value in [&input.source_path, &input.content_hash] {
        if value.chars().count() > MAX_METADATA_CHARS {
            return Err(AppError::msg("agent_evidence_invalid_local_metadata"));
        }
    }
    validate_optional_metadata(&input.heading_path)?;
    validate_optional_metadata(&input.retrieval_reason)
}

fn validate_web_input(input: &WebEvidenceInput) -> AppResult<()> {
    validate_common(
        input.session_id,
        &input.run_id,
        input.message_seq_first,
        &input.title,
    )?;
    for value in [
        &input.url,
        &input.normalized_url,
        &input.domain,
        &input.retrieved_at,
        &input.provider_id,
        &input.provider_kind,
        &input.raw_result_hash,
        &input.extraction_method,
    ] {
        if value.trim().is_empty() || value.chars().count() > MAX_METADATA_CHARS {
            return Err(AppError::msg("agent_evidence_invalid_web_metadata"));
        }
    }
    if !input.url.starts_with("https://") || !input.normalized_url.starts_with("https://") {
        return Err(AppError::msg("agent_evidence_invalid_web_metadata"));
    }
    let excerpt_length = input.bounded_excerpt.chars().count();
    if excerpt_length == 0 {
        return Err(AppError::msg("agent_evidence_empty_excerpt"));
    }
    if excerpt_length > MAX_BOUNDED_WEB_EXCERPT_CHARS {
        return Err(AppError::msg("agent_evidence_excerpt_too_large"));
    }
    validate_optional_metadata(&input.retrieval_reason)?;
    validate_optional_metadata(&input.conflict_group)?;
    validate_optional_metadata(&input.failure_reason)
}

fn validate_common(
    session_id: i64,
    run_id: &str,
    message_seq_first: i64,
    title: &str,
) -> AppResult<()> {
    if session_id <= 0 || message_seq_first <= 0 || run_id.trim().is_empty() {
        return Err(AppError::msg("agent_evidence_invalid_ownership"));
    }
    if title.trim().is_empty() || title.chars().count() > MAX_METADATA_CHARS {
        return Err(AppError::msg("agent_evidence_invalid_metadata"));
    }
    Ok(())
}

fn validate_optional_metadata(value: &Option<String>) -> AppResult<()> {
    if value
        .as_deref()
        .is_some_and(|value| value.chars().count() > MAX_METADATA_CHARS)
    {
        Err(AppError::msg("agent_evidence_invalid_metadata"))
    } else {
        Ok(())
    }
}
