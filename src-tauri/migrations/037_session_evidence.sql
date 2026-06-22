-- 037: Session-scoped AI evidence ledger.
-- Stores citation metadata only. Local note text and web page bodies/excerpts are intentionally not persisted here.
CREATE TABLE IF NOT EXISTS session_evidence (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id        INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    citation_index    INTEGER NOT NULL,
    citation_label    TEXT NOT NULL,
    packet_key        TEXT NOT NULL,
    message_seq_first INTEGER NOT NULL,
    source_type       TEXT NOT NULL CHECK (source_type IN ('local', 'web')),
    title             TEXT NOT NULL DEFAULT '',
    source_path       TEXT,
    source_span_start INTEGER,
    source_span_end   INTEGER,
    heading_path      TEXT,
    content_hash      TEXT,
    retrieval_reason  TEXT,
    score             REAL,
    confidence        TEXT,
    url               TEXT,
    normalized_url    TEXT,
    domain            TEXT,
    retrieved_at      TEXT,
    search_backend    TEXT,
    source_rank       INTEGER,
    failure_reason    TEXT,
    retired_at        TEXT,
    created_at        TEXT NOT NULL,
    UNIQUE(session_id, citation_index),
    UNIQUE(session_id, citation_label),
    UNIQUE(session_id, packet_key)
);

CREATE INDEX IF NOT EXISTS idx_session_evidence_session
    ON session_evidence(session_id, citation_index);
CREATE INDEX IF NOT EXISTS idx_session_evidence_source_path
    ON session_evidence(source_path)
    WHERE source_path IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_session_evidence_normalized_url
    ON session_evidence(normalized_url)
    WHERE normalized_url IS NOT NULL;