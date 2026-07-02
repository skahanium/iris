//! Session-scoped AI evidence ledger.
//!
//! This module stores citation metadata only. It must not persist local note excerpts,
//! web page excerpts, fetched page bodies, or snapshots.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

/// Source class for a session evidence ledger row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEvidenceSourceType {
    Local,
    Web,
}

impl SessionEvidenceSourceType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Web => "web",
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "web" => Self::Web,
            _ => Self::Local,
        }
    }
}

/// Metadata used to register one evidence item in a session ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvidenceRegisterPacket {
    pub source_type: SessionEvidenceSourceType,
    pub title: String,
    pub source_path: Option<String>,
    pub source_span_start: Option<i64>,
    pub source_span_end: Option<i64>,
    pub heading_path: Option<String>,
    pub content_hash: Option<String>,
    pub retrieval_reason: Option<String>,
    pub score: Option<f64>,
    pub confidence: Option<String>,
    pub url: Option<String>,
    pub normalized_url: Option<String>,
    pub domain: Option<String>,
    pub retrieved_at: Option<String>,
    pub search_backend: Option<String>,
    pub source_rank: Option<i64>,
    pub failure_reason: Option<String>,
    pub provider_id: Option<String>,
    pub provider_kind: Option<String>,
    pub raw_result_hash: Option<String>,
    pub extraction_method: Option<String>,
    pub conflict_group: Option<String>,
    pub conflict_note: Option<String>,
}

impl SessionEvidenceRegisterPacket {
    pub(crate) fn from_context_packet(packet: &crate::ai_runtime::ContextPacket) -> Self {
        let is_web = matches!(packet.source_type, crate::ai_runtime::SourceType::Web)
            || packet.web.is_some();
        let web = packet.web.as_ref();
        let url = web
            .and_then(|meta| meta.url.clone())
            .or_else(|| is_web.then(|| packet.source_path.clone()).flatten());
        let normalized_url = url.as_ref().map(|value| value.trim().to_ascii_lowercase());
        Self {
            source_type: if is_web {
                SessionEvidenceSourceType::Web
            } else {
                SessionEvidenceSourceType::Local
            },
            title: packet.title.clone(),
            source_path: if is_web {
                None
            } else {
                packet.source_path.clone()
            },
            source_span_start: packet.source_span.as_ref().map(|span| span.start as i64),
            source_span_end: packet.source_span.as_ref().map(|span| span.end as i64),
            heading_path: packet.heading_path.clone(),
            content_hash: if is_web {
                None
            } else {
                Some(packet.content_hash.clone())
            },
            retrieval_reason: Some(packet.retrieval_reason.clone()),
            score: Some(packet.score),
            confidence: Some(format!("{:?}", packet.trust_level)),
            url,
            normalized_url,
            domain: web.and_then(|meta| meta.domain.clone()),
            retrieved_at: web.map(|meta| meta.fetched_at.clone()),
            search_backend: web.map(|meta| format!("{:?}", meta.search_backend)),
            source_rank: web.map(|meta| match meta.source_rank {
                crate::ai_runtime::WebSourceRank::Official => 1,
                crate::ai_runtime::WebSourceRank::Academic => 2,
                crate::ai_runtime::WebSourceRank::Media => 3,
                crate::ai_runtime::WebSourceRank::Community => 4,
                crate::ai_runtime::WebSourceRank::Unknown => 5,
            }),
            failure_reason: web.and_then(|meta| meta.failure_reason.clone()),
            provider_id: web.and_then(|meta| meta.provider_id.clone()),
            provider_kind: web.and_then(|meta| meta.provider_kind.clone()),
            raw_result_hash: web.and_then(|meta| meta.raw_result_hash.clone()),
            extraction_method: web.and_then(|meta| meta.extraction_method.clone()),
            conflict_group: web.and_then(|meta| meta.conflict_group.clone()),
            conflict_note: web.and_then(|meta| meta.conflict_note.clone()),
        }
    }
}

