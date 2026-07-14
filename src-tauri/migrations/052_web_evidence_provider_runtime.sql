-- 052: non-sensitive runtime state for the deliberately narrow MCP web
-- evidence surface.  Tool payloads and credentials must never be persisted.
CREATE TABLE IF NOT EXISTS web_evidence_provider_runtime (
    provider_id TEXT PRIMARY KEY REFERENCES web_evidence_providers(id) ON DELETE CASCADE,
    protocol_version TEXT NOT NULL,
    server_name TEXT NOT NULL,
    server_version TEXT,
    capabilities_hash TEXT NOT NULL,
    mapping_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('unknown', 'ready', 'degraded', 'unavailable', 'blocked')),
    reason_code TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS web_evidence_provider_health (
    provider_id TEXT PRIMARY KEY REFERENCES web_evidence_providers(id) ON DELETE CASCADE,
    success_count INTEGER NOT NULL DEFAULT 0 CHECK (success_count >= 0),
    failure_count INTEGER NOT NULL DEFAULT 0 CHECK (failure_count >= 0),
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    latency_ewma_ms REAL,
    success_ewma REAL,
    last_failure_code TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_web_evidence_provider_runtime_status
    ON web_evidence_provider_runtime(status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_web_evidence_provider_health_updated
    ON web_evidence_provider_health(updated_at DESC);
