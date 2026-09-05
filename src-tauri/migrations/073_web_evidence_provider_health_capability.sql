-- 073: keep independent routing health for search and page fetch.
--
-- Older rows mixed both capabilities under one provider id. Retain them as
-- legacy diagnostics, but do not allow new routing decisions to read them.

PRAGMA foreign_keys=OFF;

CREATE TABLE web_evidence_provider_health_new (
    provider_id TEXT NOT NULL REFERENCES web_evidence_providers(id) ON DELETE CASCADE,
    capability TEXT NOT NULL CHECK (capability IN ('web.search', 'web.fetch', 'legacy')),
    success_count INTEGER NOT NULL DEFAULT 0 CHECK (success_count >= 0),
    failure_count INTEGER NOT NULL DEFAULT 0 CHECK (failure_count >= 0),
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    latency_ewma_ms REAL,
    success_ewma REAL,
    last_failure_code TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (provider_id, capability)
);

INSERT INTO web_evidence_provider_health_new
    (provider_id, capability, success_count, failure_count, consecutive_failures,
     latency_ewma_ms, success_ewma, last_failure_code, updated_at)
SELECT provider_id, 'legacy', success_count, failure_count, consecutive_failures,
       latency_ewma_ms, success_ewma, last_failure_code, updated_at
FROM web_evidence_provider_health;

DROP TABLE web_evidence_provider_health;
ALTER TABLE web_evidence_provider_health_new RENAME TO web_evidence_provider_health;

CREATE INDEX idx_web_evidence_provider_health_capability_updated
    ON web_evidence_provider_health(capability, updated_at DESC);

PRAGMA foreign_keys=ON;