/// Stored session evidence metadata returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvidenceItem {
    pub id: i64,
    pub session_id: i64,
    pub citation_index: i64,
    pub citation_label: String,
    pub packet_key: String,
    pub message_seq_first: i64,
    pub source_type: SessionEvidenceSourceType,
    pub title: String,
    pub source_path: Option<String>,
    pub source_span_start: Option<i64>,
    pub source_span_end: Option<i64>,
    pub heading_path: Option<String>,
    pub content_hash: Option<String>,
    pub retrieval_reason: Option<String>,
    pub score: Option<f64>,
    pub confidence: Option<String>,
    pub url: Option<String>,
    pub normalized_url: Option<String>,
    pub domain: Option<String>,
    pub retrieved_at: Option<String>,
    pub search_backend: Option<String>,
    pub source_rank: Option<i64>,
    pub failure_reason: Option<String>,
    pub provider_id: Option<String>,
    pub provider_kind: Option<String>,
    pub raw_result_hash: Option<String>,
    pub extraction_method: Option<String>,
    pub conflict_group: Option<String>,
    pub conflict_note: Option<String>,
    pub retired_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_excerpt: Option<String>,
}

/// Safe evidence metadata returned to the ordinary read-only detail view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvidenceDetailItem {
    pub id: i64,
    pub session_id: i64,
    pub citation_index: i64,
    pub citation_label: String,
    pub source_type: SessionEvidenceSourceType,
    pub title: String,
    pub source_path: Option<String>,
    pub heading_path: Option<String>,
    pub retrieval_reason: Option<String>,
    pub url: Option<String>,
    pub normalized_url: Option<String>,
    pub domain: Option<String>,
    pub failure_reason: Option<String>,
    pub conflict_group: Option<String>,
    pub conflict_note: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_excerpt: Option<String>,
}

impl From<SessionEvidenceItem> for SessionEvidenceDetailItem {
    fn from(item: SessionEvidenceItem) -> Self {
        Self {
            id: item.id,
            session_id: item.session_id,
            citation_index: item.citation_index,
            citation_label: item.citation_label,
            source_type: item.source_type,
            title: item.title,
            source_path: item.source_path,
            heading_path: item.heading_path,
            retrieval_reason: item.retrieval_reason,
            url: item.url,
            normalized_url: item.normalized_url,
            domain: item.domain,
            failure_reason: item.failure_reason,
            conflict_group: item.conflict_group,
            conflict_note: item.conflict_note,
            created_at: item.created_at,
            detail_status: item.detail_status,
            live_excerpt: item.live_excerpt,
        }
    }
}

pub(crate) fn register_packets_from_context_packets(
    packets: &[crate::ai_runtime::ContextPacket],
) -> Vec<SessionEvidenceRegisterPacket> {
    packets
        .iter()
        .map(SessionEvidenceRegisterPacket::from_context_packet)
        .collect()
}

pub(crate) fn register_session_evidence(
    conn: &Connection,
    session_id: i64,
    message_seq: i64,
    packets: &[SessionEvidenceRegisterPacket],
) -> AppResult<Vec<SessionEvidenceItem>> {
    let mut registered = Vec::with_capacity(packets.len());
    for packet in packets {
        let packet_key = packet_key_for_register_packet(packet);
        if let Some(existing) = find_by_packet_key(conn, session_id, &packet_key)? {
            if existing.retired_at.is_some() {
                conn.execute(
                    "UPDATE session_evidence SET retired_at = NULL WHERE id = ?1",
                    [existing.id],
                )?;
                registered.push(find_by_id(conn, existing.id)?);
            } else {
                registered.push(existing);
            }
            continue;
        }

        let next_index: i64 = conn.query_row(
            "SELECT COALESCE(MAX(citation_index), 0) + 1 FROM session_evidence WHERE session_id = ?1",
            [session_id],
            |row| row.get(0),
        )?;
        let citation_label = format!("[C{next_index}]");
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO session_evidence
             (session_id, citation_index, citation_label, packet_key, message_seq_first,
              source_type, title, source_path, source_span_start, source_span_end,
              heading_path, content_hash, retrieval_reason, score, confidence,
              url, normalized_url, domain, retrieved_at, search_backend, source_rank,
              failure_reason, provider_id, provider_kind, raw_result_hash, extraction_method,
              conflict_group, conflict_note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     ?6, ?7, ?8, ?9, ?10,
                     ?11, ?12, ?13, ?14, ?15,
                     ?16, ?17, ?18, ?19, ?20, ?21,
                     ?22, ?23, ?24, ?25, ?26,
                     ?27, ?28, ?29)",
            params![
                session_id,
                next_index,
                citation_label,
                packet_key,
                message_seq,
                packet.source_type.as_str(),
                packet.title,
                packet.source_path,
                packet.source_span_start,
                packet.source_span_end,
                packet.heading_path,
                packet.content_hash,
                packet.retrieval_reason,
                packet.score,
                packet.confidence,
                packet.url,
                packet.normalized_url,
                packet.domain,
                packet.retrieved_at,
                packet.search_backend,
                packet.source_rank,
                packet.failure_reason,
                packet.provider_id,
                packet.provider_kind,
                packet.raw_result_hash,
                packet.extraction_method,
                packet.conflict_group,
                packet.conflict_note,
                now,
            ],
        )?;
        registered.push(find_by_id(conn, conn.last_insert_rowid())?);
    }
    Ok(registered)
}

