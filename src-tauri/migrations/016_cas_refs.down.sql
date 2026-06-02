DROP INDEX IF EXISTS idx_cas_ref_links_target;
DROP INDEX IF EXISTS idx_cas_ref_links_source;
DROP INDEX IF EXISTS idx_cas_refs_object_type;
DROP INDEX IF EXISTS idx_cas_refs_ref_count;
DROP TABLE IF EXISTS cas_ref_links;
DROP TABLE IF EXISTS cas_refs;

-- Note: SQLite does not support DROP COLUMN in versions < 3.35.0
-- The cas_hash column in chunks table will remain even after rollback
-- This is acceptable as it's a nullable column with no default value
