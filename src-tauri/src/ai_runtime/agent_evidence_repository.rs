//! Normal-domain evidence ledger repository for unified Agent Runs.
//!
//! `session_evidence` remains the sole source of truth. Runs, messages and
//! checkpoints retain only stable evidence identifiers; this module neither
//! writes to legacy evidence packets nor reads a current editor document.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

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

/// Bounded text or JSON returned by one explicitly granted MCP read tool.
#[derive(Debug, Clone)]
pub(crate) struct ExternalToolEvidenceInput {
    pub(crate) session_id: i64,
    pub(crate) run_id: String,
    pub(crate) message_seq_first: i64,
    pub(crate) title: String,
    pub(crate) provider_id: String,
    pub(crate) provider_config_hash: String,
    pub(crate) binding_id: String,
    pub(crate) raw_result_hash: String,
    pub(crate) retrieved_at: String,
    pub(crate) bounded_excerpt: String,
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
    /// Build the source allow-list for one structured final submission from exact Run
    /// registrations. Web labels are Run-local `W1..Wn` projections, never
    /// session-global citation numbers or evidence rows from an older Run.
    pub(crate) fn provenance_policy(
        db: &Database,
        run_id: &str,
        strict_web: bool,
    ) -> AppResult<crate::ai_runtime::provenance::ProvenancePolicy> {
        db.with_read_conn(|conn| {
            let explicit_references_json: String = conn.query_row(
                "SELECT message.explicit_references_json
                 FROM agent_runs run
                 JOIN session_messages message
                   ON message.session_id = run.session_id AND message.turn_id = run.turn_id
                 WHERE run.run_id = ?1",
                [run_id],
                |row| row.get(0),
            )?;
            let authorized_material_count = serde_json::from_str::<Vec<serde_json::Value>>(
                &explicit_references_json,
            )
            .map(|references| references.len())
            .unwrap_or_default();
            let conversation_history_available: bool = conn.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM agent_runs prior_run
                    JOIN session_messages prior_message
                      ON prior_message.session_id = prior_run.session_id
                     AND prior_message.turn_id = prior_run.turn_id
                    WHERE prior_run.session_id = (
                        SELECT session_id FROM agent_runs WHERE run_id = ?1
                    )
                      AND prior_run.run_id != ?1
                      AND prior_message.role = 'assistant'
                )",
                [run_id],
                |row| row.get(0),
            )?;
            let mut local = BTreeSet::new();
            let mut external = BTreeSet::new();
            let web_count: i64 = conn.query_row(
                "SELECT COUNT(*)
                 FROM agent_run_evidence run_evidence
                 JOIN session_evidence evidence ON evidence.id = run_evidence.evidence_id
                 WHERE run_evidence.run_id = ?1
                   AND run_evidence.registration_source = 'web_search'
                   AND evidence.source_type = 'web'
                   AND evidence.retired_at IS NULL
                   AND evidence.url LIKE 'https://%'",
                [run_id],
                |row| row.get(0),
            )?;
            let mut statement = conn.prepare(
                "SELECT run_evidence.evidence_id, run_evidence.registration_source, evidence.source_type
                 FROM agent_run_evidence run_evidence
                 JOIN session_evidence evidence ON evidence.id = run_evidence.evidence_id
                 WHERE run_evidence.run_id = ?1 AND evidence.retired_at IS NULL",
            )?;
            let rows = statement
                .query_map([run_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (evidence_id, registration_source, source_type) in rows {
                if registration_source == "context" && source_type == "local" {
                    local.insert(evidence_id);
                }
                if registration_source == "external_tool" {
                    external.insert(evidence_id);
                }
            }
            let web = (1..=web_count).collect::<BTreeSet<_>>();
            Ok(crate::ai_runtime::provenance::ProvenancePolicy {
                current_user_available: true,
                conversation_history_available,
                runtime_fact_available: false,
                authorized_material_count,
                current_run_local_evidence_ids: local,
                current_run_web_evidence_ids: web,
                current_run_external_evidence_ids: external,
                strict_web,
            })
        })
    }

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
                let registered =
                    if let Some(existing) = find_registered(conn, input.session_id, &packet_key)? {
                        reactivate_and_return(conn, existing)?
                    } else {
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
                        registered_by_id(conn, conn.last_insert_rowid())?
                    };
                record_run_evidence_use(conn, &input.run_id, registered.evidence_id, "context")?;
                Ok(registered)
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
                let registered =
                    if let Some(existing) = find_registered(conn, input.session_id, &packet_key)? {
                        reactivate_and_return(conn, existing)?
                    } else {
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
                        registered_by_id(conn, conn.last_insert_rowid())?
                    };
                record_run_evidence_use(conn, &input.run_id, registered.evidence_id, "web_search")?;
                Ok(registered)
            })
        })
    }

    /// Register the exact bounded external-tool excerpt used by this Run.
    pub(crate) fn register_external_tool(
        db: &Database,
        input: ExternalToolEvidenceInput,
    ) -> AppResult<RegisteredEvidence> {
        validate_external_tool_input(&input)?;
        db.with_conn(|conn| {
            in_immediate_transaction(conn, |conn| {
                ensure_normal_run_ownership(conn, input.session_id, &input.run_id)?;
                ensure_reference_message(conn, input.session_id, input.message_seq_first)?;
                let packet_key = format!(
                    "external:{}:{}:{}:{}:{}:{}",
                    input.run_id.trim(),
                    input.provider_id.trim(),
                    input.binding_id.trim(),
                    input.provider_config_hash.trim(),
                    input.raw_result_hash.trim(),
                    crate::cas::hash::content_hash_str(&format!(
                        "{}:{}",
                        input.retrieved_at.trim(),
                        uuid::Uuid::new_v4()
                    ))
                );
                let citation = next_citation(conn, input.session_id)?;
                let now = chrono::Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO session_evidence
                             (session_id, citation_index, citation_label, packet_key,
                              message_seq_first, source_type, title, content_hash,
                              retrieval_reason, retrieved_at, provider_id, provider_kind,
                              raw_result_hash, extraction_method, origin_run_id,
                              material_role, stale, bounded_excerpt, created_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, 'web', ?6, ?7,
                                     'external.read', ?8, ?9, 'mcp', ?10,
                                     'mcp_tool_output_v1', ?11, 'lookup', 0, ?12, ?13)",
                    params![
                        input.session_id,
                        citation.index,
                        citation.label,
                        packet_key,
                        input.message_seq_first,
                        input.title,
                        input.raw_result_hash,
                        input.retrieved_at,
                        input.provider_id,
                        input.raw_result_hash,
                        input.run_id,
                        input.bounded_excerpt,
                        now,
                    ],
                )?;
                let registered = registered_by_id(conn, conn.last_insert_rowid())?;
                record_run_evidence_use(
                    conn,
                    &input.run_id,
                    registered.evidence_id,
                    "external_tool",
                )?;
                Ok(registered)
            })
        })
    }

    /// Load HTTPS web citation metadata for final-answer footnote linkification.
    pub(crate) fn list_web_citation_links(
        db: &Database,
        evidence_ids: &[i64],
    ) -> AppResult<Vec<crate::ai_runtime::citation_linkify::WebCitationLink>> {
        if evidence_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut unique = evidence_ids.to_vec();
        unique.sort_unstable();
        unique.dedup();
        db.with_conn(|conn| {
            let mut out = Vec::new();
            for evidence_id in unique {
                let row = conn
                    .query_row(
                        "SELECT citation_index, citation_label, title, url
                         FROM session_evidence
                         WHERE id = ?1
                           AND source_type = 'web'
                           AND retired_at IS NULL
                           AND url IS NOT NULL
                           AND url LIKE 'https://%'",
                        [evidence_id],
                        |row| {
                            Ok(crate::ai_runtime::citation_linkify::WebCitationLink {
                                index: row.get(0)?,
                                label: row.get(1)?,
                                title: row.get(2)?,
                                url: row.get(3)?,
                            })
                        },
                    )
                    .optional()?;
                if let Some(cite) = row {
                    out.push(cite);
                }
            }
            out.sort_by_key(|cite| cite.index);
            Ok(out)
        })
    }

    /// Load the exact Web evidence selected by one Run, using stable Run-local
    /// labels rather than session-global citation numbering. A follow-up Run
    /// may reuse a session evidence row, but it always receives a new W1..Wn
    /// projection for its own answer contract.
    pub(crate) fn list_current_run_web_citation_links(
        db: &Database,
        run_id: &str,
    ) -> AppResult<Vec<crate::ai_runtime::citation_linkify::WebCitationLink>> {
        db.with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT evidence.title, evidence.url
                 FROM agent_run_evidence run_evidence
                 JOIN session_evidence evidence ON evidence.id = run_evidence.evidence_id
                 WHERE run_evidence.run_id = ?1
                   AND run_evidence.registration_source = 'web_search'
                   AND evidence.source_type = 'web'
                   AND evidence.retired_at IS NULL
                   AND evidence.url LIKE 'https://%'
                 ORDER BY run_evidence.registered_at ASC, evidence.id ASC",
            )?;
            let rows = statement
                .query_map([run_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows
                .into_iter()
                .enumerate()
                .map(|(offset, (title, url))| {
                    let index = i64::try_from(offset + 1).unwrap_or(i64::MAX);
                    crate::ai_runtime::citation_linkify::WebCitationLink {
                        index,
                        label: format!("[W{index}]"),
                        title,
                        url,
                    }
                })
                .collect())
        })
    }

    /// Check that the exact Run, rather than merely its session, successfully
    /// registered HTTPS Web evidence through the `web_search` capability.
    pub(crate) fn has_current_run_web_evidence(
        db: &Database,
        run_id: &str,
        evidence_ids: &[i64],
    ) -> AppResult<bool> {
        if evidence_ids.is_empty() {
            return Ok(false);
        }
        db.with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT EXISTS(
                     SELECT 1
                     FROM agent_run_evidence run_evidence
                     JOIN session_evidence evidence ON evidence.id = run_evidence.evidence_id
                     WHERE run_evidence.run_id = ?1
                       AND run_evidence.registration_source = 'web_search'
                       AND evidence.source_type = 'web'
                       AND evidence.retired_at IS NULL
                       AND evidence.url LIKE 'https://%'
                 )",
            )?;
            statement
                .query_row([run_id], |row| row.get(0))
                .map_err(Into::into)
        })
    }

    /// Check that at least one supplied evidence row was registered through an
    /// explicitly granted MCP read tool by this exact Run.
    pub(crate) fn has_current_run_external_evidence(
        db: &Database,
        run_id: &str,
        evidence_ids: &[i64],
    ) -> AppResult<bool> {
        if evidence_ids.is_empty() {
            return Ok(false);
        }
        let mut evidence_ids = evidence_ids.to_vec();
        evidence_ids.sort_unstable();
        evidence_ids.dedup();
        db.with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT EXISTS(
                     SELECT 1
                     FROM agent_run_evidence run_evidence
                     JOIN session_evidence evidence ON evidence.id = run_evidence.evidence_id
                     WHERE run_evidence.run_id = ?1
                       AND run_evidence.evidence_id = ?2
                       AND run_evidence.registration_source = 'external_tool'
                       AND evidence.origin_run_id = ?1
                       AND evidence.provider_kind = 'mcp'
                       AND evidence.extraction_method = 'mcp_tool_output_v1'
                       AND evidence.retired_at IS NULL
                 )",
            )?;
            for evidence_id in evidence_ids {
                if statement.query_row(params![run_id, evidence_id], |row| row.get(0))? {
                    return Ok(true);
                }
            }
            Ok(false)
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

