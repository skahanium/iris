-- C26: append-only boundary events and outbound structure witnesses.
-- This is diagnostic evidence, not a second copy of Run lifecycle state.
CREATE TABLE IF NOT EXISTS audit_boundary_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    input_revision TEXT NOT NULL,
    parent_run_id TEXT,
    child_run_id TEXT,
    model_turn INTEGER NOT NULL,
    call_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    tool_surface_version TEXT NOT NULL,
    protocol_adapter TEXT NOT NULL,
    layer TEXT NOT NULL,
    event_kind TEXT NOT NULL,
    record_completeness TEXT NOT NULL,
    module_id TEXT NOT NULL,
    component_id TEXT NOT NULL,
    tool_instance TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_audit_boundary_events_run
    ON audit_boundary_events(run_id, id);
CREATE INDEX IF NOT EXISTS idx_audit_boundary_events_attempt
    ON audit_boundary_events(run_id, attempt_id, event_kind);
