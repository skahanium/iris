-- 063_feed_library.sql — RSS 订阅资料库（阶段 1）
-- 应用级事实表：订阅源配置与订阅文章快照；派生 FTS 与触发器。
-- 笔记索引层与订阅资料库互相独立；本 migration 不触碰任何既有表。

CREATE TABLE feed_sources (
    id TEXT PRIMARY KEY,
    feed_url TEXT NOT NULL UNIQUE,
    site_url TEXT,
    title TEXT NOT NULL,
    title_override TEXT,
    description TEXT,
    icon_url TEXT,
    language TEXT,
    folder_path TEXT NOT NULL DEFAULT '',
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    fetch_interval_minutes INTEGER NOT NULL DEFAULT 60
        CHECK (fetch_interval_minutes BETWEEN 15 AND 10080),
    etag TEXT,
    last_modified TEXT,
    last_checked_at TEXT,
    last_success_at TEXT,
    next_fetch_at TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    last_error_code TEXT,
    last_error_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE feed_items (
    row_id INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    source_id TEXT NOT NULL REFERENCES feed_sources(id) ON DELETE CASCADE,
    external_key TEXT NOT NULL,
    canonical_url TEXT,
    title TEXT NOT NULL,
    author_name TEXT,
    published_at TEXT,
    source_updated_at TEXT,
    received_at TEXT NOT NULL,
    summary_markdown TEXT NOT NULL DEFAULT '',
    content_markdown TEXT NOT NULL,
    content_text TEXT NOT NULL,
    source_payload TEXT NOT NULL,
    source_payload_kind TEXT NOT NULL
        CHECK (source_payload_kind IN ('html', 'xhtml', 'text', 'markdown')),
    content_hash TEXT NOT NULL,
    conversion_version INTEGER NOT NULL,
    conversion_status TEXT NOT NULL
        CHECK (conversion_status IN ('ok', 'degraded')),
    read_at TEXT,
    starred_at TEXT,
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (source_id, external_key)
);

CREATE INDEX idx_feed_sources_due
    ON feed_sources(is_enabled, next_fetch_at);
CREATE INDEX idx_feed_sources_folder
    ON feed_sources(folder_path, title);
CREATE INDEX idx_feed_items_inbox
    ON feed_items(archived_at, read_at, received_at DESC, row_id DESC);
CREATE INDEX idx_feed_items_source_time
    ON feed_items(source_id, received_at DESC, row_id DESC);
CREATE INDEX idx_feed_items_starred
    ON feed_items(starred_at DESC) WHERE starred_at IS NOT NULL;
CREATE INDEX idx_feed_items_archived
    ON feed_items(archived_at DESC) WHERE archived_at IS NOT NULL;

CREATE VIRTUAL TABLE feed_items_fts USING fts5(
    title,
    author_name,
    content_text,
    content='feed_items',
    content_rowid='row_id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER feed_items_fts_ai AFTER INSERT ON feed_items BEGIN
    INSERT INTO feed_items_fts(rowid, title, author_name, content_text)
    VALUES (new.row_id, new.title, COALESCE(new.author_name, ''), new.content_text);
END;
CREATE TRIGGER feed_items_fts_ad AFTER DELETE ON feed_items BEGIN
    INSERT INTO feed_items_fts(feed_items_fts, rowid, title, author_name, content_text)
    VALUES ('delete', old.row_id, old.title, COALESCE(old.author_name, ''), old.content_text);
END;
CREATE TRIGGER feed_items_fts_au AFTER UPDATE OF title, author_name, content_text ON feed_items BEGIN
    INSERT INTO feed_items_fts(feed_items_fts, rowid, title, author_name, content_text)
    VALUES ('delete', old.row_id, old.title, COALESCE(old.author_name, ''), old.content_text);
    INSERT INTO feed_items_fts(rowid, title, author_name, content_text)
    VALUES (new.row_id, new.title, COALESCE(new.author_name, ''), new.content_text);
END;
