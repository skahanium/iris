ALTER TABLE llm_model_registry RENAME TO llm_model_registry_legacy_slots;

CREATE TABLE llm_model_registry (
    provider_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('built_in', 'provider_discovered', 'manual')),
    stale INTEGER NOT NULL DEFAULT 0 CHECK (stale IN (0, 1)),
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    last_refreshed_at TEXT NOT NULL,
    text_verified_at TEXT,
    vision_verified_at TEXT,
    PRIMARY KEY (provider_id, model_id)
);

INSERT INTO llm_model_registry (
    provider_id, model_id, display_name, source, stale, first_seen_at,
    last_seen_at, last_refreshed_at, text_verified_at, vision_verified_at
)
SELECT
    provider_id, model_id, display_name, source, stale, first_seen_at,
    last_seen_at, last_refreshed_at, text_verified_at, vision_verified_at
FROM llm_model_registry_legacy_slots;

DROP TABLE llm_model_registry_legacy_slots;
CREATE INDEX idx_llm_model_registry_provider ON llm_model_registry(provider_id);
