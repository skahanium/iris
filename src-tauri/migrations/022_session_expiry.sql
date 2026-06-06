-- 022: Session & trace expiry indexes for scheduled cleanup.
-- No schema changes; only adds indexes to support efficient
-- purging of stale sessions and ai_traces by date.

CREATE INDEX IF NOT EXISTS idx_sessions_updated_at
    ON sessions(updated_at);

CREATE INDEX IF NOT EXISTS idx_ai_traces_created_prune
    ON ai_traces(created_at);