pub(crate) fn list_session_evidence(
    conn: &Connection,
    session_id: i64,
) -> AppResult<Vec<SessionEvidenceItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, citation_index, citation_label, packet_key, message_seq_first,
                source_type, title, source_path, source_span_start, source_span_end,
                heading_path, content_hash, retrieval_reason, score, confidence,
                url, normalized_url, domain, retrieved_at, search_backend, source_rank,
                failure_reason, provider_id, provider_kind, raw_result_hash, extraction_method,
                conflict_group, conflict_note, retired_at, created_at
         FROM session_evidence
         WHERE session_id = ?1 AND retired_at IS NULL
         ORDER BY citation_index ASC",
    )?;
    let rows = stmt.query_map([session_id], row_to_item)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

pub(crate) fn enrich_session_evidence_details(
    mut items: Vec<SessionEvidenceItem>,
    vault_path: &Path,
) -> Vec<SessionEvidenceItem> {
    for item in &mut items {
        if item.source_type != SessionEvidenceSourceType::Local {
            item.detail_status = Some("external_metadata_only".to_string());
            continue;
        }
        enrich_local_evidence_detail(item, vault_path);
    }
    items
}

fn enrich_local_evidence_detail(item: &mut SessionEvidenceItem, vault_path: &Path) {
    let Some(source_path) = item.source_path.as_deref() else {
        item.detail_status = Some("source_missing".to_string());
        return;
    };
    let Ok(resolved) = crate::storage::paths::resolve_vault_path(vault_path, source_path) else {
        item.detail_status = Some("source_missing".to_string());
        return;
    };
    let Ok(content) = std::fs::read_to_string(resolved) else {
        item.detail_status = Some("source_missing".to_string());
        return;
    };
    item.live_excerpt = span_excerpt(&content, item.source_span_start, item.source_span_end);
    let hash_matches = item
        .content_hash
        .as_deref()
        .map(|hash| crate::cas::hash::content_hash_str(&content) == hash)
        .unwrap_or(false);
    item.detail_status = Some(
        if item.live_excerpt.is_none() && item.source_span_start.is_some() {
            "span_missing"
        } else if hash_matches {
            "source_unchanged"
        } else {
            "source_changed"
        }
        .to_string(),
    );
}

fn span_excerpt(content: &str, start: Option<i64>, end: Option<i64>) -> Option<String> {
    let (Some(start), Some(end)) = (start, end) else {
        return None;
    };
    if start < 0 || end < start {
        return None;
    }
    let start = start as usize;
    let end = end as usize;
    if end > content.len() || !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return None;
    }
    Some(content[start..end].to_string())
}

pub(crate) fn retire_evidence_first_introduced_at_or_after(
    conn: &Connection,
    session_id: i64,
    from_seq: i64,
) -> AppResult<usize> {
    let now = chrono::Utc::now().to_rfc3339();
    Ok(conn.execute(
        "UPDATE session_evidence
         SET retired_at = ?1
         WHERE session_id = ?2 AND message_seq_first >= ?3 AND retired_at IS NULL",
        params![now, session_id, from_seq],
    )?)
}

