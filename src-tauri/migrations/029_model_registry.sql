CREATE TABLE IF NOT EXISTS llm_model_registry (
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
    user_confirmed_capabilities TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (provider_id, model_id)
);

CREATE INDEX IF NOT EXISTS idx_llm_model_registry_provider
    ON llm_model_registry(provider_id);
