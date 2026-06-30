PRAGMA foreign_keys=OFF;

CREATE TABLE IF NOT EXISTS mcp_server_catalog_new (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    transport TEXT NOT NULL CHECK (transport IN ('stdio', 'https', 'sse')),
    command TEXT,
    args_json TEXT NOT NULL DEFAULT '[]',
    url TEXT,
    env_schema_json TEXT NOT NULL DEFAULT '{}',
    capability_tags_json TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL DEFAULT 'user',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO mcp_server_catalog_new
    (id, display_name, transport, command, args_json, url, env_schema_json,
     capability_tags_json, source, created_at, updated_at)
SELECT
    id,
    display_name,
    CASE WHEN lower(transport) = 'http' THEN 'https' ELSE lower(transport) END,
    command,
    args_json,
    url,
    env_schema_json,
    capability_tags_json,
    source,
    created_at,
    updated_at
FROM mcp_server_catalog;

DROP TABLE mcp_server_catalog;
ALTER TABLE mcp_server_catalog_new RENAME TO mcp_server_catalog;

PRAGMA foreign_keys=ON;