fn record_run_evidence_use(
    conn: &Connection,
    run_id: &str,
    evidence_id: i64,
    registration_source: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO agent_run_evidence (run_id, evidence_id, registration_source, registered_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(run_id, evidence_id, registration_source) DO NOTHING",
        params![
            run_id,
            evidence_id,
            registration_source,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
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

fn validate_external_tool_input(input: &ExternalToolEvidenceInput) -> AppResult<()> {
    validate_common(
        input.session_id,
        &input.run_id,
        input.message_seq_first,
        &input.title,
    )?;
    for value in [
        &input.provider_id,
        &input.provider_config_hash,
        &input.binding_id,
        &input.raw_result_hash,
        &input.retrieved_at,
    ] {
        if value.trim().is_empty() || value.chars().count() > MAX_METADATA_CHARS {
            return Err(AppError::msg("agent_evidence_invalid_external_metadata"));
        }
    }
    let excerpt_length = input.bounded_excerpt.chars().count();
    if excerpt_length == 0 {
        return Err(AppError::msg("agent_evidence_empty_excerpt"));
    }
    if excerpt_length > MAX_BOUNDED_WEB_EXCERPT_CHARS {
        return Err(AppError::msg("agent_evidence_excerpt_too_large"));
    }
    Ok(())
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
