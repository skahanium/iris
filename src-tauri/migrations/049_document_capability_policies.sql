-- Persisted six-capability document policy matrix for unified Agent Runs.
-- Stores only scope/capability decisions; never note content, prompts, credentials, or session grants.
CREATE TABLE document_capability_policies (
    id INTEGER PRIMARY KEY,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('vault', 'folder', 'document')),
    scope_path TEXT NOT NULL DEFAULT '',
    capability TEXT NOT NULL CHECK (capability IN (
        'discover', 'read', 'send_to_model', 'cite', 'propose_change', 'apply_change'
    )),
    decision TEXT NOT NULL CHECK (decision IN ('allow', 'deny')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (
        (scope_kind = 'vault' AND scope_path = '')
        OR (scope_kind IN ('folder', 'document') AND length(trim(scope_path)) > 0)
    ),
    UNIQUE (scope_kind, scope_path, capability)
);

CREATE INDEX idx_document_capability_policies_scope
    ON document_capability_policies(scope_kind, scope_path);