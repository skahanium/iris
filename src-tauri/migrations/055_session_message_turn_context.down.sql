-- Rebuild the table so rollback works on SQLite versions without DROP COLUMN support.
DROP INDEX IF EXISTS idx_session_messages_session;
DROP INDEX IF EXISTS idx_session_messages_vault_id;

CREATE TABLE session_messages__without_turn_context (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id               INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq                      INTEGER NOT NULL,
    role                     TEXT NOT NULL,
    content                  TEXT NOT NULL,
    content_parts            TEXT,
    tool_calls               JSON,
    turn_id                  TEXT,
    explicit_references_json TEXT,
    evidence_refs_json       TEXT,
    citation_map_json        TEXT,
    content_hash             TEXT,
    vault_id                 TEXT,
    created_at               TEXT NOT NULL,
    UNIQUE(session_id, seq)
);

INSERT INTO session_messages__without_turn_context (
    id, session_id, seq, role, content, content_parts, tool_calls, turn_id,
    explicit_references_json, evidence_refs_json, citation_map_json,
    content_hash, vault_id, created_at
)
SELECT id, session_id, seq, role, content, content_parts, tool_calls, turn_id,
       explicit_references_json, evidence_refs_json, citation_map_json,
       content_hash, vault_id, created_at
FROM session_messages;

DROP TABLE session_messages;
ALTER TABLE session_messages__without_turn_context RENAME TO session_messages;

CREATE INDEX idx_session_messages_session ON session_messages(session_id, seq);
CREATE INDEX idx_session_messages_vault_id ON session_messages(vault_id);
