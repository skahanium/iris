CREATE VIRTUAL TABLE IF NOT EXISTS files_metadata_fts USING fts5(
    path,
    aliases,
    tags,
    tokenize='unicode61'
);

-- Existing notes are normally content-hash stable, so migration must backfill rather than wait
-- for a later file write. Values mirror the runtime writer: trim, discard empties, sort/dedup.
INSERT INTO files_metadata_fts (path, aliases, tags)
SELECT
    f.path,
    COALESCE((
        SELECT group_concat(alias, ' ')
        FROM (
            SELECT DISTINCT trim(value) AS alias
            FROM json_each(COALESCE(f.frontmatter, '{}'), '$.aliases')
            WHERE type = 'text' AND trim(value) <> ''
            ORDER BY alias
        )
    ), ''),
    COALESCE((
        SELECT group_concat(name, ' ')
        FROM (
            SELECT t.name
            FROM file_tags AS ft
            INNER JOIN tags AS t ON t.id = ft.tag_id
            WHERE ft.file_id = f.id
            ORDER BY t.name
        )
    ), '')
FROM files AS f
WHERE f.path NOT LIKE '.classified/%';