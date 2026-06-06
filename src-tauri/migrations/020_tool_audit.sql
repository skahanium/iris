-- Tool audit log: records every tool call with sanitized arguments/results.
-- Sensitive info (API keys, full note content, tokens) is NEVER stored.
CREATE TABLE IF NOT EXISTS tool_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    harness_round INTEGER NOT NULL,
    tool_name TEXT NOT NULL,
    arguments_summary TEXT,
    result_summary TEXT,
    success INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    scene TEXT,
    subagent_depth INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (request_id) REFERENCES ai_traces(request_id)
);

CREATE INDEX IF NOT EXISTS idx_tool_audit_request_id
    ON tool_audit(request_id);

CREATE INDEX IF NOT EXISTS idx_tool_audit_tool_name
    ON tool_audit(tool_name);
