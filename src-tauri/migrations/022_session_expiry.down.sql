-- 022 down: remove session & trace expiry indexes.

DROP INDEX IF EXISTS idx_sessions_updated_at;
DROP INDEX IF EXISTS idx_ai_traces_created_prune;
