-- 047: Unified Agent Run persistence foundation.
--
-- This incremental migration is deliberately additive: the legacy execution
-- path remains the only writer until the single-cutover phase. New runtime
-- code writes only these Run tables once connected; it never dual-writes the
-- legacy task/trace tables.

CREATE TABLE IF NOT EXISTS agent_runs (
    run_id                       TEXT PRIMARY KEY,
    client_request_id            TEXT NOT NULL UNIQUE,
    session_id                   INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_id                      TEXT NOT NULL,
    status                       TEXT NOT NULL,
    state_version                INTEGER NOT NULL DEFAULT 0,
    effect                       TEXT NOT NULL,
    effort                       TEXT NOT NULL,
    security_domain              TEXT NOT NULL,
    risk                         TEXT NOT NULL,
    envelope_json                TEXT NOT NULL DEFAULT '{}',
    goal_summary                 TEXT NOT NULL DEFAULT '',
    budget_policy_json           TEXT NOT NULL DEFAULT '{}',
    provider_route_summary_json  TEXT NOT NULL DEFAULT '{}',
    stage_metrics_json           TEXT NOT NULL DEFAULT '{}',
    token_input                  INTEGER NOT NULL DEFAULT 0,
    token_output                 INTEGER NOT NULL DEFAULT 0,
    error_code                   TEXT,
    safe_error_message           TEXT,
    created_at                   TEXT NOT NULL,
    updated_at                   TEXT NOT NULL,
    completed_at                 TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_runs_session
    ON agent_runs(session_id, created_at);
CREATE INDEX IF NOT EXISTS idx_agent_runs_status
    ON agent_runs(status, updated_at);

CREATE TABLE IF NOT EXISTS agent_run_steps (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id              TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    step_seq            INTEGER NOT NULL,
    kind                TEXT NOT NULL,
    status              TEXT NOT NULL,
    input_summary       TEXT NOT NULL DEFAULT '',
    output_summary      TEXT NOT NULL DEFAULT '',
    resume_state_json   TEXT NOT NULL DEFAULT '{}',
    evidence_refs_json  TEXT NOT NULL DEFAULT '[]',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    UNIQUE(run_id, step_seq)
);

CREATE INDEX IF NOT EXISTS idx_agent_run_steps_run
    ON agent_run_steps(run_id, step_seq);

CREATE TABLE IF NOT EXISTS agent_run_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id          TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    event_seq       INTEGER NOT NULL,
    state_version   INTEGER NOT NULL,
    event_type      TEXT NOT NULL,
    payload_json    TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    UNIQUE(run_id, event_seq)
);

CREATE INDEX IF NOT EXISTS idx_agent_run_events_run
    ON agent_run_events(run_id, event_seq);

ALTER TABLE session_messages ADD COLUMN turn_id TEXT;
ALTER TABLE session_messages ADD COLUMN explicit_references_json TEXT;
ALTER TABLE session_messages ADD COLUMN evidence_refs_json TEXT;
ALTER TABLE session_messages ADD COLUMN citation_map_json TEXT;

ALTER TABLE session_evidence ADD COLUMN origin_run_id TEXT;
ALTER TABLE session_evidence ADD COLUMN material_role TEXT;
ALTER TABLE session_evidence ADD COLUMN stale INTEGER NOT NULL DEFAULT 0;
ALTER TABLE session_evidence ADD COLUMN bounded_excerpt TEXT;

CREATE INDEX IF NOT EXISTS idx_session_evidence_origin_run
    ON session_evidence(origin_run_id)
    WHERE origin_run_id IS NOT NULL;
