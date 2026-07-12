-- 048: Frozen confirmation facts for unified Agent Runs.
CREATE TABLE IF NOT EXISTS agent_run_confirmations (
    confirmation_id  TEXT PRIMARY KEY,
    run_id           TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    plan_hash        TEXT NOT NULL,
    plan_json        TEXT NOT NULL,
    expires_at       INTEGER NOT NULL,
    status           TEXT NOT NULL DEFAULT 'pending',
    created_at       TEXT NOT NULL,
    consumed_at      TEXT,
    UNIQUE(run_id, confirmation_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_run_confirmations_pending
    ON agent_run_confirmations(run_id, status, expires_at);
