-- 062: `links` is the sole derived note-link index. `block_links` duplicated
-- that relationship, while search results no longer use `search_cache`.
-- Both tables contain disposable derived state and can be removed safely.

DROP TABLE IF EXISTS block_links;
DROP TABLE IF EXISTS search_cache;
