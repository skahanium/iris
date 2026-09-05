-- Restore the pre-073 aggregate diagnostic table. Capability-specific health
-- is intentionally folded for rollback only; forward routing never uses it.

PRAGMA foreign_keys=OFF;

CREATE TABLE web_evidence_provider_health_old (
    provider_id TEXT PRIMARY KEY REFERENCES web_evidence_providers(id) ON DELETE CASCADE,
    success_count INTEGER NOT NULL DEFAULT 0 CHECK (success_count >= 0),
    failure_count INTEGER NOT NULL DEFAULT 0 CHECK (failure_count >= 0),
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    latency_ewma_ms REAL,
    success_ewma REAL,
    last_failure_code TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO web_evidence_provider_health_old
    (provider_id, success_count, failure_count, consecutive_failures,
     latency_ewma_ms, success_ewma, last_failure_code, updated_at)
SELECT provider_id,
       SUM(success_count),
       SUM(failure_count),
       MAX(consecutive_failures),
       MAX(latency_ewma_ms),
       MAX(success_ewma),
       MAX(last_failure_code),
       MAX(updated_at)
FROM web_evidence_provider_health
GROUP BY provider_id;

DROP TABLE web_evidence_provider_health;
ALTER TABLE web_evidence_provider_health_old RENAME TO web_evidence_provider_health;

CREATE INDEX idx_web_evidence_provider_health_updated
    ON web_evidence_provider_health(updated_at DESC);

PRAGMA foreign_keys=ON;
