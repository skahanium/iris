-- Phase 5 Markdown Agent permission grants and audit.
-- Stores only permission decisions and safe scope summaries. Never store note
-- body, clipboard body, screenshot content, shell output, tokens, or API keys.

CREATE TABLE IF NOT EXISTS agent_permission_grants (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    permission_name TEXT NOT NULL,
    decision TEXT NOT NULL,
    scope_kind TEXT NOT NULL,
    scope_value TEXT,
    risk_level TEXT NOT NULL,
    skill_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_permission_grants_permission
    ON agent_permission_grants(permission_name);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_permission_grants_unique_scope
    ON agent_permission_grants(
        permission_name,
        scope_kind,
        COALESCE(scope_value, ''),
        COALESCE(skill_id, '')
    );

CREATE INDEX IF NOT EXISTS idx_agent_permission_grants_scope
    ON agent_permission_grants(scope_kind, scope_value);

CREATE INDEX IF NOT EXISTS idx_agent_permission_grants_skill
    ON agent_permission_grants(skill_id);

CREATE TABLE IF NOT EXISTS agent_permission_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    skill_id TEXT,
    tool_name TEXT NOT NULL,
    permission_name TEXT NOT NULL,
    decision TEXT NOT NULL,
    scope_summary TEXT NOT NULL,
    risk_level TEXT NOT NULL,
    result_status TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (request_id) REFERENCES ai_traces(request_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_permission_audit_request
    ON agent_permission_audit(request_id);

CREATE INDEX IF NOT EXISTS idx_agent_permission_audit_permission
    ON agent_permission_audit(permission_name);

CREATE INDEX IF NOT EXISTS idx_agent_permission_audit_tool
    ON agent_permission_audit(tool_name);
