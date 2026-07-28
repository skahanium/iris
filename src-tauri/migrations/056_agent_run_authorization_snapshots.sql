CREATE TABLE IF NOT EXISTS agent_run_authorizations (
    run_id                     TEXT PRIMARY KEY REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    allowed_capabilities_json  TEXT NOT NULL,
    authorization_hash         TEXT NOT NULL,
    created_at                 TEXT NOT NULL
);