pub(crate) fn update_local_evidence_source_path(
    conn: &Connection,
    old_path: &str,
    new_path: &str,
) -> AppResult<usize> {
    let old = old_path.trim_end_matches('/');
    let new = new_path.trim_end_matches('/');
    let old_child_prefix = format!("{old}/");
    let new_child_prefix = format!("{new}/");
    let like_pattern = format!("{old_child_prefix}%");
    Ok(conn.execute(
        "UPDATE session_evidence
         SET source_path = CASE
             WHEN source_path = ?1 THEN ?2
             ELSE ?3 || substr(source_path, ?4)
         END
         WHERE source_type = 'local'
           AND retired_at IS NULL
           AND (source_path = ?1 OR source_path LIKE ?5)",
        params![
            old,
            new,
            new_child_prefix,
            (old_child_prefix.len() + 1) as i64,
            like_pattern,
        ],
    )?)
}
pub(crate) fn packet_key_for_register_packet(packet: &SessionEvidenceRegisterPacket) -> String {
    match packet.source_type {
        SessionEvidenceSourceType::Web => packet
            .normalized_url
            .as_deref()
            .or(packet.url.as_deref())
            .map(|url| format!("web:{}", url.trim().to_ascii_lowercase()))
            .unwrap_or_else(|| format!("web:title:{}", packet.title.trim())),
        SessionEvidenceSourceType::Local => {
            let path = packet.source_path.as_deref().unwrap_or("").trim();
            match (
                packet.source_span_start,
                packet.source_span_end,
                packet.content_hash.as_deref(),
                packet.heading_path.as_deref(),
            ) {
                (Some(start), Some(end), Some(hash), _) if !path.is_empty() => {
                    format!("local:{path}:span:{start}-{end}:hash:{hash}")
                }
                (_, _, Some(hash), Some(heading)) if !path.is_empty() => {
                    format!("local:{path}:heading:{heading}:hash:{hash}")
                }
                (_, _, _, Some(heading)) if !path.is_empty() => {
                    format!("local:{path}:heading:{heading}")
                }
                (_, _, Some(hash), _) if !path.is_empty() => format!("local:{path}:hash:{hash}"),
                _ => format!("local:title:{}", packet.title.trim()),
            }
        }
    }
}

fn find_by_packet_key(
    conn: &Connection,
    session_id: i64,
    packet_key: &str,
) -> AppResult<Option<SessionEvidenceItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, citation_index, citation_label, packet_key, message_seq_first,
                source_type, title, source_path, source_span_start, source_span_end,
                heading_path, content_hash, retrieval_reason, score, confidence,
                url, normalized_url, domain, retrieved_at, search_backend, source_rank,
                failure_reason, provider_id, provider_kind, raw_result_hash, extraction_method,
                conflict_group, conflict_note, retired_at, created_at
         FROM session_evidence
         WHERE session_id = ?1 AND packet_key = ?2",
    )?;
    Ok(stmt
        .query_row(params![session_id, packet_key], row_to_item)
        .optional()?)
}

fn find_by_id(conn: &Connection, id: i64) -> AppResult<SessionEvidenceItem> {
    conn.query_row(
        "SELECT id, session_id, citation_index, citation_label, packet_key, message_seq_first,
                source_type, title, source_path, source_span_start, source_span_end,
                heading_path, content_hash, retrieval_reason, score, confidence,
                url, normalized_url, domain, retrieved_at, search_backend, source_rank,
                failure_reason, provider_id, provider_kind, raw_result_hash, extraction_method,
                conflict_group, conflict_note, retired_at, created_at
         FROM session_evidence
         WHERE id = ?1",
        [id],
        row_to_item,
    )
    .map_err(Into::into)
}

