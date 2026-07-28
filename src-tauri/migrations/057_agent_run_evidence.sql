CREATE TABLE IF NOT EXISTS agent_run_evidence (
    run_id              TEXT NOT NULL REFERENCES agent_runs(run_id) ON DELETE CASCADE,
    evidence_id         INTEGER NOT NULL REFERENCES session_evidence(id) ON DELETE CASCADE,
    registration_source TEXT NOT NULL CHECK (registration_source IN ('context', 'web_search')),
    registered_at       TEXT NOT NULL,
    PRIMARY KEY (run_id, evidence_id, registration_source)
);

CREATE INDEX IF NOT EXISTS idx_agent_run_evidence_web_verification
    ON agent_run_evidence(run_id, registration_source, evidence_id);