fn row_to_item(row: &Row<'_>) -> rusqlite::Result<SessionEvidenceItem> {
    let source_type: String = row.get(6)?;
    Ok(SessionEvidenceItem {
        id: row.get(0)?,
        session_id: row.get(1)?,
        citation_index: row.get(2)?,
        citation_label: row.get(3)?,
        packet_key: row.get(4)?,
        message_seq_first: row.get(5)?,
        source_type: SessionEvidenceSourceType::from_db(&source_type),
        title: row.get(7)?,
        source_path: row.get(8)?,
        source_span_start: row.get(9)?,
        source_span_end: row.get(10)?,
        heading_path: row.get(11)?,
        content_hash: row.get(12)?,
        retrieval_reason: row.get(13)?,
        score: row.get(14)?,
        confidence: row.get(15)?,
        url: row.get(16)?,
        normalized_url: row.get(17)?,
        domain: row.get(18)?,
        retrieved_at: row.get(19)?,
        search_backend: row.get(20)?,
        source_rank: row.get(21)?,
        failure_reason: row.get(22)?,
        provider_id: row.get(23)?,
        provider_kind: row.get(24)?,
        raw_result_hash: row.get(25)?,
        extraction_method: row.get(26)?,
        conflict_group: row.get(27)?,
        conflict_note: row.get(28)?,
        retired_at: row.get(29)?,
        created_at: row.get(30)?,
        detail_status: None,
        live_excerpt: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::{session::SessionManager, AiScene};
    use crate::storage::db::Database;

    fn setup_session() -> (Database, i64) {
        let db = Database::open_in_memory().unwrap();
        let session_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
        (db, session_id)
    }

    fn local_packet(path: &str, hash: &str) -> SessionEvidenceRegisterPacket {
        SessionEvidenceRegisterPacket {
            source_type: SessionEvidenceSourceType::Local,
            title: "Local Source".to_string(),
            source_path: Some(path.to_string()),
            source_span_start: Some(10),
            source_span_end: Some(42),
            heading_path: Some("Intro > Detail".to_string()),
            content_hash: Some(hash.to_string()),
            retrieval_reason: Some("semantic_match".to_string()),
            score: Some(0.82),
            confidence: Some("high".to_string()),
            url: None,
            normalized_url: None,
            domain: None,
            retrieved_at: None,
            search_backend: None,
            source_rank: None,
            failure_reason: None,
            provider_id: None,
            provider_kind: None,
            raw_result_hash: None,
            extraction_method: None,
            conflict_group: None,
            conflict_note: None,
        }
    }

    fn web_packet(url: &str) -> SessionEvidenceRegisterPacket {
        SessionEvidenceRegisterPacket {
            source_type: SessionEvidenceSourceType::Web,
            title: "Web Source".to_string(),
            source_path: None,
            source_span_start: None,
            source_span_end: None,
            heading_path: None,
            content_hash: None,
            retrieval_reason: Some("web_search".to_string()),
            score: Some(0.64),
            confidence: Some("medium".to_string()),
            url: Some(url.to_string()),
            normalized_url: Some(url.to_ascii_lowercase()),
            domain: Some("example.com".to_string()),
            retrieved_at: Some("2026-06-22T00:00:00Z".to_string()),
            search_backend: Some("test".to_string()),
            source_rank: Some(1),
            failure_reason: None,
            provider_id: Some("anysearch".to_string()),
            provider_kind: Some("mcp".to_string()),
            raw_result_hash: Some("hash".to_string()),
            extraction_method: Some("search_snippet".to_string()),
            conflict_group: None,
            conflict_note: None,
        }
    }

    #[test]
    fn register_packets_from_context_packets_excludes_excerpt_body() {
        let packet = crate::ai_runtime::ContextPacket {
            id: "p1".to_string(),
            source_type: crate::ai_runtime::SourceType::Note,
            source_path: Some("notes/a.md".to_string()),
            title: "A".to_string(),
            heading_path: Some("Intro".to_string()),
            source_span: Some(crate::ai_runtime::SourceSpan { start: 1, end: 8 }),
            content_hash: "hash-a".to_string(),
            excerpt: "do not persist me".to_string(),
            retrieval_reason: "semantic".to_string(),
            score: 0.7,
            trust_level: crate::ai_runtime::TrustLevel::UserNote,
            citation_label: "[X]".to_string(),
            stale: false,
            web: None,
            corpus: None,
        };

        let packets = register_packets_from_context_packets(&[packet]);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].source_path.as_deref(), Some("notes/a.md"));
        assert_eq!(packets[0].content_hash.as_deref(), Some("hash-a"));
        assert_eq!(packets[0].source_span_start, Some(1));
        assert_eq!(packets[0].source_span_end, Some(8));
    }

    #[test]
    fn registers_session_unique_labels_and_reuses_duplicates() {
        let (db, session_id) = setup_session();

        db.with_conn(|conn| {
            let first = register_session_evidence(
                conn,
                session_id,
                2,
                &[
                    local_packet("notes/a.md", "hash-a"),
                    web_packet("https://example.com/a"),
                ],
            )?;
            assert_eq!(first[0].citation_label, "[C1]");
            assert_eq!(first[1].citation_label, "[C2]");

            let second = register_session_evidence(
                conn,
                session_id,
                4,
                &[
                    local_packet("notes/a.md", "hash-a"),
                    local_packet("notes/b.md", "hash-b"),
                ],
            )?;
            assert_eq!(second[0].citation_label, "[C1]");
            assert_eq!(second[1].citation_label, "[C3]");

            let listed = list_session_evidence(conn, session_id)?;
            let labels = listed
                .into_iter()
                .map(|item| item.citation_label)
                .collect::<Vec<_>>();
            assert_eq!(labels, vec!["[C1]", "[C2]", "[C3]"]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn retired_evidence_is_hidden_without_recycling_labels() {
        let (db, session_id) = setup_session();

        db.with_conn(|conn| {
            register_session_evidence(
                conn,
                session_id,
                2,
                &[local_packet("notes/a.md", "hash-a")],
            )?;
            register_session_evidence(
                conn,
                session_id,
                4,
                &[local_packet("notes/b.md", "hash-b")],
            )?;
            let retired = retire_evidence_first_introduced_at_or_after(conn, session_id, 3)?;
            assert_eq!(retired, 1);

            let next = register_session_evidence(
                conn,
                session_id,
                6,
                &[local_packet("notes/c.md", "hash-c")],
            )?;
            assert_eq!(next[0].citation_label, "[C3]");

            let labels = list_session_evidence(conn, session_id)?
                .into_iter()
                .map(|item| item.citation_label)
                .collect::<Vec<_>>();
            assert_eq!(labels, vec!["[C1]", "[C3]"]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn updates_local_source_paths_for_file_and_folder_rename() {
        let (db, session_id) = setup_session();

        db.with_conn(|conn| {
            register_session_evidence(
                conn,
                session_id,
                2,
                &[
                    local_packet("folder/a.md", "hash-a"),
                    local_packet("folder/nested/b.md", "hash-b"),
                    web_packet("https://example.com/a"),
                ],
            )?;

            let file_updates =
                update_local_evidence_source_path(conn, "folder/a.md", "folder/renamed.md")?;
            assert_eq!(file_updates, 1);

            let folder_updates = update_local_evidence_source_path(conn, "folder", "archive")?;
            assert_eq!(folder_updates, 2);

            let paths = list_session_evidence(conn, session_id)?
                .into_iter()
                .filter_map(|item| item.source_path)
                .collect::<Vec<_>>();
            assert!(paths.contains(&"archive/renamed.md".to_string()));
            assert!(paths.contains(&"archive/nested/b.md".to_string()));
            assert!(!paths.iter().any(|path| path.starts_with("folder/")));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn enriches_local_detail_from_live_vault_file() {
        let (db, session_id) = setup_session();
        let vault = tempfile::tempdir().unwrap();
        let note_dir = vault.path().join("notes");
        std::fs::create_dir_all(&note_dir).unwrap();
        let content = "abcdef";
        std::fs::write(note_dir.join("a.md"), content).unwrap();
        let hash = crate::cas::hash::content_hash_str(content);

        db.with_conn(|conn| {
            register_session_evidence(
                conn,
                session_id,
                2,
                &[SessionEvidenceRegisterPacket {
                    source_span_start: Some(1),
                    source_span_end: Some(4),
                    ..local_packet("notes/a.md", &hash)
                }],
            )?;
            let enriched = enrich_session_evidence_details(
                list_session_evidence(conn, session_id)?,
                vault.path(),
            );
            assert_eq!(
                enriched[0].detail_status.as_deref(),
                Some("source_unchanged")
            );
            assert_eq!(enriched[0].live_excerpt.as_deref(), Some("bcd"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn web_packets_dedupe_by_normalized_url() {
        let (db, session_id) = setup_session();

        db.with_conn(|conn| {
            let first = register_session_evidence(
                conn,
                session_id,
                2,
                &[web_packet("https://example.com/A")],
            )?;
            let second = register_session_evidence(
                conn,
                session_id,
                4,
                &[web_packet("https://example.com/a")],
            )?;
            assert_eq!(first[0].citation_label, "[C1]");
            assert_eq!(second[0].citation_label, "[C1]");
            assert_eq!(list_session_evidence(conn, session_id)?.len(), 1);
            Ok(())
        })
        .unwrap();
    }
}
